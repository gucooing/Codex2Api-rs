use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use codex2api_auth::AuthService;

use crate::client::UpstreamClient;
use crate::error::Result;
use crate::headers::RequestHeaders;
use crate::stream::SseForwardStream;

/// Per-account [`UpstreamClient`] cache.
///
/// Cookie jars and HTTP connection pools are never shared across accounts.
#[derive(Clone)]
pub struct UpstreamPool {
    auth: AuthService,
    grok: crate::grok::GrokUpstream,
    clients: Arc<tokio::sync::Mutex<HashMap<String, Arc<UpstreamClient>>>>,
    stream_idle_timeout: Option<Duration>,
}

impl UpstreamPool {
    pub fn new(auth: AuthService) -> Self {
        Self {
            grok: crate::grok::GrokUpstream::new(codex2api_auth::grok::GrokAuthService::new(
                auth.storage().ok().cloned(),
            )),
            auth,
            clients: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            stream_idle_timeout: None,
        }
    }

    pub fn with_stream_idle_timeout(mut self, timeout: Duration) -> Self {
        self.stream_idle_timeout = Some(timeout);
        self
    }

    pub fn auth(&self) -> &AuthService {
        &self.auth
    }
    pub fn grok(&self) -> &crate::grok::GrokUpstream {
        &self.grok
    }
    pub fn with_grok(mut self, grok: crate::grok::GrokUpstream) -> Self {
        self.grok = grok;
        self
    }
    pub async fn refresh_supplier_auth(
        &self,
        account: &codex2api_storage::SupplierAccount,
        rejected: &str,
    ) -> Result<()> {
        match account.provider_id.as_str() {
            codex2api_core::CHATGPT => {
                self.auth
                    .refresh_rejected_token(&account.id, rejected)
                    .await?;
            }
            codex2api_core::GROK => {
                self.grok
                    .auth()
                    .refresh_grok(&account.id, true, Some(rejected))
                    .await?;
            }
            _ => {
                return Err(crate::UpstreamError::InvalidRequest(
                    "Unsupported provider".into(),
                ));
            }
        }
        Ok(())
    }

    /// Return the isolated client for `account_id`, creating it on first use.
    pub async fn get(&self, account_id: &str) -> Result<Arc<UpstreamClient>> {
        if self
            .auth
            .storage()?
            .require_account(account_id)
            .await?
            .provider_id
            != codex2api_core::CHATGPT
        {
            return Err(crate::UpstreamError::InvalidRequest(
                "Supplier provider mismatch".into(),
            ));
        }
        let mut map = self.clients.lock().await;
        let clients = self.auth.account_http(account_id).await?;
        if let Some(existing) = map.get(account_id)
            && Arc::ptr_eq(&existing.account_http, &clients)
        {
            return Ok(existing.clone());
        }
        let ctx = self.auth.accounts().load_context(account_id).await?;
        if ctx.account.provider_id != codex2api_core::CHATGPT {
            return Err(crate::UpstreamError::InvalidRequest(
                "Supplier provider mismatch".into(),
            ));
        }
        let mut client = UpstreamClient::from_context(ctx, Some(self.auth.clone()))?;
        client.use_account_http(clients);
        if let Some(timeout) = self.stream_idle_timeout {
            client = client.with_stream_idle_timeout(timeout);
        }
        let client = Arc::new(client);
        map.insert(account_id.to_string(), client.clone());
        Ok(client)
    }

    pub async fn evict(&self, account_id: &str) {
        self.clients.lock().await.remove(account_id);
        self.grok.auth().evict_account_http(account_id).await;
    }

    pub async fn stream_responses(
        &self,
        account_id: &str,
        body: &Value,
        extra: RequestHeaders,
    ) -> Result<SseForwardStream> {
        self.get(account_id)
            .await?
            .stream_responses(body, extra)
            .await
    }

    pub async fn post_responses(
        &self,
        account_id: &str,
        body: &Value,
        extra: RequestHeaders,
    ) -> Result<reqwest::Response> {
        self.get(account_id)
            .await?
            .post_responses_with(body, extra)
            .await
    }
}
