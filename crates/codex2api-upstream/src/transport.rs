use std::{future::Future, pin::Pin};

pub type HttpFuture = Pin<Box<dyn Future<Output = crate::Result<reqwest::Response>> + Send>>;

/// Network boundary only. Providers still construct and authenticate their requests.
pub trait HttpTransport: Send + Sync {
    fn send(&self, request: reqwest::RequestBuilder) -> HttpFuture;
}

pub struct ReqwestTransport;
impl HttpTransport for ReqwestTransport {
    fn send(&self, request: reqwest::RequestBuilder) -> HttpFuture {
        Box::pin(async move { Ok(request.send().await?) })
    }
}
