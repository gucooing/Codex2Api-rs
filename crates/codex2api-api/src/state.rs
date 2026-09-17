use axum::extract::FromRef;
use codex2api_accounts::AccountStore;
use codex2api_storage::Storage;
use codex2api_upstream::UpstreamPool;

/// Shared state for the public Codex-compatible API.
///
/// Callers (the process binary) construct this once and pass it to [`crate::router`].
#[derive(Clone)]
pub struct ApiState {
    pub storage: Storage,
    pub accounts: AccountStore,
    pub upstream: UpstreamPool,
}

impl ApiState {
    pub fn new(storage: Storage, accounts: AccountStore, upstream: UpstreamPool) -> Self {
        Self {
            storage,
            accounts,
            upstream,
        }
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub fn accounts(&self) -> &AccountStore {
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

impl FromRef<ApiState> for AccountStore {
    fn from_ref(state: &ApiState) -> Self {
        state.accounts.clone()
    }
}

impl FromRef<ApiState> for UpstreamPool {
    fn from_ref(state: &ApiState) -> Self {
        state.upstream.clone()
    }
}
