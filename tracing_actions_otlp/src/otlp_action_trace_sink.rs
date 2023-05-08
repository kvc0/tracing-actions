use std::{
    error::Error,
    mem,
    sync::{Arc, Mutex, MutexGuard},
    time::SystemTime,
};

use tracing_actions::{ActionEvent, AttributeValue, SpanStatus, TraceKind, TraceSink};

const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");

use crate::{
    channel_connection::{default_trust_store, get_channel, ChannelType, insecure_trust_store},
    proto::opentelemetry::{
        collector::trace::v1::{
            trace_service_client::TraceServiceClient, ExportTraceServiceRequest,
        },
        common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue},
        trace::v1::{
            span::{self, Event},
            status::StatusCode,
            ResourceSpans, ScopeSpans, Span, Status,
        },
    },
};

pub trait RequestInterceptor: Send + Sync {
    /// Put your request metadata in here if you have need
    fn intercept_request(&self, _request: &mut tonic::Request<ExportTraceServiceRequest>) {}
}

pub struct OtlpActionTraceSink {
    client: TraceServiceClient<ChannelType>,
    interceptors: Arc<Option<Box<dyn RequestInterceptor>>>,
    batch: Mutex<Vec<Span>>,
    batch_size: usize,
}

impl OtlpActionTraceSink {
    pub fn new(
        otlp_endpoint: &str,
        interceptors: Option<Box<dyn RequestInterceptor>>,
        batch_size: usize,
        insecure: bool,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            client: get_channel(
                otlp_endpoint.parse()?,
                if insecure {
                    insecure_trust_store
                } else {
                    default_trust_store
                },
                TraceServiceClient::with_origin,
            ),
            interceptors: interceptors.into(),
            batch_size,
            batch: Mutex::new(Vec::with_capacity(batch_size)),
        })
    }

    pub fn drain_batch(&self) {
        let spans = self.batch.lock().expect("lock should not be poisoned");
        if spans.len() == 0 {
            return;
        }
        self.send_batch(spans)
    }

    fn send_batch(&self, mut current_batch: MutexGuard<Vec<Span>>) {
        let new_buffer = Vec::with_capacity(self.batch_size);
        let batch = mem::replace(&mut *current_batch, new_buffer);
        drop(current_batch);

        let batch_client = self.client.clone();
        let batch_interceptors = self.interceptors.clone();
        tokio::spawn(send_batch(batch, batch_client, batch_interceptors));
    }

    fn add_span_to_batch(&self, span: Span) {
        let mut spans = self.batch.lock().expect("lock should not be poisoned");
        spans.push(span);
        if self.batch_size <= spans.len() {
            self.send_batch(spans)
        }
    }
}

async fn send_batch(
    batch: Vec<Span>,
    mut client: TraceServiceClient<ChannelType>,
    interceptors: Arc<Option<Box<dyn RequestInterceptor>>>,
) {
    let mut request = tonic::Request::new(ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: None,
            scope_spans: vec![ScopeSpans {
                scope: Some(InstrumentationScope {
                    name: "tracing-actions".to_string(),
                    version: VERSION.unwrap_or("unknown").to_string(),
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                spans: batch,
                schema_url: "".to_string(),
            }],
            schema_url: Default::default(),
        }],
    });
    log::trace!("sending batch: {request:#?}");
    if let Some(interceptor) = interceptors.as_ref() {
        interceptor.intercept_request(&mut request);
    }
    match client.export(request).await {
        Ok(response) => {
            if !response.metadata().is_empty() {
                log::info!(
                    "received metadata from trace request: {:?}",
                    response.metadata()
                )
            }
            let inner = response.into_inner();
            if let Some(partial) = inner.partial_success {
                log::warn!("partial trace report: {partial:#?}")
            }
        }
        Err(error) => {
            log::error!("failed to send traces: {error:?}")
        }
    }
}

impl TraceSink for OtlpActionTraceSink {
    fn sink_trace(&self, trace: &mut tracing_actions::ActionSpan) {
        self.add_span_to_batch(trace.into())
    }
}

impl From<&mut tracing_actions::ActionSpan> for Span {
    fn from(value: &mut tracing_actions::ActionSpan) -> Self {
        Self {
            trace_id: value.trace_id.to_vec(),
            span_id: value.span_id.to_vec(),
            trace_state: value.trace_state.clone(),
            parent_span_id: value
                .parent_span_id
                .map(|id| id.to_vec())
                .unwrap_or_default(),
            name: value
                .metadata
                .map(|m| m.name().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            kind: as_spankind(value.kind) as i32,
            start_time_unix_nano: value
                .start
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            end_time_unix_nano: value
                .end
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            attributes: value.attributes.drain().map(KeyValue::from).collect(),
            dropped_attributes_count: 0,
            events: value.events.drain(..).map(Event::from).collect(),
            dropped_events_count: 0,
            links: vec![],
            dropped_links_count: 0,
            status: Some(value.status.into()),
        }
    }
}

fn as_spankind(value: TraceKind) -> span::SpanKind {
    match value {
        TraceKind::Client => span::SpanKind::Client,
        TraceKind::Server => span::SpanKind::Server,
    }
}

impl From<(&str, AttributeValue)> for KeyValue {
    fn from(value: (&str, AttributeValue)) -> Self {
        let (name, value) = value;
        Self {
            key: name.to_string(),
            value: Some(AnyValue {
                value: Some(match value {
                    AttributeValue::String(s) => any_value::Value::StringValue(s),
                    AttributeValue::F64(f) => any_value::Value::DoubleValue(f),
                    AttributeValue::I64(i) => any_value::Value::IntValue(i),
                    AttributeValue::U64(u) => any_value::Value::IntValue(u as i64),
                    AttributeValue::I128(i) => any_value::Value::IntValue(i as i64),
                    AttributeValue::U128(u) => any_value::Value::IntValue(u as i64),
                    AttributeValue::Bool(b) => any_value::Value::BoolValue(b),
                    AttributeValue::Error(e) => any_value::Value::StringValue(e),
                }),
            }),
        }
    }
}

impl From<ActionEvent> for Event {
    fn from(mut value: ActionEvent) -> Self {
        Self {
            time_unix_nano: value
                .timestamp
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            name: value.metadata.name().to_string(),
            attributes: value.attributes.drain().map(KeyValue::from).collect(),
            dropped_attributes_count: 0,
        }
    }
}

impl From<SpanStatus> for Status {
    fn from(value: SpanStatus) -> Self {
        match value {
            SpanStatus::Unset => Self {
                message: "traces should set status".to_string(),
                code: StatusCode::Unset.into(),
            },
            SpanStatus::Ok => Self {
                message: "".to_string(),
                code: StatusCode::Ok.into(),
            },
            SpanStatus::Error => Self {
                message: "".to_string(),
                code: StatusCode::Error.into(),
            },
        }
    }
}
