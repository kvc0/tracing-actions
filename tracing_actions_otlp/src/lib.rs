pub mod proto;

mod channel_connection;
mod otlp_action_trace_sink;

pub use otlp_action_trace_sink::{OtlpActionTraceSink, RequestInterceptor};
