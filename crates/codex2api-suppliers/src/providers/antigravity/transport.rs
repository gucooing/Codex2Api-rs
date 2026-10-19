use super::{AntigravityIdentity, Result};
use std::sync::Arc;

/// One HTTP client per supplier account; connection pools are never shared.
pub struct AntigravityHttpClients {
    pub identity: AntigravityIdentity,
    pub client: reqwest::Client,
    pub refresh_lock: Arc<tokio::sync::Mutex<()>>,
    pub(crate) proxy: Option<String>,
}
impl AntigravityHttpClients {
    pub fn with_proxy(identity: &AntigravityIdentity, proxy: Option<&str>) -> Result<Self> {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none());
        if let Some(proxy) = proxy {
            builder = builder.proxy(reqwest::Proxy::all(codex2api_storage::parse_proxy_url(
                proxy,
            )?)?);
        }
        Ok(Self {
            identity: identity.clone(),
            client: builder.build()?,
            refresh_lock: Arc::default(),
            proxy: proxy.map(str::to_owned),
        })
    }
}
