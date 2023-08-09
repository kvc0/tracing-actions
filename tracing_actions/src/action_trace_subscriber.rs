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

pub trait TraceSink {
    fn sink_trace(&self, trace: &mut ActionSpan);
}

pub struct ActionTraceSubscriber<Sink, SpanConstructor, FnShouldRecord> {
    id_counter: AtomicU64,
    current_traces: Mutex<HashMap<span::Id, ActionSpan>>,
    level: Option<Level>,
    active_span_stack: ThreadLocal<Mutex<Vec<span::Id>>>,
    span_sink: Sink,
    span_constructor: SpanConstructor,
    should_record_span: FnShouldRecord,
}

impl<
        Sink: TraceSink,
        TSpanConstructor: SpanConstructor,
        FnShouldRecord: Fn(&mut ActionSpan) -> bool,
    > ActionTraceSubscriber<Sink, TSpanConstructor, FnShouldRecord>
{
    pub fn new(
        level: LevelFilter,
        sink: Sink,
        span_constructor: TSpanConstructor,
        should_record_span: FnShouldRecord,
    ) -> Self {
        Self {
            id_counter: Default::default(),
            current_traces: Default::default(),
            level: level.into_level(),
            active_span_stack: ThreadLocal::new(),
            span_sink: sink,
            span_constructor,
            should_record_span,
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

impl<
        Sink: TraceSink + 'static,
        TSpanConstructor: SpanConstructor + 'static,
        FnShouldRecord: Fn(&mut ActionSpan) -> bool + 'static,
    > Subscriber for ActionTraceSubscriber<Sink, TSpanConstructor, FnShouldRecord>
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
                let parent_result = self.use_span(parent, |parent| {
                    log::debug!("found parent span - starting new child");
                    action_span.start_child(attributes, &parent.trace_id, &parent.span_id)
                });
                if parent_result.is_none() {
                    log::debug!("could not find parent span - starting new root");
                    action_span.start_root(attributes);
                }
            }
            None => {
                if attributes.is_contextual() {
                    let current = self.current_span();
                    match current.id() {
                        Some(current_id) => {
                            match self.use_span(current_id, |current| {
                                action_span.start_child(
                                    attributes,
                                    &current.trace_id,
                                    &current.span_id,
                                );
                            }) {
                                Some(_) => (),
                                None => {
                                    log::debug!("could not find indicated current active span - starting new root");
                                    action_span.start_root(attributes)
                                }
                            }
                        }
                        None => {
                            log::debug!("no current span - starting new root");
                            action_span.start_root(attributes)
                        }
                    }
                } else {
                    log::debug!("no parent span - starting new root");
                    action_span.start_root(attributes)
                }
            }
        }

        self.insert_new_span(id.clone(), action_span);
        id
    }

    fn record(&self, span: &span::Id, values: &span::Record<'_>) {
        self.use_span(span, |span| values.record(span));
    }

    fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let active_span = self
            .active_span_stack
            .get_or_default()
            .lock()
            .expect("threadlocal current")
            .last()
            .cloned();
        active_span.map(|id| self.use_span(&id, |span| span.events.push(ActionEvent::from(event))));
    }

    fn enter(&self, span: &span::Id) {
        let mut active_trace = self
            .active_span_stack
            .get_or_default()
            .lock()
            .expect("threadlocal enter");
        log::trace!(
            "entering span. Current: {:?}, entering: {:?}",
            *active_trace,
            span
        );
        active_trace.push(span.clone());
    }

    fn exit(&self, span: &span::Id) {
        let mut active_span_stack = self
            .active_span_stack
            .get_or_default()
            .lock()
            .expect("threadlocal exit");
        log::trace!(
            "exiting span. Current: {:?}, exiting: {:?}",
            active_span_stack.last(),
            span
        );
        if active_span_stack.last() == Some(span) {
            active_span_stack.pop();
        } else {
            log::trace!(
                "tried to exit non-active span. Current: {:?}, attempted: {:?}",
                *active_span_stack,
                span
            );
        }
    }

    fn current_span(&self) -> tracing_core::span::Current {
        let current = self
            .active_span_stack
            .get_or_default()
            .lock()
            .expect("current trace mutex should not be poisoned")
            .last()
            .cloned();

        match current {
            Some(span) => match self.use_span(&span, |s| s.metadata).unwrap_or_default() {
                Some(metadata) => tracing_core::span::Current::new(span, metadata),
                None => tracing_core::span::Current::none(),
            },
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
                closed_span.end();
                log::trace!("Closed action span: {closed_span:?}");
                if (self.should_record_span)(&mut closed_span) {
                    log::trace!("Action span can be recorded: {closed_span:?}");
                    self.span_sink.sink_trace(&mut closed_span);
                }
                closed_span.reset();
                self.span_constructor.return_span(closed_span);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::{Arc, Mutex};

    use tracing::Instrument;
    use tracing_core::dispatcher::DefaultGuard;

    use crate::{span_constructor::LazySpanCache, ActionSpan, ActionTraceSubscriber, TraceSink};

    struct TestSink {
        spans: Arc<Mutex<Vec<ActionSpan>>>,
    }
    impl TraceSink for TestSink {
        fn sink_trace(&self, action_span: &mut ActionSpan) {
            self.spans
                .lock()
                .expect("local lock should work")
                .push(action_span.clone());
        }
    }

    fn set_up_tracing() -> (DefaultGuard, Arc<Mutex<Vec<ActionSpan>>>) {
        static INITIALIZE_LOGGER_ONCE: std::sync::Once = std::sync::Once::new();
        INITIALIZE_LOGGER_ONCE.call_once(|| {
            env_logger::builder().is_test(true).init();
        });
        let level = "debug".parse().expect("debug is a level filter");
        let spans: Arc<Mutex<Vec<ActionSpan>>> = Default::default();
        let k_logging_subscriber = ActionTraceSubscriber::new(
            level,
            TestSink {
                spans: spans.clone(),
            },
            LazySpanCache::default(),
            move |_| true,
        );
        (
            tracing::subscriber::set_default(k_logging_subscriber),
            spans,
        )
    }

    fn set_up_tracing_should_record<
        FnShouldRecord: Fn(&mut ActionSpan) -> bool + 'static + Send + Sync,
    >(
        should_record: FnShouldRecord,
    ) -> (DefaultGuard, Arc<Mutex<Vec<ActionSpan>>>) {
        let level = "debug".parse().expect("debug is a level filter");
        let spans: Arc<Mutex<Vec<ActionSpan>>> = Default::default();
        let k_logging_subscriber = ActionTraceSubscriber::new(
            level,
            TestSink {
                spans: spans.clone(),
            },
            LazySpanCache::default(),
            should_record,
        );
        (
            tracing::subscriber::set_default(k_logging_subscriber),
            spans,
        )
    }

    #[tokio::test]
    async fn contextual_spans() {
        let (_guard, spans) = set_up_tracing();

        {
            let outer = tracing::info_span!("a root");
            let _guard = outer.enter();

            let inner = tracing::info_span!("a subspan");
            let _g2 = inner.enter();
        }

        let spans: Vec<ActionSpan> = spans.lock().expect("local mutex").clone();
        assert_eq!(2, spans.len());

        let root_span = spans
            .iter()
            .find(|s| s.metadata.expect("there is metadata").name() == "a root")
            .expect("there is a root span");

        let trace = &root_span.trace_id;

        for span in &spans {
            assert_eq!(trace, &span.trace_id);
        }
    }

    #[tokio::test]
    async fn async_contextual_spans() {
        let (_guard, spans) = set_up_tracing();

        async {
            async {}.instrument(tracing::info_span!("a subspan")).await;
        }
        .instrument(tracing::info_span!("a root"))
        .await;

        let spans: Vec<ActionSpan> = spans.lock().expect("local mutex").clone();
        assert_eq!(2, spans.len());

        let root_span = spans
            .iter()
            .find(|s| s.metadata.expect("there is metadata").name() == "a root")
            .expect("there is a root span");

        let trace = &root_span.trace_id;

        for span in &spans {
            assert_eq!(trace, &span.trace_id);
        }
    }

    #[tokio::test]
    async fn async_contextual_spans_sync_within_async() {
        let (_guard, spans) = set_up_tracing();

        async {
            {
                let inner_0 = tracing::info_span!("a synchronous subspan");
                let _inner_0_guard = inner_0.enter();
            }
            {
                let inner_1 = tracing::info_span!("another synchronous subspan");
                let _inner_1_guard = inner_1.enter();
            }
            async {}.instrument(tracing::info_span!("a subspan")).await;
        }
        .instrument(tracing::info_span!("a root"))
        .await;

        let spans: Vec<ActionSpan> = spans.lock().expect("local mutex").clone();
        assert_eq!(4, spans.len());

        let root_span = spans
            .iter()
            .find(|s| s.metadata.expect("there is metadata").name() == "a root")
            .expect("there is a root span");

        let trace = &root_span.trace_id;

        for span in &spans {
            assert_eq!(trace, &span.trace_id);
        }
    }

    #[tokio::test]
    async fn no_spans_recorded() {
        let (_guard, spans) = set_up_tracing_should_record(move |_| false);

        {
            let outer = tracing::info_span!("a root");
            let _guard = outer.enter();

            let inner = tracing::info_span!("a subspan");
            let _g2 = inner.enter();
        }

        let spans: Vec<ActionSpan> = spans.lock().expect("local mutex").clone();
        assert_eq!(0, spans.len());
    }

    #[tokio::test]
    async fn only_spans_with_attribute_recorded() {
        let (_guard, spans) = set_up_tracing_should_record(move |action_span| {
            action_span.attributes.contains_key("foobar")
        });

        // Should not be recorded
        {
            let outer = tracing::info_span!("a root");
            let _guard = outer.enter();

            let inner = tracing::info_span!("a subspan");
            let _g2 = inner.enter();
        }

        {
            // Should be recorded
            let outer = tracing::info_span!("a root", foobar = "42");
            let _guard = outer.enter();

            // Should not be recorded
            let inner = tracing::info_span!("a subspan");
            let _g2 = inner.enter();

            // Should be recorded
            let inner_inner = tracing::info_span!("a subspan", foobar = "24");
            let _g3 = inner_inner.enter();
        }

        let spans: Vec<ActionSpan> = spans.lock().expect("local mutex").clone();
        assert_eq!(2, spans.len());

        let root_span = spans
            .iter()
            .find(|s| s.metadata.expect("there is metadata").name() == "a root")
            .expect("there is a root span");

        let trace = &root_span.trace_id;

        for span in &spans {
            assert_eq!(trace, &span.trace_id);
        }
    }
}
