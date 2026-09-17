use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, SecondsFormat, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::api_key::{generate_proxy_api_key, hash_api_key, proxy_api_key_prefix};
use crate::error::{Result, StorageError};
use crate::password::{hash_password, verify_password};
use crate::types::{
    Account, AccountRuntime, AccountStatus, AccountTokens, AccountUpdate, AdminSession, AdminUser,
    IssuedProxyApiKey, NewAccount, OAuthPending, ProxyApiKey,
};
use crate::{DEFAULT_ADMIN_PASSWORD, DEFAULT_ADMIN_USERNAME};

pub const DEFAULT_ADMIN_SESSION_TTL: Duration = Duration::from_secs(60 * 60 * 24);
pub const DEFAULT_OAUTH_PENDING_TTL: Duration = Duration::from_secs(10 * 60);

const ACCOUNT_COLUMNS: &str = "id, status, display_name, chatgpt_account_id, chatgpt_user_id, email, \
     plan_type, installation_id, originator, user_agent, os_type, os_version, arch, home_dir, \
     http_fingerprint_json, proxy_id, created_at, updated_at, last_used_at";

#[derive(Clone, Debug)]
pub struct Storage {
    pool: SqlitePool,
    db_path: PathBuf,
}

#[derive(Debug, FromRow)]
struct AccountRow {
    id: String,
    proxy_id: Option<String>,
    status: String,
    display_name: Option<String>,
    chatgpt_account_id: Option<String>,
    chatgpt_user_id: Option<String>,
    email: Option<String>,
    plan_type: Option<String>,
    installation_id: String,
    originator: String,
    user_agent: String,
    os_type: String,
    os_version: String,
    arch: String,
    home_dir: String,
    http_fingerprint_json: String,
    created_at: String,
    updated_at: String,
    last_used_at: Option<String>,
}

impl TryFrom<AccountRow> for Account {
    type Error = StorageError;

    fn try_from(row: AccountRow) -> Result<Self> {
        Ok(Self {
            id: row.id,
            proxy_id: row.proxy_id,
            status: row.status.parse()?,
            display_name: row.display_name,
            chatgpt_account_id: row.chatgpt_account_id,
            chatgpt_user_id: row.chatgpt_user_id,
            email: row.email,
            plan_type: row.plan_type,
            installation_id: row.installation_id,
            originator: row.originator,
            user_agent: row.user_agent,
            os_type: row.os_type,
            os_version: row.os_version,
            arch: row.arch,
            home_dir: row.home_dir,
            http_fingerprint_json: row.http_fingerprint_json,
            created_at: row.created_at,
            updated_at: row.updated_at,
            last_used_at: row.last_used_at,
        })
    }
}

