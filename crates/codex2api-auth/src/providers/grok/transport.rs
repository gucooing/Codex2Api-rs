use super::Result;
use codex2api_accounts::providers::grok::GrokIdentity;
use std::sync::Arc;

pub struct GrokHttpClients {
    pub identity: GrokIdentity,
    pub raw: reqwest::Client,
    pub routed_api: reqwest::Client,
    pub refresh_lock: Arc<tokio::sync::Mutex<()>>,
    pub(crate) proxy: Option<String>,
}
impl GrokHttpClients {
    pub fn with_proxy(identity: &GrokIdentity, proxy: Option<&str>) -> Result<Self> {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none());
        if let Some(proxy) = proxy {
            builder = builder.proxy(reqwest::Proxy::all(codex2api_storage::parse_proxy_url(
                proxy,
            )?)?);
        }
        let client = builder.build()?;
        Ok(Self {
            identity: identity.clone(),
            raw: client.clone(),
            routed_api: client,
            refresh_lock: Arc::default(),
            proxy: proxy.map(str::to_owned),
        })
    }
}
