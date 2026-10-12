use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{AuthError, Result};
use crate::oauth::{CallbackQuery, OAuthConfig};
use crate::persist::{
    CompletedLogin, complete_login, load_auth, pending_from_account, persist_auth, refresh_account,
    revoke_account,
};
use crate::tokens::{TokenSet, token_set_from_auth};
use codex2api_accounts::AuthDotJson;
use codex2api_accounts::{AccountIdentity, SupplierAccountStore};
use codex2api_storage::Storage;

/// High-level ChatGPT OAuth helper used by the admin UI.
///
/// The library never opens a system browser. Callers receive `authorize_url`
/// and present it in the admin page. Callback URLs are submitted manually;
/// this service never binds a local callback listener.
#[derive(Clone)]
pub struct AuthService {
    accounts: SupplierAccountStore,
    cfg: OAuthConfig,
    clients: Arc<crate::account_cache::AccountCache<CachedAccountHttp>>,
    pub(crate) drafts: Arc<tokio::sync::Mutex<HashMap<String, Arc<crate::draft::DraftLogin>>>>,
}

#[derive(Clone)]
struct CachedAccountHttp {
    snapshot: Arc<codex2api_storage::SupplierTransportSnapshot>,
    clients: Arc<crate::transport::AccountHttpClients>,
}

impl AuthService {
    pub fn new(accounts: SupplierAccountStore) -> Result<Self> {
        Self::with_config(accounts, OAuthConfig::default())
    }

    pub fn with_storage(storage: Storage, accounts: SupplierAccountStore) -> Result<Self> {
        let _ = storage;
        Self::new(accounts)
    }

    pub fn with_config(accounts: SupplierAccountStore, cfg: OAuthConfig) -> Result<Self> {
        Ok(Self {
            accounts,
            cfg,
            clients: Arc::default(),
            drafts: Arc::default(),
        })
    }

    pub fn config(&self) -> &OAuthConfig {
        &self.cfg
    }

    pub fn storage(&self) -> Result<&Storage> {
        Ok(self.accounts.storage()?)
    }

    pub fn accounts(&self) -> &SupplierAccountStore {
        &self.accounts
    }

    pub async fn account_http(
        &self,
        account_id: &str,
    ) -> Result<Arc<crate::transport::AccountHttpClients>> {
        let snapshot = self
            .storage()?
            .supplier_transport_snapshot(account_id)
            .await?;
        if snapshot.provider_id != codex2api_core::CHATGPT {
            return Err(AuthError::AccountNotFound(account_id.into()));
        }
        let current = snapshot.clone();
        Ok(self
            .clients
            .get_or_update(
                account_id,
                |old| {
                    Arc::ptr_eq(&old.snapshot, &current)
                        || old.snapshot.cache_generation > current.cache_generation
                },
                |old| async move {
                    let identity = AccountIdentity::from_transport(&snapshot);
                    let proxy = snapshot.proxy_url.as_deref();
                    let clients = match old {
                        Some(old) if old.clients.matches(&identity, proxy) => old.clients,
                        Some(old) => Arc::new(old.clients.reconfigure(&identity, proxy)?),
                        None => Arc::new(crate::transport::AccountHttpClients::with_proxy(
                            &identity, proxy,
                        )?),
                    };
                    Ok::<_, AuthError>(CachedAccountHttp { snapshot, clients })
                },
            )
            .await?
            .clients)
    }

    /// Reload the stored HTTP identity without discarding account cookies or refresh coordination.
    pub async fn reload_account_http(&self, account_id: &str) -> Result<()> {
        self.account_http(account_id).await?;
        Ok(())
    }

    pub async fn evict_account_http(&self, account_id: &str) {
        self.clients.remove(account_id);
    }

