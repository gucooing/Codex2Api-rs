//! Dependencies for the cookie-authenticated administrator API.
use codex2api_accounts::SupplierAccountStore;
use codex2api_auth::AuthService;
use codex2api_storage::Storage;
use std::sync::Arc;
#[derive(Clone)]
pub struct AdminState {
    pub storage: Storage,
    pub accounts: SupplierAccountStore,
    pub auth: AuthService,
    pub upstream: codex2api_upstream::UpstreamPool,
    pub(crate) supplier_cache: Arc<crate::quota::SupplierCache>,
}
impl AdminState {
    pub fn new(storage: Storage) -> anyhow::Result<Self> {
        let accounts = SupplierAccountStore::open(storage.clone());
        let auth = AuthService::new(accounts.clone())?;
        Ok(Self::from_parts(storage, accounts, auth))
    }
    pub fn from_parts(storage: Storage, accounts: SupplierAccountStore, auth: AuthService) -> Self {
        Self {
            upstream: codex2api_upstream::UpstreamPool::new(auth.clone()),
            supplier_cache: Arc::new(crate::quota::SupplierCache::new(storage.clone())),
            storage,
            accounts,
            auth,
        }
    }
    pub fn with_upstream(mut self, upstream: codex2api_upstream::UpstreamPool) -> Self {
        self.upstream = upstream;
        self
    }
}