impl Storage {
    pub async fn open(db_path: impl AsRef<Path>) -> Result<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }

        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await?;

        let storage = Self { pool, db_path };
        storage.migrate().await?;
        storage.stamp_codex_ref().await?;
        storage.ensure_default_admin().await?;
        Ok(storage)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    async fn stamp_codex_ref(&self) -> Result<()> {
        sqlx::query(
            "INSERT INTO meta (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind("codex_ref_commit")
        .bind(codex2api_version::CODEX_REF_COMMIT)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "INSERT INTO meta (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind("codex_ref_repo")
        .bind(codex2api_version::CODEX_REPO)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn ensure_default_admin(&self) -> Result<()> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admin_users")
            .fetch_one(&self.pool)
            .await?;
        if count == 0 {
            let hash = hash_password(DEFAULT_ADMIN_PASSWORD)?;
            let now = now_rfc3339();
            sqlx::query(
                "INSERT INTO admin_users (id, username, password_hash, created_at, updated_at)
                 VALUES (1, ?, ?, ?, ?)",
            )
            .bind(DEFAULT_ADMIN_USERNAME)
            .bind(hash)
            .bind(&now)
            .bind(&now)
            .execute(&self.pool)
            .await?;
            tracing::info!(
                username = DEFAULT_ADMIN_USERNAME,
                "created default admin user"
            );
        }
        Ok(())
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }

    // --- admin user ---

    pub async fn get_admin_user(&self) -> Result<Option<AdminUser>> {
        let user = sqlx::query_as::<_, AdminUser>(
            "SELECT id, username, password_hash, created_at, updated_at
             FROM admin_users WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(user)
    }

    pub async fn require_admin_user(&self) -> Result<AdminUser> {
        self.get_admin_user()
            .await?
            .ok_or(StorageError::AdminNotFound)
    }

    pub async fn get_admin_by_username(&self, username: &str) -> Result<Option<AdminUser>> {
        let user = sqlx::query_as::<_, AdminUser>(
            "SELECT id, username, password_hash, created_at, updated_at
             FROM admin_users WHERE username = ?",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;
        Ok(user)
    }

    /// Returns the admin user when username and password match.
    pub async fn verify_admin(&self, username: &str, password: &str) -> Result<Option<AdminUser>> {
        let Some(user) = self.get_admin_by_username(username).await? else {
            return Ok(None);
        };
        if !verify_password(password, &user.password_hash)? {
            return Ok(None);
        }
        Ok(Some(user))
    }

    /// Verify credentials and create a cookie session.
    pub async fn login_admin(
        &self,
        username: &str,
        password: &str,
        ttl: Duration,
    ) -> Result<Option<AdminSession>> {
        let Some(user) = self.verify_admin(username, password).await? else {
            return Ok(None);
        };
        let now = now_rfc3339();
        let expires_at =
            (Utc::now() + duration_to_chrono(ttl)).to_rfc3339_opts(SecondsFormat::Millis, true);
        // Do not issue a session if credentials changed after password verification.
        Ok(sqlx::query_as::<_, AdminSession>(
            "INSERT INTO admin_sessions (id, admin_user_id, created_at, expires_at)
             SELECT ?, id, ?, ? FROM admin_users
             WHERE id = ? AND username = ? AND password_hash = ?
             RETURNING id, admin_user_id, created_at, expires_at",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(now)
        .bind(expires_at)
        .bind(user.id)
        .bind(user.username)
        .bind(user.password_hash)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn require_admin(&self, username: &str, password: &str) -> Result<AdminUser> {
        self.verify_admin(username, password)
            .await?
            .ok_or(StorageError::InvalidCredentials)
    }

    /// Verify both current credentials before changing them and revoking sessions atomically.
    /// An empty new password keeps the existing password.
    pub async fn change_admin_credentials(
        &self,
        old_username: &str,
        old_password: &str,
        new_username: &str,
        new_password: &str,
    ) -> Result<()> {
        let user = self
            .require_admin(old_username.trim(), old_password)
            .await?;
        let new_username = new_username.trim();
        if new_username.is_empty() {
            return Err(StorageError::InvalidAdminUpdate("新用户名不能为空"));
        }
        if new_username == user.username && new_password.is_empty() {
            return Err(StorageError::InvalidAdminUpdate("请输入新的用户名或密码"));
        }
        let password_hash = if new_password.is_empty() {
            user.password_hash.clone()
        } else {
            hash_password(new_password)?
        };
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query(
            "UPDATE admin_users SET username = ?, password_hash = ?, updated_at = ?
             WHERE id = ? AND username = ? AND password_hash = ?",
        )
        .bind(new_username)
        .bind(password_hash)
        .bind(now_rfc3339())
        .bind(user.id)
        .bind(user.username)
        .bind(user.password_hash)
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(StorageError::InvalidCredentials);
        }
        sqlx::query("DELETE FROM admin_sessions WHERE admin_user_id = ?")
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    // --- admin sessions ---

    pub async fn create_admin_session(&self, ttl: Duration) -> Result<AdminSession> {
        self.create_admin_session_for(1, ttl).await
    }

    pub async fn create_admin_session_for(
        &self,
        admin_user_id: i64,
        ttl: Duration,
    ) -> Result<AdminSession> {
        let id = Uuid::new_v4().to_string();
        let created_at = now_rfc3339();
        let expires_at =
            (Utc::now() + duration_to_chrono(ttl)).to_rfc3339_opts(SecondsFormat::Millis, true);
        let session = sqlx::query_as::<_, AdminSession>(
            "INSERT INTO admin_sessions (id, admin_user_id, created_at, expires_at)
             VALUES (?, ?, ?, ?)
             RETURNING id, admin_user_id, created_at, expires_at",
        )
        .bind(&id)
        .bind(admin_user_id)
        .bind(&created_at)
        .bind(&expires_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(session)
    }

    pub async fn get_admin_session(&self, id: &str) -> Result<Option<AdminSession>> {
        let session = sqlx::query_as::<_, AdminSession>(
            "SELECT id, admin_user_id, created_at, expires_at FROM admin_sessions WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(session) = session else {
            return Ok(None);
        };
        if is_expired(&session.expires_at) {
            self.delete_admin_session(&session.id).await?;
            return Ok(None);
        }
        Ok(Some(session))
    }

    pub async fn require_admin_session(&self, id: &str) -> Result<AdminSession> {
        self.get_admin_session(id)
            .await?
            .ok_or(StorageError::SessionNotFound)
    }

    pub async fn delete_admin_session(&self, id: &str) -> Result<bool> {
        let res = sqlx::query("DELETE FROM admin_sessions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn expire_admin_sessions(&self) -> Result<u64> {
        let now = now_rfc3339();
        let res = sqlx::query("DELETE FROM admin_sessions WHERE expires_at <= ?")
            .bind(&now)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    // --- accounts ---

    pub async fn create_account(&self, new: NewAccount) -> Result<Account> {
        let id = new
            .id
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let now = now_rfc3339();
        let chatgpt_account_id = empty_to_none(new.chatgpt_account_id);
        sqlx::query(
            "INSERT INTO accounts (
                id, status, display_name, chatgpt_account_id, chatgpt_user_id, email, plan_type,
                installation_id, originator, user_agent, os_type, os_version, arch, home_dir,
                http_fingerprint_json, created_at, updated_at, last_used_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)",
        )
        .bind(&id)
        .bind(new.status.as_str())
        .bind(&new.display_name)
        .bind(&chatgpt_account_id)
        .bind(&new.chatgpt_user_id)
        .bind(&new.email)
        .bind(&new.plan_type)
        .bind(&new.installation_id)
        .bind(&new.originator)
        .bind(&new.user_agent)
        .bind(&new.os_type)
        .bind(&new.os_version)
        .bind(&new.arch)
        .bind(&new.home_dir)
        .bind(&new.http_fingerprint_json)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        self.get_account(&id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(id))
    }

    /// Save a successful login atomically, retaining an existing account's frozen identity.
    pub async fn save_authorized_account(
        &self,
        new: NewAccount,
        tokens: AccountTokens,
        proxy_id: Option<&str>,
    ) -> Result<Account> {
        let mut tx = self.pool.begin().await?;
        let now = now_rfc3339();
        let sql = format!(
            "INSERT INTO accounts (
                id, status, display_name, chatgpt_account_id, chatgpt_user_id, email, plan_type,
                installation_id, originator, user_agent, os_type, os_version, arch, home_dir,
                http_fingerprint_json, proxy_id, created_at, updated_at
             ) VALUES (?, 'active', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(chatgpt_account_id) DO UPDATE SET
                status = 'active',
                display_name = COALESCE(excluded.display_name, accounts.display_name),
                chatgpt_user_id = COALESCE(excluded.chatgpt_user_id, accounts.chatgpt_user_id),
                email = COALESCE(excluded.email, accounts.email),
                plan_type = COALESCE(excluded.plan_type, accounts.plan_type),
                proxy_id = excluded.proxy_id,
                updated_at = excluded.updated_at
             RETURNING {ACCOUNT_COLUMNS}"
        );
        let row = sqlx::query_as::<_, AccountRow>(&sql)
            .bind(new.id.unwrap_or_else(|| Uuid::new_v4().to_string()))
            .bind(new.display_name)
            .bind(new.chatgpt_account_id)
            .bind(new.chatgpt_user_id)
            .bind(new.email)
            .bind(new.plan_type)
            .bind(new.installation_id)
            .bind(new.originator)
            .bind(new.user_agent)
            .bind(new.os_type)
            .bind(new.os_version)
            .bind(new.arch)
            .bind(new.home_dir)
            .bind(new.http_fingerprint_json)
            .bind(proxy_id)
            .bind(&now)
            .bind(&now)
            .fetch_one(&mut *tx)
            .await?;
        let account = Account::try_from(row)?;
        sqlx::query(
            "INSERT INTO account_tokens (account_id, auth_mode, id_token, access_token,
                refresh_token, last_refresh, raw_auth_json, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(account_id) DO UPDATE SET
                auth_mode = excluded.auth_mode, id_token = excluded.id_token,
                access_token = excluded.access_token, refresh_token = excluded.refresh_token,
                last_refresh = excluded.last_refresh, raw_auth_json = excluded.raw_auth_json,
                updated_at = excluded.updated_at",
        )
        .bind(&account.id)
        .bind(tokens.auth_mode)
        .bind(tokens.id_token)
        .bind(tokens.access_token)
        .bind(tokens.refresh_token)
        .bind(tokens.last_refresh)
        .bind(tokens.raw_auth_json)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(account)
    }

    pub async fn get_account(&self, id: &str) -> Result<Option<Account>> {
        let sql = format!("SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE id = ?");
        let row = sqlx::query_as::<_, AccountRow>(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(Account::try_from).transpose()
    }

    pub async fn require_account(&self, id: &str) -> Result<Account> {
        self.get_account(id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(id.to_string()))
    }

    pub async fn get_account_by_chatgpt_account_id(
        &self,
        chatgpt_account_id: &str,
    ) -> Result<Option<Account>> {
        let sql = format!("SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE chatgpt_account_id = ?");
        let row = sqlx::query_as::<_, AccountRow>(&sql)
            .bind(chatgpt_account_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(Account::try_from).transpose()
    }

    pub async fn list_accounts(&self) -> Result<Vec<Account>> {
        self.list_accounts_filtered(None).await
    }

    pub async fn list_accounts_filtered(
        &self,
        status: Option<AccountStatus>,
    ) -> Result<Vec<Account>> {
        let rows = if let Some(status) = status {
            let sql = format!(
                "SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE status = ? ORDER BY created_at"
            );
            sqlx::query_as::<_, AccountRow>(&sql)
                .bind(status.as_str())
                .fetch_all(&self.pool)
                .await?
        } else {
            let sql = format!("SELECT {ACCOUNT_COLUMNS} FROM accounts ORDER BY created_at");
            sqlx::query_as::<_, AccountRow>(&sql)
                .fetch_all(&self.pool)
                .await?
        };
        rows.into_iter().map(Account::try_from).collect()
    }

    pub async fn update_account(&self, id: &str, update: AccountUpdate) -> Result<Account> {
        let mut account = self
            .get_account(id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(id.to_string()))?;
        if let Some(status) = update.status {
            account.status = status;
        }
        if let Some(display_name) = update.display_name {
            account.display_name = empty_to_none(Some(display_name));
        }
        if let Some(chatgpt_account_id) = update.chatgpt_account_id {
            account.chatgpt_account_id = empty_to_none(Some(chatgpt_account_id));
        }
        if let Some(chatgpt_user_id) = update.chatgpt_user_id {
            account.chatgpt_user_id = empty_to_none(Some(chatgpt_user_id));
        }
        if let Some(email) = update.email {
            account.email = empty_to_none(Some(email));
        }
        if let Some(plan_type) = update.plan_type {
            account.plan_type = empty_to_none(Some(plan_type));
        }
        if let Some(originator) = update.originator {
            account.originator = originator;
        }
        if let Some(user_agent) = update.user_agent {
            account.user_agent = user_agent;
        }
        if let Some(os_type) = update.os_type {
            account.os_type = os_type;
        }
        if let Some(os_version) = update.os_version {
            account.os_version = os_version;
        }
        if let Some(arch) = update.arch {
            account.arch = arch;
        }
        if let Some(home_dir) = update.home_dir {
            account.home_dir = home_dir;
        }
        if let Some(http_fingerprint_json) = update.http_fingerprint_json {
            account.http_fingerprint_json = http_fingerprint_json;
        }
        self.save_account(&account).await
    }

    pub async fn set_account_status(&self, id: &str, status: AccountStatus) -> Result<Account> {
        self.update_account(
            id,
            AccountUpdate {
                status: Some(status),
                ..AccountUpdate::default()
            },
        )
        .await
    }

    pub async fn touch_account(&self, id: &str) -> Result<Account> {
        let now = now_rfc3339();
        let res = sqlx::query("UPDATE accounts SET last_used_at = ?, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StorageError::AccountNotFound(id.to_string()));
        }
        self.get_account(id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(id.to_string()))
    }

    pub async fn delete_account(&self, id: &str) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM oauth_pending WHERE account_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let res = sqlx::query("DELETE FROM accounts WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(res.rows_affected() > 0)
    }

    async fn save_account(&self, account: &Account) -> Result<Account> {
        let now = now_rfc3339();
        let res = sqlx::query(
            "UPDATE accounts SET
                status = ?, display_name = ?, chatgpt_account_id = ?, chatgpt_user_id = ?,
                email = ?, plan_type = ?, originator = ?, user_agent = ?, os_type = ?,
                os_version = ?, arch = ?, home_dir = ?, http_fingerprint_json = ?,
                updated_at = ?, last_used_at = ?
             WHERE id = ?",
        )
        .bind(account.status.as_str())
        .bind(&account.display_name)
        .bind(&account.chatgpt_account_id)
        .bind(&account.chatgpt_user_id)
        .bind(&account.email)
        .bind(&account.plan_type)
        .bind(&account.originator)
        .bind(&account.user_agent)
        .bind(&account.os_type)
        .bind(&account.os_version)
        .bind(&account.arch)
        .bind(&account.home_dir)
        .bind(&account.http_fingerprint_json)
        .bind(&now)
        .bind(&account.last_used_at)
        .bind(&account.id)
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(StorageError::AccountNotFound(account.id.clone()));
        }
        self.get_account(&account.id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(account.id.clone()))
    }

    /// Update fingerprint and outbound proxy together, preserving account state and identity.
    pub async fn save_account_fingerprint(&self, account: &Account) -> Result<Account> {
        let result = sqlx::query(
            "UPDATE accounts SET originator = ?, user_agent = ?, os_type = ?, os_version = ?,
             arch = ?, http_fingerprint_json = ?, proxy_id = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&account.originator)
        .bind(&account.user_agent)
        .bind(&account.os_type)
        .bind(&account.os_version)
        .bind(&account.arch)
        .bind(&account.http_fingerprint_json)
        .bind(&account.proxy_id)
        .bind(now_rfc3339())
        .bind(&account.id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(StorageError::AccountNotFound(account.id.clone()));
        }
        self.require_account(&account.id).await
    }

    // --- account tokens ---

    pub async fn upsert_account_tokens(&self, tokens: AccountTokens) -> Result<AccountTokens> {
        let updated_at = if tokens.updated_at.is_empty() {
            now_rfc3339()
        } else {
            tokens.updated_at.clone()
        };
        let row = sqlx::query_as::<_, AccountTokens>(
            "INSERT INTO account_tokens (
                account_id, auth_mode, id_token, access_token, refresh_token,
                last_refresh, raw_auth_json, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(account_id) DO UPDATE SET
                auth_mode = excluded.auth_mode,
                id_token = excluded.id_token,
                access_token = excluded.access_token,
                refresh_token = excluded.refresh_token,
                last_refresh = excluded.last_refresh,
                raw_auth_json = excluded.raw_auth_json,
                updated_at = excluded.updated_at
             RETURNING account_id, auth_mode, id_token, access_token, refresh_token,
                       last_refresh, raw_auth_json, updated_at",
        )
        .bind(&tokens.account_id)
        .bind(&tokens.auth_mode)
        .bind(&tokens.id_token)
        .bind(&tokens.access_token)
        .bind(&tokens.refresh_token)
        .bind(&tokens.last_refresh)
        .bind(&tokens.raw_auth_json)
        .bind(&updated_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn load_account_tokens(&self, account_id: &str) -> Result<Option<AccountTokens>> {
        let row = sqlx::query_as::<_, AccountTokens>(
            "SELECT account_id, auth_mode, id_token, access_token, refresh_token,
                    last_refresh, raw_auth_json, updated_at
             FROM account_tokens WHERE account_id = ?",
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    // --- oauth pending ---

    pub async fn insert_oauth_pending(
        &self,
        state: &str,
        code_verifier: &str,
        redirect_uri: &str,
        account_id: Option<&str>,
        ttl: Duration,
    ) -> Result<OAuthPending> {
        let created_at = now_rfc3339();
        let expires_at =
            (Utc::now() + duration_to_chrono(ttl)).to_rfc3339_opts(SecondsFormat::Millis, true);
        let row = sqlx::query_as::<_, OAuthPending>(
            "INSERT INTO oauth_pending (state, code_verifier, redirect_uri, account_id, created_at, expires_at)
             VALUES (?, ?, ?, ?, ?, ?)
             RETURNING *",
        )
        .bind(state)
        .bind(code_verifier)
        .bind(redirect_uri)
        .bind(account_id)
        .bind(&created_at)
        .bind(&expires_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_oauth_pending(&self, state: &str) -> Result<Option<OAuthPending>> {
        let row = sqlx::query_as::<_, OAuthPending>(
            "SELECT *
             FROM oauth_pending WHERE state = ?",
        )
        .bind(state)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        if is_expired(&row.expires_at) {
            self.delete_oauth_pending(&row.state).await?;
            return Ok(None);
        }
        Ok(Some(row))
    }

    pub async fn require_oauth_pending(&self, state: &str) -> Result<OAuthPending> {
        self.get_oauth_pending(state)
            .await?
            .ok_or(StorageError::OAuthPendingNotFound)
    }

    pub async fn set_oauth_flow(&self, state: &str, flow_data_json: &str) -> Result<OAuthPending> {
        let row = sqlx::query_as::<_, OAuthPending>(
            "UPDATE oauth_pending SET flow_data_json = ? WHERE state = ? RETURNING *",
        )
        .bind(flow_data_json)
        .bind(state)
        .fetch_optional(&self.pool)
        .await?;
        row.ok_or(StorageError::OAuthPendingNotFound)
    }

    pub async fn claim_oauth_poll(&self, state: &str, interval_seconds: u64) -> Result<bool> {
        let now = Utc::now().timestamp_millis();
        let interval_ms = i64::try_from(interval_seconds)
            .unwrap_or(i64::MAX)
            .saturating_mul(1000);
        let result = sqlx::query("UPDATE oauth_pending SET last_polled_at_ms = ? WHERE state = ? AND expires_at > ? AND (last_polled_at_ms IS NULL OR last_polled_at_ms <= ?)")
            .bind(now).bind(state).bind(now_rfc3339()).bind(now.saturating_sub(interval_ms))
            .execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_oauth_pending(&self, state: &str) -> Result<bool> {
        let res = sqlx::query("DELETE FROM oauth_pending WHERE state = ?")
            .bind(state)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn expire_oauth_pending(&self) -> Result<u64> {
        let now = now_rfc3339();
        let res = sqlx::query("DELETE FROM oauth_pending WHERE expires_at <= ?")
            .bind(&now)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    // --- proxy API keys ---

    pub async fn create_proxy_api_key(
        &self,
        account_id: &str,
        name: Option<&str>,
    ) -> Result<IssuedProxyApiKey> {
        self.get_account(account_id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(account_id.to_string()))?;
        let id = Uuid::new_v4().to_string();
        let token = generate_proxy_api_key();
        let key_hash = hash_api_key(&token);
        let key_prefix = proxy_api_key_prefix(&token);
        let created_at = now_rfc3339();
        let record = sqlx::query_as::<_, ProxyApiKey>(
            "INSERT INTO proxy_api_keys (id, account_id, name, key_hash, key_prefix, created_at, last_used_at, key_token)
             VALUES (?, ?, ?, ?, ?, ?, NULL, ?)
             RETURNING id, account_id, name, key_hash, key_prefix, created_at, last_used_at, paused_at, key_token IS NOT NULL AS can_copy",
        )
        .bind(&id)
        .bind(account_id)
        .bind(name)
        .bind(&key_hash)
        .bind(&key_prefix)
        .bind(&created_at)
        .bind(&token)
        .fetch_one(&self.pool)
        .await?;
        Ok(IssuedProxyApiKey { record, token })
    }

    pub async fn lookup_proxy_api_key_by_hash(
        &self,
        key_hash: &str,
    ) -> Result<Option<ProxyApiKey>> {
        let row = sqlx::query_as::<_, ProxyApiKey>(
            "SELECT id, account_id, name, key_hash, key_prefix, created_at, last_used_at, paused_at, key_token IS NOT NULL AS can_copy
             FROM proxy_api_keys
             WHERE key_hash = ? AND paused_at IS NULL",
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn lookup_proxy_api_key(&self, token: &str) -> Result<Option<ProxyApiKey>> {
        self.lookup_proxy_api_key_by_hash(&hash_api_key(token))
            .await
    }

    pub async fn require_proxy_api_key(&self, token: &str) -> Result<ProxyApiKey> {
        self.lookup_proxy_api_key(token)
            .await?
            .ok_or(StorageError::ApiKeyNotFound)
    }

    pub async fn list_proxy_api_keys(&self, account_id: &str) -> Result<Vec<ProxyApiKey>> {
        Ok(sqlx::query_as::<_, ProxyApiKey>(
            "SELECT id, account_id, name, key_hash, key_prefix, created_at, last_used_at, paused_at, key_token IS NOT NULL AS can_copy
             FROM proxy_api_keys WHERE account_id = ? ORDER BY created_at",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_all_proxy_api_keys(&self) -> Result<Vec<ProxyApiKey>> {
        Ok(sqlx::query_as::<_, ProxyApiKey>(
            "SELECT id, account_id, name, key_hash, key_prefix, created_at, last_used_at, paused_at, key_token IS NOT NULL AS can_copy
             FROM proxy_api_keys ORDER BY created_at, id",
        ).fetch_all(&self.pool).await?)
    }

    pub async fn get_proxy_api_key(&self, id: &str) -> Result<Option<ProxyApiKey>> {
        Ok(sqlx::query_as::<_, ProxyApiKey>(
            "SELECT id, account_id, name, key_hash, key_prefix, created_at, last_used_at, paused_at, key_token IS NOT NULL AS can_copy
             FROM proxy_api_keys WHERE id = ?",
        ).bind(id).fetch_optional(&self.pool).await?)
    }

    pub async fn bind_proxy_api_key(&self, id: &str, account_id: &str) -> Result<bool> {
        let result = sqlx::query("UPDATE proxy_api_keys SET account_id = ? WHERE id = ? AND EXISTS (SELECT 1 FROM accounts WHERE id = ? AND status IN ('active', 'disabled'))")
            .bind(account_id).bind(id).bind(account_id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn set_proxy_api_key_paused(
        &self,
        account_id: &str,
        id: &str,
        paused: bool,
    ) -> Result<bool> {
        let result =
            sqlx::query("UPDATE proxy_api_keys SET paused_at = ? WHERE account_id = ? AND id = ?")
                .bind(paused.then(now_rfc3339))
                .bind(account_id)
                .bind(id)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_proxy_api_key(&self, account_id: &str, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM proxy_api_keys WHERE account_id = ? AND id = ?")
            .bind(account_id)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn proxy_api_key_token(&self, account_id: &str, id: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar::<_, Option<String>>(
            "SELECT key_token FROM proxy_api_keys WHERE account_id = ? AND id = ?",
        )
        .bind(account_id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .flatten())
    }

    pub async fn touch_proxy_api_key(&self, id: &str) -> Result<bool> {
        let now = now_rfc3339();
        let res = sqlx::query(
            "UPDATE proxy_api_keys SET last_used_at = ? WHERE id = ? AND paused_at IS NULL",
        )
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    // --- account runtime ---

    pub async fn get_account_runtime(&self, account_id: &str) -> Result<Option<AccountRuntime>> {
        let row = sqlx::query_as::<_, AccountRuntime>(
            "SELECT account_id, session_id, extra_json, updated_at
             FROM account_runtime WHERE account_id = ?",
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn set_account_runtime(
        &self,
        account_id: &str,
        session_id: Option<&str>,
        extra_json: Option<&str>,
    ) -> Result<AccountRuntime> {
        let updated_at = now_rfc3339();
        let row = sqlx::query_as::<_, AccountRuntime>(
            "INSERT INTO account_runtime (account_id, session_id, extra_json, updated_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(account_id) DO UPDATE SET
                session_id = excluded.session_id,
                extra_json = excluded.extra_json,
                updated_at = excluded.updated_at
             RETURNING account_id, session_id, extra_json, updated_at",
        )
        .bind(account_id)
        .bind(session_id)
        .bind(extra_json)
        .bind(&updated_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn duration_to_chrono(ttl: Duration) -> chrono::Duration {
    chrono::Duration::from_std(ttl).unwrap_or_else(|_| chrono::Duration::hours(24))
}

fn is_expired(expires_at: &str) -> bool {
    match DateTime::parse_from_rfc3339(expires_at) {
        Ok(dt) => dt.with_timezone(&Utc) <= Utc::now(),
        Err(_) => true,
    }
}

fn empty_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}
