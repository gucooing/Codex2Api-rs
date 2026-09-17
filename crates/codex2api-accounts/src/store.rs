use serde::{Deserialize, Serialize};
use uuid::Uuid;

use codex2api_storage::{Account, AccountStatus, AccountUpdate, NewAccount, Storage, StorageError};

use crate::auth_json::AuthDotJson;
use crate::error::{AccountError, Result};
use crate::identity::{AccountIdentity, HostRuntime, new_installation_id};

/// SQLite-backed store for isolated per-account Codex identity.
///
/// Account identity, installation_id, and tokens live on the account row /
/// `account_tokens` table. No per-account filesystem home is created.
#[derive(Debug, Clone)]
pub struct AccountStore {
    storage: Option<Storage>,
}

/// Pending authorization context created before OAuth identity is known.
#[derive(Debug, Clone)]
pub struct PendingAccount {
    pub account: Account,
    pub identity: AccountIdentity,
}

/// Result of binding OAuth identity onto a pending (or existing) account.
#[derive(Debug, Clone)]
pub struct BoundAccount {
    pub account: Account,
    pub identity: AccountIdentity,
    /// True when an existing row for the same `chatgpt_account_id` was reused.
    pub reused_existing: bool,
}

/// Upstream ChatGPT identity discovered after OAuth succeeds.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OauthIdentity {
    pub chatgpt_account_id: String,
    pub chatgpt_user_id: Option<String>,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub display_name: Option<String>,
}

/// Loaded runtime context for an account.
#[derive(Debug, Clone)]
pub struct AccountContext {
    pub account: Account,
    pub identity: AccountIdentity,
    pub auth: Option<AuthDotJson>,
}

impl AccountStore {
    pub fn new() -> Self {
        Self { storage: None }
    }

    pub fn open(storage: Storage) -> Self {
        Self {
            storage: Some(storage),
        }
    }

    pub fn with_storage(mut self, storage: Storage) -> Self {
        self.storage = Some(storage);
        self
    }

    pub fn storage(&self) -> Result<&Storage> {
        self.storage.as_ref().ok_or(AccountError::StorageRequired)
    }

    pub fn build_identity(&self, account_id: &str, installation_id: String) -> AccountIdentity {
        AccountIdentity::new(account_id, installation_id, HostRuntime::generate())
    }

    /// Create a pending account: UUID installation_id, official originator /
    /// UA version, and a random OS/terminal profile frozen on the SQLite row.
    pub async fn create_pending(&self) -> Result<PendingAccount> {
        let account_id = Uuid::new_v4().to_string();
        self.create_pending_with_id(&account_id).await
    }

    pub async fn create_pending_with_id(&self, account_id: &str) -> Result<PendingAccount> {
        validate_account_id(account_id)?;
        let installation_id = new_installation_id();
        let identity = self.build_identity(account_id, installation_id);
        self.create_pending_with_identity(identity).await
    }

    pub async fn create_pending_with_identity(
        &self,
        identity: AccountIdentity,
    ) -> Result<PendingAccount> {
        validate_account_id(&identity.account_id)?;
        let storage = self.storage()?;
        let mut new = NewAccount::pending_identity(
            identity.installation_id.clone(),
            identity.originator.clone(),
            identity.user_agent.clone(),
            identity.os_type.clone(),
            identity.os_version.clone(),
            identity.arch.clone(),
            String::new(),
            identity.fingerprint_json()?,
        );
        new.id = Some(identity.account_id.clone());
        let account = storage.create_account(new).await?;
        Ok(PendingAccount { account, identity })
    }

    /// Persist a draft only after authorization succeeds, together with its credentials.
    pub async fn save_authorized_identity(
        &self,
        identity: &AccountIdentity,
        oauth: OauthIdentity,
        auth: &AuthDotJson,
        proxy_id: Option<&str>,
    ) -> Result<BoundAccount> {
        validate_account_id(&identity.account_id)?;
        if oauth.chatgpt_account_id.trim().is_empty() {
            return Err(AccountError::MissingChatgptAccountId);
        }
        let mut new = NewAccount::pending_identity(
            &identity.installation_id,
            &identity.originator,
            &identity.user_agent,
            &identity.os_type,
            &identity.os_version,
            &identity.arch,
            "",
            identity.fingerprint_json()?,
        );
        new.id = Some(identity.account_id.clone());
        new.chatgpt_account_id = Some(oauth.chatgpt_account_id);
        new.chatgpt_user_id = oauth.chatgpt_user_id;
        new.display_name = oauth.display_name.or(oauth.email.clone());
        new.email = oauth.email;
        new.plan_type = oauth.plan_type;
        let account = self
            .storage()?
            .save_authorized_account(new, auth.to_account_tokens(&identity.account_id), proxy_id)
            .await?;
        Ok(BoundAccount {
            reused_existing: account.id != identity.account_id,
            identity: AccountIdentity::from_account(&account),
            account,
        })
    }

