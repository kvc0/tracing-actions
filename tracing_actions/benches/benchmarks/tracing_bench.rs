use criterion::{black_box, criterion_group, Criterion};
use tracing::{metadata::LevelFilter, Instrument, Level};
use tracing_actions::{
    span_constructor::{always_record, AlwaysNewSpanConstructor, LazySpanCache},
    ActionTraceSubscriber, TraceSink,
};

struct NoSink;
impl TraceSink for NoSink {
    fn sink_trace(&self, _trace: &mut tracing_actions::ActionSpan) {}
}

fn trace(c: &mut Criterion) {
    let mut group = c.benchmark_group("Traces");

    let actions = ActionTraceSubscriber::new(
        LevelFilter::DEBUG,
        NoSink,
        AlwaysNewSpanConstructor,
        always_record,
    );

    tracing::subscriber::with_default(actions, || {
        group.bench_function("always new span", |bencher| {
            bencher.iter(|| {
                let span = tracing::span!(Level::INFO, "bench");
                let _guard = black_box(span.enter());
                span.record("some", 42);
                {
                    let child_span = tracing::span!(parent: &span, Level::DEBUG, "subspan");
                    let _a = async {}.instrument(child_span);
                }
            })
        });
    });

    let actions = ActionTraceSubscriber::new(
        LevelFilter::DEBUG,
        NoSink,
        LazySpanCache::default(),
        always_record,
    );

    tracing::subscriber::with_default(actions, || {
        group.bench_function("default span cache", |bencher| {
            bencher.iter(|| {
                let span = tracing::span!(Level::INFO, "bench");
                let _guard = black_box(span.enter());
                span.record("some", 42);
                {
                    let child_span = tracing::span!(parent: &span, Level::DEBUG, "subspan");
                    let _a = async {}.instrument(child_span);
                }
            })
        });
    });
}

criterion_group!(benches, trace);
