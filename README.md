Tracing-actions is a `tracing` extension for recording service functions.

It seeks to ease integrating tracing into your trace infrastructure.

# Getting started

## A full usage of of opentelemetry downstream
```rust
// First, we set up our trace sink.
let batch_size = 1024;
let secure = true;
let otlp_sink = tracing_actions_otlp::OtlpActionTraceSink::new(
    "https://ingest.lightstep.com:443",
    tracing_actions_otlp::header_interceptor(vec![
        ("lightstep-access-token", std::env::var("token").unwrap_or_else(|_| "none".to_string())
    )]),
    batch_size,
    secure,
    tracing_actions_otlp::OtlpAttributes {
        service_name: "docs-rs example".to_string(),
        other_attributes: None
    },
).expect("should be able to make otlp sink");

// Next, we configure a subscriber (just like any usage of `tracing-actions`)
let level = "debug".parse().unwrap();
let k_logging_subscriber = tracing_actions::ActionTraceSubscriber::new(
    level,
    otlp_sink,
    tracing_actions::span_constructor::LazySpanCache::default(),
);

// Finally, we install the subscriber.
tracing::subscriber::set_global_default(k_logging_subscriber)
    .expect("I should be able to set the global trace subscriber");

// Now the rest of your application will emit ActionSpans as opentelemetry spans to Lightstep.
```

## A custom downstream
First step, implement your custom sink:
```rust
struct KLog {
  k: usize,
  n: std::sync::atomic::AtomicUsize,
}
impl tracing_actions::TraceSink for KLog {
    fn sink_trace(&self, action_span: &mut tracing_actions::ActionSpan) {
        if self.n.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % self.k == 0 {
            log::info!("trace: {action_span:?}")
        }
    }
}
```

Then you hook it up to tracing via the ActionTraceSubscriber.
```rust
// We configure a subscriber.
let level = "debug".parse().unwrap();
let k_logging_subscriber = tracing_actions::ActionTraceSubscriber::new(
    level,
    KLog { k: 42, n: Default::default() },
    tracing_actions::span_constructor::LazySpanCache::default(),
);

// Finally, we install the subscriber.
tracing::subscriber::set_global_default(k_logging_subscriber)
    .expect("I should be able to set the global trace subscriber");

// Now the rest of your application will k-log ActionSpans.
```
