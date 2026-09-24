use axum::extract::FromRef;
use codex2api_accounts::SupplierAccountStore;
use codex2api_storage::Storage;
use codex2api_upstream::UpstreamPool;

/// Shared state for the public Codex-compatible API.
///
/// Callers (the process binary) construct this once and pass it to [`crate::router`].
#[derive(Clone)]
pub struct ApiState {
    pub storage: Storage,
    pub accounts: SupplierAccountStore,
    pub upstream: UpstreamPool,
    pub public_base_url: Option<String>,
}

impl ApiState {
    pub fn new(storage: Storage, accounts: SupplierAccountStore, upstream: UpstreamPool) -> Self {
        Self {
            storage,
            accounts,
            upstream,
            public_base_url: None,
        }
    }

    pub fn with_public_base_url(mut self, value: &str) -> anyhow::Result<Self> {
        let url = url::Url::parse(value)?;
        anyhow::ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none(),
            "CODEX2API_PUBLIC_BASE_URL must be an HTTP(S) origin without a path, credentials, query or fragment"
        );
        self.public_base_url = Some(url.origin().ascii_serialization());
        Ok(self)
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub fn accounts(&self) -> &SupplierAccountStore {
        &self.accounts
    }

    pub fn upstream(&self) -> &UpstreamPool {
        &self.upstream
    }
}

impl FromRef<ApiState> for Storage {
    fn from_ref(state: &ApiState) -> Self {
        state.storage.clone()
    }
}

impl FromRef<ApiState> for SupplierAccountStore {
    fn from_ref(state: &ApiState) -> Self {
        state.accounts.clone()
    }
}

impl FromRef<ApiState> for UpstreamPool {
    fn from_ref(state: &ApiState) -> Self {
        state.upstream.clone()
    }
}
