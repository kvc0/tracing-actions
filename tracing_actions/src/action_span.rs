use std::{collections::HashMap, time::SystemTime};

use tracing::{field::Visit, span::Attributes, Metadata};

pub trait Resettable {
    fn reset(&mut self);
}

#[derive(Debug, Clone, Copy)]
pub enum TraceKind {
    Client,
    Server,
}
impl Default for TraceKind {
    fn default() -> Self {
        Self::Server
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SpanStatus {
    Ok,
    Error,
}
impl Default for SpanStatus {
    fn default() -> Self {
        Self::Ok
    }
}

#[derive(Debug, Clone)]
pub struct ActionSpan {
    pub ref_count: usize,

    /// A unique identifier for a trace. All spans from the same trace share
    /// the same `trace_id`. The ID is a 16-byte array. An ID with all zeroes is considered invalid.
    pub trace_id: [u8; 16],

    /// A unique identifier for a span within a trace, assigned when the span
    /// is created. The ID is an 8-byte array. An ID with all zeroes is considered invalid.
    pub span_id: [u8; 8],

    /// trace_state conveys information about request position in multiple distributed tracing graphs.
    /// It is a trace_state in w3c-trace-context format: <https://www.w3.org/TR/trace-context/#tracestate-header>
    /// See also <https://github.com/w3c/distributed-tracing> for more details about this field.
    pub trace_state: String,

    /// The `span_id` of this span's parent span. If this is a root span, then this
    /// field must be empty.
    pub parent_span_id: Option<[u8; 8]>,

    /// A description of the span, with its name inside.
    pub metadata: Option<&'static Metadata<'static>>,

    /// Distinguishes between spans generated in a particular context. For example,
    /// two spans with the same name may be distinguished using `CLIENT` (caller)
    /// and `SERVER` (callee) to identify queueing latency associated with the span.
    pub kind: TraceKind,

    /// start_time_unix_nano is the start time of the span. On the client side, this is the time
    /// kept by the local machine where the span execution starts. On the server side, this
    /// is the time when the server's application handler starts running.
    /// Value is UNIX Epoch time in nanoseconds since 00:00:00 UTC on 1 January 1970.
    ///
    /// This field is semantically required and it is expected that end_time >= start_time.
    pub start: SystemTime,

    /// end_time_unix_nano is the end time of the span. On the client side, this is the time
    /// kept by the local machine where the span execution ends. On the server side, this
    /// is the time when the server application handler stops running.
    /// Value is UNIX Epoch time in nanoseconds since 00:00:00 UTC on 1 January 1970.
    ///
    /// This field is semantically required and it is expected that end_time >= start_time.
    pub end: SystemTime,

    /// attributes is a collection of key/value pairs.
    ///
    /// The OpenTelemetry API specification further restricts the allowed value types:
    /// <https://github.com/open-telemetry/opentelemetry-specification/blob/main/specification/common/README.md#attribute>
    /// Attribute keys MUST be unique (it is not allowed to have more than one
    /// attribute with the same key).
    pub attributes: HashMap<&'static str, AttributeValue>,

    /// events is a collection of Event items.
    pub events: Vec<ActionEvent>,

    pub status: SpanStatus,
}

impl Default for ActionSpan {
    fn default() -> Self {
        Self {
            ref_count: 0,
            trace_id: Default::default(),
            span_id: Default::default(),
            trace_state: Default::default(),
            parent_span_id: Default::default(),
            metadata: Default::default(),
            kind: Default::default(),
            start: SystemTime::now(),
            end: SystemTime::now(),
            attributes: Default::default(),
            events: Default::default(),
            status: Default::default(),
        }
    }
}

impl Resettable for ActionSpan {
    fn reset(&mut self) {
        self.ref_count = 0;
        self.trace_id.fill(0);
        self.span_id.fill(0);
        self.trace_state = Default::default();
        self.parent_span_id = None;
        self.metadata = Default::default();
        self.kind = Default::default();
        self.attributes.clear();
        self.events.clear();
        self.status = Default::default();
    }
}

impl ActionSpan {
    pub fn start_root(&mut self, attributes: &Attributes) {
        self.trace_id = rand::random();
        self.span_id = rand::random();

        self.start = SystemTime::now();

        self.attach_attributes(attributes);
    }

    pub fn start_child(
        &mut self,
        attributes: &Attributes,
        trace_id: &[u8; 16],
        parent_span_id: &[u8; 8],
    ) {
        self.trace_id.copy_from_slice(trace_id);
        self.span_id = rand::random();
        self.parent_span_id = Some(*parent_span_id); // We can use the Copy trait here

        self.start = SystemTime::now();

        self.attach_attributes(attributes);
    }

    pub fn end(&mut self) {
        self.end = SystemTime::now();
    }

    fn attach_attributes(&mut self, attributes: &Attributes) {
        let metadata = attributes.metadata();
        self.metadata = Some(metadata);
        attributes.values().record(self)
    }
}

#[derive(Debug, Clone)]
pub struct ActionEvent {
    pub metadata: &'static Metadata<'static>,
    pub attributes: HashMap<&'static str, AttributeValue>,
    pub timestamp: SystemTime,
}

impl<'a> From<&'a tracing::Event<'a>> for ActionEvent {
    fn from(event: &'a tracing::Event<'a>) -> Self {
        let mut selff = Self {
            metadata: event.metadata(),
            attributes: HashMap::new(),
            timestamp: SystemTime::now(),
        };
        event.record(&mut selff);
        selff
    }
}

#[derive(Debug, Clone)]
pub enum AttributeValue {
    String(String),
    F64(f64),
    I64(i64),
    U64(u64),
    I128(i128),
    U128(u128),
    Bool(bool),
    Error(String),
}

impl Visit for ActionSpan {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.attributes
            .insert(field.name(), AttributeValue::String(format!("{value:?}")));
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.attributes
            .insert(field.name(), AttributeValue::F64(value));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.attributes
            .insert(field.name(), AttributeValue::I64(value));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.attributes
            .insert(field.name(), AttributeValue::U64(value));
    }

