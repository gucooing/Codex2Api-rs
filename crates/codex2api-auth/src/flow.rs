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
    clients: Arc<tokio::sync::Mutex<HashMap<String, Arc<crate::transport::AccountHttpClients>>>>,
    pub(crate) drafts: Arc<tokio::sync::Mutex<HashMap<String, Arc<crate::draft::DraftLogin>>>>,
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
            clients: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
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
        let mut clients = self.clients.lock().await;
        let account = self.storage()?.require_account(account_id).await?;
        let identity = AccountIdentity::from_account(&account);
        let proxy = match account.proxy_id.as_deref() {
            Some(id) => Some(self.storage()?.require_outbound_proxy(id).await?),
            None => None,
        };
        let proxy_url = proxy.as_ref().map(|proxy| proxy.url.as_str());
        let client = if let Some(existing) = clients.get(account_id) {
            if existing.matches(&identity, proxy_url) {
                return Ok(existing.clone());
            }
            existing.reconfigure(&identity, proxy_url)?
        } else {
            crate::transport::AccountHttpClients::with_proxy(&identity, proxy_url)?
        };
        let client = Arc::new(client);
        clients.insert(account_id.to_string(), client.clone());
        Ok(client)
    }

    /// Reload the stored HTTP identity without discarding account cookies or refresh coordination.
    pub async fn reload_account_http(&self, account_id: &str) -> Result<()> {
        self.account_http(account_id).await?;
        Ok(())
    }

    pub async fn evict_account_http(&self, account_id: &str) {
        self.clients.lock().await.remove(account_id);
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

    pub async fn refresh(&self, account_id: &str, force: bool) -> Result<AuthDotJson> {
        let http = self.account_http(account_id).await?;
        let _guard = http.refresh_lock.lock().await;
        let account = self.storage()?.require_account(account_id).await?;
        refresh_account(
            &self.accounts,
            &http.authenticated,
            &self.cfg,
            &account,
            force,
        )
        .await
    }

    pub async fn refresh_rejected_token(
        &self,
        account_id: &str,
        rejected: &str,
    ) -> Result<AuthDotJson> {
        let http = self.account_http(account_id).await?;
        let _guard = http.refresh_lock.lock().await;
        let account = self.storage()?.require_account(account_id).await?;
        let current = load_auth(&self.accounts, &account)
            .await?
            .ok_or_else(|| AuthError::TokensNotFound(account_id.to_string()))?;
        if current
            .tokens
            .as_ref()
            .is_some_and(|tokens| tokens.access_token != rejected)
        {
            return Ok(current);
        }
        refresh_account(
            &self.accounts,
            &http.authenticated,
            &self.cfg,
            &account,
            true,
        )
        .await
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
