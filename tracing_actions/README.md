# tracing-actions

Tracing-actions is a `tracing` extension for recording service functions.

It gives you a `tracing` subscriber to which you can supply a callback. The `tracing` crate
is highly general, and that generality is more than some uses need.

If you are trying to send opentelemetry line protocol traces to some service, you need to
materialize whole spans to build up OTLP messages. The `ActionTraceSubscriber` gives you a
callback for each completed span. It was made with OTLP in mind, but `ActionTrace`s can be
used for any number of other destinations.

Spans used by `ActionTraceSubscriber` can be optimistically cached, if you use the `LazySpanCache`.
That feature is a simple best-effort racing cache. If 2 threads need a span at the same instant,
one gets a cached span and the other makes a new one. They both try to return the new span to the
cache upon completion, and if the cache is full the span is simply dropped.
