use std::{
    error::Error,
    mem,
    sync::{Arc, Mutex, MutexGuard},
};

use tracing_actions::TraceSink;

const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");

use crate::{
    channel_connection::{default_trust_store, get_channel, insecure_trust_store, ChannelType},
    proto::opentelemetry::{
        collector::trace::v1::{
            trace_service_client::TraceServiceClient, ExportTraceServiceRequest,
        },
        common::v1::InstrumentationScope,
        resource::v1::Resource,
        trace::v1::{ResourceSpans, ScopeSpans, Span},
    },
};

/// Visits each request on its way out; for adding auth headers or what have you.
pub trait RequestInterceptor: Send + Sync {
    /// Put your request metadata in here if you have need
    fn intercept_request(&self, _request: &mut tonic::Request<ExportTraceServiceRequest>) {}
}

#[derive(Debug, Clone)]
pub struct OtlpAttributes {
    pub service_name: String,
    pub other_attributes: Option<Vec<(String, String)>>,
}

/// A bridge from action-trace to batched opentelemetry trace services.
pub struct OtlpActionTraceSink {
    client: TraceServiceClient<ChannelType>,
    interceptors: Arc<Option<Box<dyn RequestInterceptor>>>,
    batch: Mutex<Vec<Span>>,
    batch_size: usize,
    attributes: OtlpAttributes,
}

impl TraceSink for OtlpActionTraceSink {
    fn sink_trace(&self, trace: &mut tracing_actions::ActionSpan) {
        self.send(trace)
    }
}

impl OtlpActionTraceSink {
    pub fn new(
        otlp_endpoint: &str,
        interceptors: Option<Box<dyn RequestInterceptor>>,
        batch_size: usize,
        insecure: bool,
        attributes: OtlpAttributes,
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
            attributes,
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
        tokio::spawn(send_batch(
            batch,
            batch_client,
            batch_interceptors,
            self.attributes.clone(),
        ));
    }

    /// Spans are batched up and sent to your downstream.
    ///
    /// See `send_many()` instead if you have a collection of spans to send.
    /// ```
    /// # use tracing_actions_otlp::OtlpActionTraceSink;
    /// # use tracing_actions::ActionSpan;
    /// # fn f(sink: OtlpActionTraceSink, span: &mut ActionSpan) {
    /// sink.send(span);
    /// # }
    /// ```
    pub fn send(&self, span: impl Into<Span>) {
        // You can implement the conversion to Span for your custom type. See `proto_conversions.rs` for an example.
        let span = span.into();
        let mut spans = self.batch.lock().expect("lock should not be poisoned");
        spans.push(span);
        if self.batch_size <= spans.len() {
            self.send_batch(spans)
        }
    }

    /// Spans are batched up and sent to your downstream.
    ///
    /// Use this over `send()` when you have already collected spans to avoid needless extra synchronization cost.
    /// ```
    /// # use tracing_actions_otlp::OtlpActionTraceSink;
    /// # use tracing_actions::ActionSpan;
    /// # fn f(sink: OtlpActionTraceSink, span1: &mut ActionSpan, span2: &mut ActionSpan) {
    /// let batch = vec![span1, span2];   // Collect a batch somehow
    /// sink.send_many(batch);            // Send them all at once
    /// # }
    /// ```
    pub fn send_many(&self, batch: impl IntoIterator<Item = impl Into<Span>>) {
        let mut spans = self.batch.lock().expect("lock should not be poisoned");
        for span in batch {
            let span = span.into();
            spans.push(span);
            if self.batch_size <= spans.len() {
                self.send_batch(spans);
                // release and re-acquire the lock once per batch to allow other threads to make progress
                spans = self.batch.lock().expect("lock should not be poisoned");
            }
        }
    }
}

async fn send_batch(
    batch: Vec<Span>,
    mut client: TraceServiceClient<ChannelType>,
    interceptors: Arc<Option<Box<dyn RequestInterceptor>>>,
    attributes: OtlpAttributes,
) {
    let mut request = tonic::Request::new(ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: attributes.into(),
                dropped_attributes_count: 0,
            }),
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
