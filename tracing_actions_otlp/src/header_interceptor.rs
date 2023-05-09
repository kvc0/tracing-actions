use tonic::metadata::{AsciiMetadataKey, AsciiMetadataValue};

use crate::RequestInterceptor;

struct HeaderInterceptor {
    headers: Vec<(AsciiMetadataKey, AsciiMetadataValue)>,
}
impl RequestInterceptor for HeaderInterceptor {
    fn intercept_request(
        &self,
        request: &mut tonic::Request<
            crate::proto::opentelemetry::collector::trace::v1::ExportTraceServiceRequest,
        >,
    ) {
        let metadata = request.metadata_mut();
        for (name, value) in &self.headers {
            metadata.insert(name, value.clone());
        }
    }
}

/// A convenience request interceptor for adding metadata.
///
/// List your (header, value) pairs and this interceptor will put them as metadata on all trace RPC calls.
pub fn header_interceptor(
    headers: Vec<(&'static str, String)>,
) -> Option<Box<dyn RequestInterceptor>> {
    Some(Box::new(HeaderInterceptor {
        headers: headers
            .into_iter()
            .map(|(k, v)| {
                (
                    AsciiMetadataKey::from_static(k),
                    v.try_into().expect("headers must be ascii values"),
                )
            })
            .collect(),
    }))
}
