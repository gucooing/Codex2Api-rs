use axum::extract::FromRef;
use codex2api_storage::Storage;
use codex2api_suppliers::SupplierAccountStore;
use codex2api_upstream::UpstreamPool;

/// Shared state for the configured-client OAuth listener.
///
/// Callers (the process binary) construct this once and pass it to [`crate::router`].
#[derive(Clone)]
pub struct OAuthState {
    pub storage: Storage,
    pub accounts: SupplierAccountStore,
    pub upstream: UpstreamPool,
    pub oauth_base_url: Option<String>,
    pub user_base_url: String,
    #[cfg(test)]
    pub(crate) websocket_test_targets: std::collections::HashMap<String, String>,
}

impl OAuthState {
    pub fn new(storage: Storage, accounts: SupplierAccountStore, upstream: UpstreamPool) -> Self {
        Self {
            storage,
            accounts,
            upstream,
            oauth_base_url: None,
            user_base_url: "http://127.0.0.1:8082".into(),
            #[cfg(test)]
            websocket_test_targets: Default::default(),
        }
    }

    pub fn with_oauth_base_url(mut self, value: &str) -> anyhow::Result<Self> {
        let url = url::Url::parse(value)?;
        anyhow::ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none(),
            "CODEX2API_PUBLIC_OAUTH_URL must be an HTTP(S) origin without a path, credentials, query or fragment"
        );
        self.oauth_base_url = Some(url.origin().ascii_serialization());
        Ok(self)
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub async fn public_oauth_url(&self) -> codex2api_storage::Result<Option<String>> {
        Ok(self
            .storage
            .public_url_settings()
            .await?
            .map(|settings| settings.oauth_url)
            .or_else(|| self.oauth_base_url.clone()))
    }

    pub async fn public_user_url(&self) -> codex2api_storage::Result<String> {
        Ok(self
            .storage
            .public_url_settings()
            .await?
            .map(|settings| settings.user_url)
            .unwrap_or_else(|| self.user_base_url.clone()))
    }

    pub fn accounts(&self) -> &SupplierAccountStore {
        &self.accounts
    }

    pub fn upstream(&self) -> &UpstreamPool {
        &self.upstream
    }
}

impl FromRef<OAuthState> for Storage {
    fn from_ref(state: &OAuthState) -> Self {
        state.storage.clone()
    }
}

impl FromRef<OAuthState> for SupplierAccountStore {
    fn from_ref(state: &OAuthState) -> Self {
        state.accounts.clone()
    }
}

impl FromRef<OAuthState> for UpstreamPool {
    fn from_ref(state: &OAuthState) -> Self {
        state.upstream.clone()
    }
}
