//! Shared administrator dependencies and the most recent authorization link.
use codex2api_accounts::AccountStore;
use codex2api_auth::AuthService;
use codex2api_storage::Storage;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub(crate) struct InflightOauth {
    pub account_id: String,
    pub state: String,
}

#[derive(Clone)]
pub struct AdminState {
    pub storage: Storage,
    pub accounts: AccountStore,
    pub auth: AuthService,
    pub upstream: codex2api_upstream::UpstreamPool,
    pub(crate) quota_cache: Arc<crate::quota::QuotaCache>,
    last_oauth: Arc<Mutex<Option<InflightOauth>>>,
}

impl AdminState {
    pub fn new(storage: Storage) -> anyhow::Result<Self> {
        let accounts = AccountStore::open(storage.clone());
        let auth = AuthService::new(accounts.clone())?;
        Ok(Self::from_parts(storage, accounts, auth))
    }

    pub fn from_parts(storage: Storage, accounts: AccountStore, auth: AuthService) -> Self {
        Self {
            upstream: codex2api_upstream::UpstreamPool::new(auth.clone()),
            quota_cache: Arc::new(crate::quota::QuotaCache::new(storage.clone())),
            storage,
            accounts,
            auth,
            last_oauth: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn last_oauth(&self) -> Option<InflightOauth> {
        self.last_oauth
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn with_upstream(mut self, upstream: codex2api_upstream::UpstreamPool) -> Self {
        self.upstream = upstream;
        self
    }

    pub(crate) fn store_last_oauth(&self, info: InflightOauth) {
        *self
            .last_oauth
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(info);
    }

    pub(crate) fn clear_oauth_if(&self, state: &str) {
        let mut slot = self
            .last_oauth
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if slot.as_ref().is_some_and(|info| info.state == state) {
            *slot = None;
        }
    }

    pub(crate) fn clear_account_oauth(&self, account_id: &str) {
        let mut slot = self
            .last_oauth
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if slot
            .as_ref()
            .is_some_and(|info| info.account_id == account_id)
        {
            *slot = None;
        }
    }
}
