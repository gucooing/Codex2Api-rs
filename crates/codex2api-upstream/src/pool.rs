use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use codex2api_suppliers::AuthService;

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
    antigravity: crate::antigravity::AntigravityUpstream,
    api: crate::api::ApiUpstream,
    clients: Arc<codex2api_suppliers::account_cache::AccountCache<Arc<UpstreamClient>>>,
    stream_idle_timeout: Option<Duration>,
    transport: Arc<dyn crate::HttpTransport>,
}

impl UpstreamPool {
    pub async fn sync_supplier_models(
        &self,
        account: &codex2api_storage::SupplierAccount,
    ) -> Result<()> {
        match account.provider_id.as_str() {
            codex2api_common::CHATGPT => {
                let (models, revision) = self
                    .get(&account.id)
                    .await?
                    .discover_supplier_models()
                    .await?;
                if !self
                    .auth
                    .storage()?
                    .sync_supplier_models(&account.id, revision, &models)
                    .await?
                {
                    return Err(crate::UpstreamError::WorkspaceChanged);
                }
            }
            codex2api_common::GROK => {
                self.grok.discover_supplier_models(&account.id).await?;
            }
            codex2api_common::ANTIGRAVITY => {
                self.antigravity
                    .discover_supplier_models(&account.id)
                    .await?;
            }
            codex2api_common::API => {
                self.api.discover_supplier_models(&account.id).await?;
            }
            _ => {
                return Err(crate::UpstreamError::Internal(
                    "Unsupported provider".into(),
                ));
            }
        }
        Ok(())
    }
    pub fn new(auth: AuthService) -> Self {
        let storage = auth.storage().ok().cloned();
        Self {
            grok: crate::grok::GrokUpstream::new(codex2api_suppliers::grok::GrokAuthService::new(
                storage.clone(),
            )),
            antigravity: crate::antigravity::AntigravityUpstream::new(
                codex2api_suppliers::antigravity::AntigravityAuthService::new(storage.clone()),
            ),
            api: crate::api::ApiUpstream::new(codex2api_suppliers::api::ApiAuthService::new(
                storage,
            )),
            auth,
            clients: Arc::default(),
            stream_idle_timeout: None,
            transport: Arc::new(crate::ReqwestTransport),
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
        self.grok = grok.with_transport(self.transport.clone());
        self
    }
    pub fn antigravity(&self) -> &crate::antigravity::AntigravityUpstream {
        &self.antigravity
    }
    pub fn with_antigravity(
        mut self,
        antigravity: crate::antigravity::AntigravityUpstream,
    ) -> Self {
        self.antigravity = antigravity.with_transport(self.transport.clone());
        self
    }
    pub fn api(&self) -> &crate::api::ApiUpstream {
        &self.api
    }
    pub fn with_api(mut self, api: crate::api::ApiUpstream) -> Self {
        self.api = api.with_transport(self.transport.clone());
        self
    }
    /// Composition-time injection for an alternate transport, including in-memory load fixtures.
    pub fn with_transport(mut self, transport: Arc<dyn crate::HttpTransport>) -> Self {
        self.grok = self.grok.with_transport(transport.clone());
        self.antigravity = self.antigravity.with_transport(transport.clone());
        self.api = self.api.with_transport(transport.clone());
        self.transport = transport;
        self.clients = Arc::default();
        self
    }
    pub async fn refresh_supplier_auth(
        &self,
        account: &codex2api_storage::SupplierAccount,
        rejected: &str,
    ) -> Result<()> {
        match account.provider_id.as_str() {
            codex2api_common::CHATGPT => {
                self.auth
                    .refresh_rejected_token(&account.id, rejected)
                    .await?;
            }
            codex2api_common::GROK => {
                self.grok
                    .auth()
                    .refresh_grok(&account.id, true, Some(rejected))
                    .await?;
            }
            codex2api_common::ANTIGRAVITY => {
                self.antigravity
                    .auth()
                    .refresh(&account.id, true, Some(rejected))
                    .await?;
            }
            // A rejected key has no refresh; an administrator replaces it.
            codex2api_common::API => return Err(crate::UpstreamError::Unauthorized),
            _ => {
                return Err(crate::UpstreamError::Internal(
                    "Unsupported provider".into(),
                ));
            }
        }
        Ok(())
    }

    /// Return the isolated client for `account_id`, creating it on first use.
    pub async fn get(&self, account_id: &str) -> Result<Arc<UpstreamClient>> {
        let clients = self.auth.account_http(account_id).await?;
        let current = clients.clone();
        self.clients
            .get_or_update(
                account_id,
                |old| Arc::ptr_eq(&old.account_http, &current),
                |_| async {
                    let credentials = self.auth.credentials(account_id).await?;
                    let tokens = credentials
                        .auth
                        .tokens
                        .as_ref()
                        .filter(|t| !t.access_token.is_empty())
                        .ok_or_else(|| {
                            crate::UpstreamError::MissingAccessToken(account_id.into())
                        })?;
                    let mut client = UpstreamClient::build_with_http(
                        clients.identity.clone(),
                        tokens.access_token.clone(),
                        tokens.account_id.clone(),
                        Some(self.auth.clone()),
                        clients,
                    );
                    client.apply_auth(credentials)?;
                    client.transport = self.transport.clone();
                    if let Some(timeout) = self.stream_idle_timeout {
                        client = client.with_stream_idle_timeout(timeout);
                    }
                    Ok(Arc::new(client))
                },
            )
            .await
    }

    pub async fn evict(&self, account_id: &str) {
        self.clients.remove(account_id);
        self.auth.evict_account_http(account_id).await;
        self.grok.auth().evict_account_http(account_id).await;
        self.antigravity.auth().evict_account_http(account_id).await;
        self.api.auth().evict_account_http(account_id).await;
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