    fn record_i128(&mut self, field: &tracing::field::Field, value: i128) {
        self.attributes
            .insert(field.name(), AttributeValue::I128(value));
    }

    fn record_u128(&mut self, field: &tracing::field::Field, value: u128) {
        self.attributes
            .insert(field.name(), AttributeValue::U128(value));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.attributes
            .insert(field.name(), AttributeValue::Bool(value));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.attributes
            .insert(field.name(), AttributeValue::String(value.to_owned()));
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        // This defaults to ok. If you want to make a span error, you just record at least 1 error on the span.
        self.status = SpanStatus::Error;
        self.attributes
            .insert(field.name(), AttributeValue::Error(format!("{value:?}")));
    }
}

impl Visit for ActionEvent {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.attributes
            .insert(field.name(), AttributeValue::String(format!("{value:?}")));
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.attributes
            .insert(field.name(), AttributeValue::F64(value));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.attributes
            .insert(field.name(), AttributeValue::I64(value));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.attributes
            .insert(field.name(), AttributeValue::U64(value));
    }

    fn record_i128(&mut self, field: &tracing::field::Field, value: i128) {
        self.attributes
            .insert(field.name(), AttributeValue::I128(value));
    }

    fn record_u128(&mut self, field: &tracing::field::Field, value: u128) {
        self.attributes
            .insert(field.name(), AttributeValue::U128(value));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.attributes
            .insert(field.name(), AttributeValue::Bool(value));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.attributes
            .insert(field.name(), AttributeValue::String(value.to_owned()));
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        self.attributes
            .insert(field.name(), AttributeValue::Error(format!("{value:?}")));
    }
}