    /// Bind OAuth identity onto a pending context.
    ///
    /// If `chatgpt_account_id` already has an account, that account is reused and
    /// its `installation_id` is never rotated. The pending row is deleted.
    pub async fn bind(
        &self,
        pending: &PendingAccount,
        mut oauth: OauthIdentity,
        auth: Option<&AuthDotJson>,
    ) -> Result<BoundAccount> {
        if oauth.chatgpt_account_id.trim().is_empty() {
            if let Some(from_auth) = auth.and_then(AuthDotJson::chatgpt_account_id) {
                oauth.chatgpt_account_id = from_auth.to_string();
            }
        }
        let chatgpt_account_id = oauth.chatgpt_account_id.trim().to_string();
        if chatgpt_account_id.is_empty() {
            return Err(AccountError::MissingChatgptAccountId);
        }
        oauth.chatgpt_account_id = chatgpt_account_id.clone();

        let storage = self.storage()?;
        match storage
            .get_account_by_chatgpt_account_id(&chatgpt_account_id)
            .await?
        {
            Some(existing) => self.reuse_existing(pending, existing, oauth, auth).await,
            None => match self.finalize_pending(pending, oauth.clone(), auth).await {
                Ok(bound) => Ok(bound),
                Err(AccountError::Storage(StorageError::DuplicateChatgptAccountId)) => {
                    let existing = storage
                        .get_account_by_chatgpt_account_id(&chatgpt_account_id)
                        .await?
                        .ok_or(AccountError::Storage(
                            StorageError::DuplicateChatgptAccountId,
                        ))?;
                    self.reuse_existing(pending, existing, oauth, auth).await
                }
                Err(err) => Err(err),
            },
        }
    }

    pub fn refuse_installation_id_rotation(
        &self,
        account: &Account,
        requested: &str,
    ) -> Result<()> {
        if account.installation_id == requested {
            return Ok(());
        }
        let chatgpt_account_id = account.chatgpt_account_id.clone().unwrap_or_default();
        Err(AccountError::InstallationIdRotationRefused {
            chatgpt_account_id,
            account_id: account.id.clone(),
            installation_id: account.installation_id.clone(),
        })
    }

    pub async fn abandon_pending(&self, pending: &PendingAccount) -> Result<()> {
        let storage = self.storage()?;
        let account = storage.get_account(&pending.account.id).await?;
        if let Some(account) = account {
            if account.status != AccountStatus::Pending {
                return Err(AccountError::NotPending(account.id));
            }
            if account.chatgpt_account_id.is_some() {
                return Err(AccountError::NotPending(account.id));
            }
            storage.delete_account(&account.id).await?;
        }
        Ok(())
    }

    pub async fn load_context(&self, account_id: &str) -> Result<AccountContext> {
        validate_account_id(account_id)?;
        let storage = self.storage()?;
        let account = storage
            .get_account(account_id)
            .await?
            .ok_or_else(|| AccountError::NotFound(account_id.to_string()))?;
        let identity = AccountIdentity::from_account(&account);
        let auth = self.load_auth_for_account(&account).await?;
        Ok(AccountContext {
            account,
            identity,
            auth,
        })
    }

    pub async fn load_identity(&self, account_id: &str) -> Result<AccountIdentity> {
        validate_account_id(account_id)?;
        let storage = self.storage()?;
        let account = storage
            .get_account(account_id)
            .await?
            .ok_or_else(|| AccountError::NotFound(account_id.to_string()))?;
        Ok(AccountIdentity::from_account(&account))
    }

