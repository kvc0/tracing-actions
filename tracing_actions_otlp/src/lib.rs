//! A bridge between Rust tracing and opentelemetry traces.
//!
//! [`tracing-actions-otlp`] is a [`tracing-actions`] sink for sending traces
//! in opentelemetry trace format to a listening server.
//! That server might be an opentelemetry collector on your NAS, or a service
//! like Honeycomb or Lightstep.
//!
//! Your batches are built up on your heap from ActionSpans and then sent.
//! There's not a background timer in here to flush your pipeline. If you need
//! to make sure traces are not sitting in a batch for too long you can call
//! drain_batch:
//! ```rust
//! fn periodic_job(otlp_sink: &tracing_actions_otlp::OtlpActionTraceSink) {
//!     otlp_sink.drain_batch();
//! }
//! ```
//!
//! # Examples
//!
//! ## Lightstep
//! ```rust
//! use tracing_actions;
//! use tracing_actions_otlp;
//!
//! // First, we set up our trace sink.
//! let batch_size = 1024;
//! let secure = true;
//! let otlp_sink = tracing_actions_otlp::OtlpActionTraceSink::new(
//!     "https://ingest.lightstep.com:443",
//!     tracing_actions_otlp::header_interceptor(vec![("lightstep-access-token", std::env::var("token").unwrap_or_else(|_| "none".to_string()))]),
//!     batch_size,
//!     secure,
//!     tracing_actions_otlp::OtlpAttributes { service_name: "docs-rs example".to_string(), other_attributes: None }
//! ).expect("should be able to make otlp sink");
//!
//!
//! // Next, we configure a subscriber (just like any usage of `tracing-actions`)
//! let level = "debug".parse().unwrap();
//! let k_logging_subscriber = tracing_actions::ActionTraceSubscriber::new(
//!     level,
//!     otlp_sink,
//!     tracing_actions::span_constructor::LazySpanCache::default(),
//! );
//!
//! // Finally, we install the subscriber.
//! tracing::subscriber::set_global_default(k_logging_subscriber)
//!     .expect("I should be able to set the global trace subscriber");
//!
//! // Now the rest of your application will emit ActionSpans as opentelemetry spans to Lightstep.
//! ```
//!

mod proto;

mod channel_connection;
mod header_interceptor;
mod otlp_action_trace_sink;
mod proto_conversions;

pub use header_interceptor::header_interceptor;
pub use otlp_action_trace_sink::{OtlpActionTraceSink, OtlpAttributes, RequestInterceptor};
