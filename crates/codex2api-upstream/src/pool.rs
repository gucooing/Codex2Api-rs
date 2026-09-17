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
    clients: Arc<tokio::sync::Mutex<HashMap<String, Arc<UpstreamClient>>>>,
    stream_idle_timeout: Option<Duration>,
}

impl UpstreamPool {
    pub fn new(auth: AuthService) -> Self {
        Self {
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

    /// Return the isolated client for `account_id`, creating it on first use.
    pub async fn get(&self, account_id: &str) -> Result<Arc<UpstreamClient>> {
        let mut map = self.clients.lock().await;
        let clients = self.auth.account_http(account_id).await?;
        if let Some(existing) = map.get(account_id) {
            if Arc::ptr_eq(&existing.account_http, &clients) {
                return Ok(existing.clone());
            }
        }
        let ctx = self.auth.accounts().load_context(account_id).await?;
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