    pub async fn load_auth_for_account(&self, account: &Account) -> Result<Option<AuthDotJson>> {
        let storage = self.storage()?;
        match storage.load_account_tokens(&account.id).await? {
            Some(tokens) => {
                let mut auth = AuthDotJson::from_account_tokens(&tokens)?;
                if auth.chatgpt_account_id().is_none() {
                    if let Some(tokens_inner) = auth.tokens.as_mut() {
                        tokens_inner.account_id = account.chatgpt_account_id.clone();
                    }
                }
                if auth.tokens.is_some() || auth.auth_mode.is_some() {
                    Ok(Some(auth))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    pub async fn save_auth_for_account(&self, account_id: &str, auth: &AuthDotJson) -> Result<()> {
        validate_account_id(account_id)?;
        let storage = self.storage()?;
        storage
            .upsert_account_tokens(auth.to_account_tokens(account_id))
            .await?;
        Ok(())
    }

    async fn finalize_pending(
        &self,
        pending: &PendingAccount,
        oauth: OauthIdentity,
        auth: Option<&AuthDotJson>,
    ) -> Result<BoundAccount> {
        let storage = self.storage()?;
        let current = storage
            .get_account(&pending.account.id)
            .await?
            .ok_or_else(|| AccountError::NotFound(pending.account.id.clone()))?;
        if current.status != AccountStatus::Pending {
            return Err(AccountError::NotPending(current.id));
        }

        if let Some(auth) = auth {
            storage
                .upsert_account_tokens(auth.to_account_tokens(&current.id))
                .await?;
        }

        let account = storage
            .update_account(
                &current.id,
                AccountUpdate {
                    status: Some(AccountStatus::Active),
                    display_name: oauth.display_name.or(oauth.email.clone()),
                    chatgpt_account_id: Some(oauth.chatgpt_account_id),
                    chatgpt_user_id: oauth.chatgpt_user_id,
                    email: oauth.email,
                    plan_type: oauth.plan_type,
                    ..AccountUpdate::default()
                },
            )
            .await?;

        Ok(BoundAccount {
            identity: AccountIdentity::from_account(&account),
            account,
            reused_existing: false,
        })
    }

    async fn reuse_existing(
        &self,
        pending: &PendingAccount,
        existing: Account,
        oauth: OauthIdentity,
        auth: Option<&AuthDotJson>,
    ) -> Result<BoundAccount> {
        let storage = self.storage()?;

        if pending.identity.installation_id != existing.installation_id {
            tracing::info!(
                chatgpt_account_id = %oauth.chatgpt_account_id,
                existing_account = %existing.id,
                existing_installation_id = %existing.installation_id,
                pending_installation_id = %pending.identity.installation_id,
                "re-login of existing ChatGPT account; keeping persisted installation_id"
            );
        }

        if let Some(auth) = auth {
            storage
                .upsert_account_tokens(auth.to_account_tokens(&existing.id))
                .await?;
        }

        let status = match existing.status {
            AccountStatus::Disabled | AccountStatus::Pending => Some(AccountStatus::Active),
            AccountStatus::Active => None,
        };

        let account = storage
            .update_account(
                &existing.id,
                AccountUpdate {
                    status,
                    display_name: oauth
                        .display_name
                        .or(oauth.email.clone())
                        .or(existing.display_name.clone()),
                    chatgpt_account_id: Some(oauth.chatgpt_account_id),
                    chatgpt_user_id: oauth.chatgpt_user_id.or(existing.chatgpt_user_id.clone()),
                    email: oauth.email.or(existing.email.clone()),
                    plan_type: oauth.plan_type.or(existing.plan_type.clone()),
                    ..AccountUpdate::default()
                },
            )
            .await?;

        if pending.account.id != existing.id {
            // A newly added login may resolve to an existing ChatGPT account. Keep
            // its identity, but carry the login's selected network route forward.
            let selected = storage.require_account(&pending.account.id).await?.proxy_id;
            let account = storage
                .set_account_proxy(&existing.id, selected.as_deref())
                .await?;
            let _ = storage.delete_account(&pending.account.id).await;
            return Ok(BoundAccount {
                identity: AccountIdentity::from_account(&account),
                account,
                reused_existing: true,
            });
        }

        Ok(BoundAccount {
            identity: AccountIdentity::from_account(&account),
            account,
            reused_existing: true,
        })
    }
}

impl Default for AccountStore {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_account_id(account_id: &str) -> Result<()> {
    if account_id.is_empty()
        || account_id.contains('/')
        || account_id.contains('\\')
        || account_id.contains("..")
        || account_id.contains('\0')
    {
        return Err(AccountError::InvalidAccountId(account_id.to_string()));
    }
    Ok(())
}

impl BoundAccount {
    pub fn installation_id(&self) -> &str {
        &self.account.installation_id
    }

    pub fn chatgpt_account_id(&self) -> Option<&str> {
        self.account.chatgpt_account_id.as_deref()
    }
}

impl PendingAccount {
    pub fn account_id(&self) -> &str {
        &self.account.id
    }

    pub fn installation_id(&self) -> &str {
        &self.identity.installation_id
    }
}
