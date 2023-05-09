# tracing-actions-otlp

An extension for tracing-actions that consumes action traces and vends them
to an opentelemetry traces server.

`tracing-actions` is under development and some material details may change. Every effort
will be made to ensure that breaking changes fail at compile time rather than runtime,
so you know what rules change when.

## How this relates to `opentelemetry_otlp`
In short, it doesn't.

`tracing-actions` is less general than `tracing-subscriber` and `opentelemetry_otlp`.
It tracks the latest upstreams - pr's are welcome and promptly addressed. Your service
will not be pinned to an old version of tonic.
