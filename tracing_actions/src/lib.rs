//! A measured, convenient approach to building traces.
//!
//! [`tracing-actions`] is a trace recording toolbox.
//! It records your live spans on the heap and offers you a visitor function
//! which is called once per span with the ActionSpan.
//!
//! Action traces, being heap-allocated, are held in an object pool to mitigate
//! the cost of allocations. While low overhead is a goal of tracing-actions,
//! 0-overhead is not. This is a tool for convenience first and performance second.
//!
//! # Examples
//!
//! ## K-log
//! ```rust
//! use tracing_actions;
//! use log;
//!
//! // First, we implement k-logging.
//! struct KLog {
//!   k: usize,
//!   n: std::sync::atomic::AtomicUsize,
//! }
//! impl tracing_actions::TraceSink for KLog {
//!     fn sink_trace(&self, action_span: &mut tracing_actions::ActionSpan) {
//!         if self.n.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % self.k == 0 {
//!             log::info!("trace: {action_span:?}")
//!         }
//!     }
//! }
//!
//! // Next, we configure a subscriber.
//! let level = "debug".parse().unwrap();
//! let k_logging_subscriber = tracing_actions::ActionTraceSubscriber::new(
//!     level,
//!     KLog { k: 42, n: Default::default() },
//!     tracing_actions::span_constructor::LazySpanCache::default(),
//! );
//!
//! // Finally, we install the subscriber.
//! tracing::subscriber::set_global_default(k_logging_subscriber)
//!     .expect("I should be able to set the global trace subscriber");
//!
//! // Now the rest of your application will k-log ActionSpans.
//! ```
//!

mod action_span;
mod action_trace_subscriber;

pub mod span_constructor;

pub use action_span::ActionEvent;
pub use action_span::ActionSpan;
pub use action_span::AttributeValue;
pub use action_span::SpanStatus;
pub use action_span::TraceKind;
pub use action_trace_subscriber::ActionTraceSubscriber;
pub use action_trace_subscriber::TraceSink;
