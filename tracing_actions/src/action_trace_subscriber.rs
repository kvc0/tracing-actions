use std::{
    collections::HashMap,
    sync::{atomic::AtomicU64, Mutex},
};

use thread_local::ThreadLocal;
use tracing::{metadata::LevelFilter, span, Level, Subscriber};

use crate::{
    action_span::{ActionEvent, Resettable},
    span_constructor::SpanConstructor,
    ActionSpan,
};

pub trait SinkFunction: Fn(&mut ActionSpan) {}

impl<T> SinkFunction for T where T: Fn(&mut ActionSpan) {}

pub struct ActionTraceSubscriber<Sink, SpanConstructor> {
    id_counter: AtomicU64,
    current_traces: Mutex<HashMap<span::Id, ActionSpan>>,
    level: Option<Level>,
    active_trace: ThreadLocal<Mutex<Option<span::Id>>>,
    span_sink: Sink,
    span_constructor: SpanConstructor,
}

impl<Sink: SinkFunction, TSpanConstructor: SpanConstructor>
    ActionTraceSubscriber<Sink, TSpanConstructor>
{
    pub fn new(level: LevelFilter, sink: Sink, span_constructor: TSpanConstructor) -> Self {
        Self {
            id_counter: Default::default(),
            current_traces: Default::default(),
            level: level.into_level(),
            active_trace: ThreadLocal::new(),
            span_sink: sink,
            span_constructor,
        }
    }

    fn insert_new_span(&self, id: span::Id, mut action_span: ActionSpan) {
        action_span.ref_count = 1; // New spans are always inserted with 1
        let mut traces = self
            .current_traces
            .lock()
            .expect("trace mutex should not be poisoned");
        traces.insert(id, action_span);
    }

    fn use_span<T>(&self, id: &span::Id, use_it: impl FnOnce(&mut ActionSpan) -> T) -> Option<T> {
        let mut traces = self
            .current_traces
            .lock()
            .expect("trace mutex should not be poisoned");

        traces.get_mut(id).map(use_it)
    }

    fn possibly_remove_span(
        &self,
        id: &span::Id,
        use_it: impl FnOnce(&mut ActionSpan) -> bool,
    ) -> Option<ActionSpan> {
        let mut traces = self
            .current_traces
            .lock()
            .expect("trace mutex should not be poisoned");

        match traces.get_mut(id).map(use_it) {
            Some(remove_it) => {
                if remove_it {
                    traces.remove(id)
                } else {
                    None
                }
            }
            None => None,
        }
    }
}

impl<Sink: SinkFunction + 'static, TSpanConstructor: SpanConstructor + 'static> Subscriber
    for ActionTraceSubscriber<Sink, TSpanConstructor>
{
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        match &self.level {
            Some(level) => metadata.level() <= level,
            None => false,
        }
    }

    fn new_span(&self, attributes: &span::Attributes<'_>) -> span::Id {
        let mut id = self
            .id_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // ...tracing ids are not allowed to be 0 so we have to do this check always if we want to use
        // nice cheap atomic increments.
        while id == 0 {
            id = self
                .id_counter
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        log::debug!("new span: {id} - {attributes:?}");

        let id = span::Id::from_u64(id);
        let mut action_span = self.span_constructor.new_span();
        match attributes.parent() {
            Some(parent) => {
                if self
                    .use_span(parent, |parent| {
                        action_span.start_child(attributes, &parent.trace_id, &parent.span_id)
                    })
                    .is_none()
                {
                    log::debug!("could not find parent span");
                    action_span.start_root(attributes);
                }
            }
            None => action_span.start_root(attributes),
        }

        self.insert_new_span(id.clone(), action_span);
        id
    }

    fn record(&self, span: &span::Id, values: &span::Record<'_>) {
        self.use_span(span, |span| values.record(span));
    }

    fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let active_trace = self
            .active_trace
            .get_or_default()
            .lock()
            .expect("threadlocal current")
            .clone();
        active_trace
            .map(|id| self.use_span(&id, |span| span.events.push(ActionEvent::from(event))));
        log::debug!("received unsupported event: {event:?}");
    }

    fn enter(&self, span: &span::Id) {
        let mut active_trace = self
            .active_trace
            .get_or_default()
            .lock()
            .expect("threadlocal enter");
        *active_trace = Some(span.clone());
    }

    fn exit(&self, span: &span::Id) {
        let mut active_trace = self
            .active_trace
            .get_or_default()
            .lock()
            .expect("threadlocal exit");
        if active_trace.as_ref() == Some(span) {
            *active_trace = None;
        } else {
            log::warn!(
                "tried to exit non-active span. Current: {:?}, attempted: {:?}",
                *active_trace,
                span
            );
        }
    }

    fn current_span(&self) -> tracing_core::span::Current {
        let current = self
            .active_trace
            .get_or_default()
            .lock()
            .expect("current trace mutex should not be poisoned")
            .clone();

        match current {
            Some(span) => {
                match self.use_span(&span, |s| s.metadata).unwrap_or_default() {
                    Some(metadata) => tracing_core::span::Current::new(span, metadata),
                    None => tracing_core::span::Current::none(),
                }
            }
            None => tracing_core::span::Current::none(),
        }
    }

    fn clone_span(&self, id: &span::Id) -> span::Id {
        self.use_span(id, |span| span.ref_count += 1);
        id.clone()
    }

    fn try_close(&self, id: span::Id) -> bool {
        let closed_span = self.possibly_remove_span(&id, |span| {
            span.ref_count -= 1;
            span.ref_count == 0
        });
        match closed_span {
            Some(mut closed_span) => {
                (self.span_sink)(&mut closed_span);
                closed_span.reset();
                self.span_constructor.return_span(closed_span);
                true
            }
            None => false,
        }
    }
}
