use std::sync::Arc;

use hyper::{client::HttpConnector, Uri};
use tokio_rustls::rustls::{client::ServerCertVerifier, ClientConfig, RootCertStore};

pub type ChannelType = hyper::Client<
    hyper_rustls::HttpsConnector<HttpConnector>,
    http_body::combinators::UnsyncBoxBody<hyper::body::Bytes, tonic::Status>,
>;

/// You can make an insecure connection by passing `|| { None }` to tls_trust.
/// If you want to make a safer connection you can add your trust roots,
/// for example:
/// ```rust
///  || {
///     let mut store = tokio_rustls::rustls::RootCertStore::empty();
///     store.add_server_trust_anchors(
///         webpki_roots::TLS_SERVER_ROOTS.0.iter().map(|trust_anchor| {
///             tokio_rustls::rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
///                 trust_anchor.subject,
///                 trust_anchor.spki,
///                 trust_anchor.name_constraints
///             )
///         })
///     );
///     Some(store)
/// }
/// ;
/// ```
pub fn get_channel<TrustFunction, ClientConstructor, B, U>(
    endpoint: Uri,
    tls_trust: TrustFunction,
    construct_client: ClientConstructor,
) -> U
where
    TrustFunction: FnOnce() -> Option<RootCertStore>,
    ClientConstructor:
        FnOnce(hyper::Client<hyper_rustls::HttpsConnector<HttpConnector>, B>, Uri) -> U,
    B: hyper::body::HttpBody + Send,
    B::Data: Send,
{
    let tls = ClientConfig::builder().with_safe_defaults();
    let tls = match tls_trust() {
        Some(trust) => tls.with_root_certificates(trust).with_no_client_auth(),
        None => {
            let mut config = tls
                .with_root_certificates(RootCertStore::empty())
                .with_no_client_auth();
            config
                .dangerous()
                .set_certificate_verifier(Arc::new(NotAVerifier));
            config
        }
    };

    let mut http_connector = HttpConnector::new();
    http_connector.enforce_http(false);

    // We have to do some wrapping here to map the request type from
    // `https://example.com` -> `https://[::1]:50051` because `rustls`
    // doesn't accept ip's as `ServerName`.
    let https_connector = tower::ServiceBuilder::new()
        .layer_fn(move |http_connector| {
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_tls_config(tls.clone())
                .https_or_http()
                .enable_http2()
                .wrap_connector(http_connector)
        })
        .service(http_connector);

    construct_client(hyper::Client::builder().build(https_connector), endpoint)
}

pub fn default_trust_store() -> Option<RootCertStore> {
    let mut store = tokio_rustls::rustls::RootCertStore::empty();
    store.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(|trust_anchor| {
        tokio_rustls::rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
            trust_anchor.subject,
            trust_anchor.spki,
            trust_anchor.name_constraints,
        )
    }));
    Some(store)
}

struct NotAVerifier;

impl ServerCertVerifier for NotAVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &tokio_rustls::rustls::Certificate,
        _intermediates: &[tokio_rustls::rustls::Certificate],
        _server_name: &tokio_rustls::rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<tokio_rustls::rustls::client::ServerCertVerified, tokio_rustls::rustls::Error> {
        // roflmao
        Ok(tokio_rustls::rustls::client::ServerCertVerified::assertion())
    }
}