    /// Exchange an already-validated callback submitted through the administrator UI.
    pub async fn complete_from_callback(&self, query: &CallbackQuery) -> Result<CompletedLogin> {
        if query.is_cancel() {
            return Err(AuthError::Cancelled);
        }
        if let Some(error) = query.error.as_deref() {
            return Err(AuthError::Callback {
                code: error.to_string(),
                description: query.error_description.clone(),
            });
        }
        let state = query.state.as_deref().ok_or(AuthError::StateMismatch)?;
        let pending_row = self
            .storage()?
            .get_oauth_pending(state)
            .await?
            .ok_or(AuthError::PendingNotFound)?;
        let account_id = pending_row
            .account_id
            .as_deref()
            .ok_or_else(|| AuthError::AccountNotFound("pending oauth".into()))?;
        let pending_account = pending_from_account(&self.accounts, account_id).await?;
        let code = query.code.as_deref().ok_or(AuthError::MissingCode)?;
        let http = self.account_http(account_id).await?;
        let completed = complete_login(
            &self.accounts,
            &http.raw,
            &self.cfg,
            &pending_account,
            &pending_row.redirect_uri,
            &pending_row.code_verifier,
            code,
        )
        .await?;
        let _ = self.storage()?.delete_oauth_pending(state).await;
        Ok(completed)
    }

    pub async fn credentials(
        &self,
        account_id: &str,
    ) -> Result<Arc<crate::tokens::ChatgptCredentials>> {
        let snapshot = self
            .storage()?
            .supplier_auth_snapshot(account_id)
            .await?
            .ok_or_else(|| AuthError::TokensNotFound(account_id.into()))?;
        if snapshot.provider_id != codex2api_core::CHATGPT {
            return Err(AuthError::AccountMismatch);
        }
        snapshot.decode(|| crate::tokens::ChatgptCredentials::from_snapshot(&snapshot))
    }

    pub async fn refresh(
        &self,
        account_id: &str,
        force: bool,
    ) -> Result<Arc<crate::tokens::ChatgptCredentials>> {
        let current = self.credentials(account_id).await?;
        if !force && !current.needs_refresh() {
            return Ok(current);
        }
        let http = self.account_http(account_id).await?;
        let _guard = http.refresh_lock.lock().await;
        let current = self.credentials(account_id).await?;
        if !force && !current.needs_refresh() {
            return Ok(current);
        }
        let account = self
            .storage()?
            .supplier_account_snapshot(account_id)
            .await?;
        let result = refresh_account(
            &self.accounts,
            &http.authenticated,
            &self.cfg,
            &account,
            force,
        )
        .await;
        if let Err(error) = &result
            && error.permanent_refresh_failure()
            && (force || !matches!(error, AuthError::MissingRefreshToken))
        {
            self.storage()?
                .reject_supplier_auth(account_id, current.revision)
                .await?;
        }
        result?;
        self.credentials(account_id).await
    }

    pub async fn refresh_rejected_token(
        &self,
        account_id: &str,
        rejected: &str,
    ) -> Result<Arc<crate::tokens::ChatgptCredentials>> {
        let http = self.account_http(account_id).await?;
        let _guard = http.refresh_lock.lock().await;
        let current = self.credentials(account_id).await?;
        if current
            .auth
            .tokens
            .as_ref()
            .is_some_and(|tokens| tokens.access_token != rejected)
        {
            return Ok(current);
        }
        let account = self
            .storage()?
            .supplier_account_snapshot(account_id)
            .await?;
        let result = refresh_account(
            &self.accounts,
            &http.authenticated,
            &self.cfg,
            &account,
            true,
        )
        .await;
        if let Err(error) = &result
            && error.permanent_refresh_failure()
        {
            self.storage()?
                .reject_supplier_auth(account_id, current.revision)
                .await?;
        }
        result?;
        self.credentials(account_id).await
    }

    pub async fn revoke(&self, account_id: &str) -> Result<()> {
        let http = self.account_http(account_id).await?;
        let _guard = http.refresh_lock.lock().await;
        let account = self.storage()?.require_account(account_id).await?;
        revoke_account(&self.accounts, &http.authenticated, &self.cfg, &account).await
    }

    pub async fn token_set(&self, account_id: &str) -> Result<Option<TokenSet>> {
        let account = self.storage()?.require_account(account_id).await?;
        match load_auth(&self.accounts, &account).await? {
            Some(auth) => Ok(Some(token_set_from_auth(&auth))),
            None => Ok(None),
        }
    }

    pub async fn persist_account_auth(&self, account_id: &str, auth: &AuthDotJson) -> Result<()> {
        persist_auth(&self.accounts, account_id, auth).await
    }
}
