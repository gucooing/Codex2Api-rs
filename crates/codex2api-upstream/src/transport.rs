use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use codex2api_storage::Storage;

tokio::task_local! {
    static ENDPOINT_LOG: RequestEndpoint;
}

/// The last HTTP path actually attempted on the current supplier. Observe the
/// built request, including its Base URL path, without storing its host or query.
#[derive(Clone)]
pub struct RequestEndpoint {
    storage: Storage,
    usage_id: String,
    latest: Arc<Mutex<Option<String>>>,
}

impl RequestEndpoint {
    pub fn new(storage: Storage, usage_id: String) -> Self {
        Self {
            storage,
            usage_id,
            latest: Arc::default(),
        }
    }

    pub fn latest(&self) -> Option<String> {
        self.latest.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        *self.latest.lock().unwrap() = None;
    }

    pub async fn scope<F: Future>(&self, work: F) -> F::Output {
        ENDPOINT_LOG.scope(self.clone(), work).await
    }
}

/// Persist the destination before sending so in-progress, failed and retried
/// attempts all report the same path as the transport, even without a response.
pub(crate) async fn send(
    transport: &dyn HttpTransport,
    request: reqwest::RequestBuilder,
) -> crate::Result<reqwest::Response> {
    let Some(log) = ENDPOINT_LOG.try_with(Clone::clone).ok() else {
        return transport.send(request).await;
    };
    let (client, request) = request.build_split();
    let request = request?;
    let path = request.url().path();
    log.storage
        .set_usage_upstream_endpoint(&log.usage_id, path)
        .await?;
    *log.latest.lock().unwrap() = Some(path.to_owned());
    transport
        .send(reqwest::RequestBuilder::from_parts(client, request))
        .await
}

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
