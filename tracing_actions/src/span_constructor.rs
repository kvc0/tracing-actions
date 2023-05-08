use std::sync::Mutex;

use crate::ActionSpan;

pub trait SpanConstructor {
    fn new_span(&self) -> ActionSpan;
    fn return_span(&self, span: ActionSpan);
}

pub struct AlwaysNewSpanConstructor;
impl SpanConstructor for AlwaysNewSpanConstructor {
    fn new_span(&self) -> ActionSpan {
        ActionSpan::default()
    }

    fn return_span(&self, _span: ActionSpan) {
        // Drops the span
    }
}

/// Works with shared spans when it wins a race to grab a mutex.
/// When it doesn't get a shared span, it simply allocates a new one.
pub struct LazySpanCache {
    span_cache_size: usize,
    span_cache: Mutex<Vec<ActionSpan>>,
}
impl LazySpanCache {
    pub fn new(span_cache_size: usize) -> Self {
        Self {
            span_cache_size,
            span_cache: Vec::with_capacity(span_cache_size).into(),
        }
    }
}
impl Default for LazySpanCache {
    fn default() -> Self {
        Self::new(64)
    }
}
impl SpanConstructor for LazySpanCache {
    fn new_span(&self) -> ActionSpan {
        match self.span_cache.try_lock() {
            Ok(mut lazy_win) => lazy_win.pop(),
            Err(_) => None,
        }
        .unwrap_or_default()
    }

    fn return_span(&self, span: ActionSpan) {
        if let Ok(mut lazy_win) = self.span_cache.try_lock() {
            if lazy_win.len() < self.span_cache_size {
                lazy_win.push(span)
            }
        }
    }
}
