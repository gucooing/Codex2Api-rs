//! Proxy-issued credentials. These never contain upstream OAuth tokens.
use chrono::Utc;
use rand::RngCore;
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Result, Storage, StorageError, hash_api_key};

const COLUMNS: &str = "id, account_id, name, token_prefix, created_at, last_used_at, paused_at";

#[derive(Clone, Debug, FromRow)]
pub struct OAuthCredential {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub token_prefix: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub paused_at: Option<String>,
}

#[derive(Clone, Debug, FromRow)]
pub struct OAuthAccess {
    pub credential_id: String,
    pub account_id: String,
    pub name: String,
    pub token_hash: String,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Default)]
pub struct OAuthDeviceIdentity {
    pub installation_id: Option<String>,
    pub user_agent: String,
}

impl OAuthDeviceIdentity {
    pub fn new(installation_id: Option<&str>, user_agent: &str) -> Self {
        Self {
            installation_id: installation_id
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.chars().take(256).collect()),
            user_agent: user_agent.chars().take(1024).collect(),
        }
    }

    fn key(&self) -> String {
        match &self.installation_id {
            Some(id) => format!("installation:{}", hash_api_key(id)),
            None => format!("ua:{}", hash_api_key(&self.user_agent)),
        }
    }
}

#[derive(Clone, Debug, FromRow)]
pub struct OAuthDevice {
    pub credential_id: String,
    pub credential_name: String,
    pub installation_id: Option<String>,
    pub user_agent: String,
    pub first_login_at: String,
    pub last_login_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Clone, Debug, FromRow)]
pub struct OAuthAccountSummary {
    pub account_id: String,
    pub rt_count: i64,
    pub device_count: i64,
    pub last_used_at: Option<String>,
}

fn random_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl Storage {
    pub async fn create_oauth_credential(
        &self,
        account_id: &str,
        name: &str,
    ) -> Result<OAuthCredential> {
        let token = format!("c2rt_{}", random_secret());
        let row = sqlx::query_as::<_, OAuthCredential>(&format!(
            "INSERT INTO oauth_credentials (id,account_id,name,refresh_hash,refresh_token,token_prefix,created_at)
             SELECT ?,id,?,?,?,?,? FROM accounts
             WHERE id=? AND status='active' AND COALESCE(chatgpt_account_id,'') != ''
             AND EXISTS (SELECT 1 FROM account_tokens WHERE account_id=accounts.id AND COALESCE(access_token,'') != '')
             RETURNING {COLUMNS}"
        ))
        .bind(Uuid::new_v4().to_string()).bind(name).bind(hash_api_key(&token))
        .bind(&token).bind(&token[..17]).bind(Utc::now().to_rfc3339()).bind(account_id)
        .fetch_optional(self.pool()).await?;
        row.ok_or(StorageError::OAuthCredentialUnavailable)
    }

    pub async fn list_oauth_credentials(&self) -> Result<Vec<OAuthCredential>> {
        Ok(sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM oauth_credentials ORDER BY created_at,id"
        ))
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn get_oauth_credential(&self, id: &str) -> Result<Option<OAuthCredential>> {
        Ok(sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM oauth_credentials WHERE id=?"
        ))
        .bind(id)
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn oauth_credentials_for_account(
        &self,
        account_id: &str,
    ) -> Result<Vec<OAuthCredential>> {
        Ok(sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM oauth_credentials WHERE account_id=? ORDER BY created_at,id"
        ))
        .bind(account_id)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn oauth_account_summaries(&self) -> Result<Vec<OAuthAccountSummary>> {
        Ok(sqlx::query_as("SELECT c.account_id,COUNT(*) AS rt_count,MAX(c.last_used_at) AS last_used_at,
            (SELECT COUNT(DISTINCT d.device_key) FROM oauth_devices d JOIN oauth_credentials c2 ON c2.id=d.credential_id WHERE c2.account_id=c.account_id) AS device_count
            FROM oauth_credentials c GROUP BY c.account_id ORDER BY MIN(c.created_at),c.account_id")
            .fetch_all(self.pool()).await?)
    }

    pub async fn oauth_devices_for_account(&self, account_id: &str) -> Result<Vec<OAuthDevice>> {
        Ok(sqlx::query_as("SELECT d.credential_id,c.name AS credential_name,d.installation_id,d.user_agent,d.first_login_at,d.last_login_at,d.last_used_at
            FROM oauth_devices d JOIN oauth_credentials c ON c.id=d.credential_id WHERE c.account_id=?
            ORDER BY COALESCE(d.last_used_at,d.last_login_at) DESC,d.credential_id,d.device_key")
            .bind(account_id).fetch_all(self.pool()).await?)
    }

    pub async fn oauth_refresh_token(&self, id: &str) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT refresh_token FROM oauth_credentials WHERE id=?")
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub async fn lookup_oauth_refresh(&self, token: &str) -> Result<Option<OAuthCredential>> {
        Ok(sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM oauth_credentials WHERE refresh_hash=? AND paused_at IS NULL
             AND EXISTS (SELECT 1 FROM accounts a JOIN account_tokens t ON t.account_id=a.id
                WHERE a.id=oauth_credentials.account_id AND a.status='active'
                AND COALESCE(a.chatgpt_account_id,'') != '' AND COALESCE(t.access_token,'') != '')"
        ))
        .bind(hash_api_key(token))
        .fetch_optional(self.pool())
        .await?)
    }

    /// Register only tokens actually issued by this server. Altering JWT claims or signatures
    /// changes the hash and cannot authorize a request. Recheck binding while holding the write lock.
    pub async fn register_oauth_access(
        &self,
        credential: &OAuthCredential,
        refresh: &str,
        access: &str,
        expires_at: i64,
        device: &OAuthDeviceIdentity,
    ) -> Result<bool> {
        let device_key = device.key();
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM oauth_access_tokens WHERE expires_at<=?")
            .bind(Utc::now().timestamp())
            .execute(&mut *tx)
            .await?;
        let result = sqlx::query(
            "INSERT INTO oauth_access_tokens (token_hash,credential_id,expires_at,device_key)
             SELECT ?,c.id,?,? FROM oauth_credentials c JOIN accounts a ON a.id=c.account_id
             JOIN account_tokens t ON t.account_id=a.id
             WHERE c.id=? AND c.account_id=? AND c.refresh_hash=? AND c.paused_at IS NULL
             AND a.status='active' AND COALESCE(a.chatgpt_account_id,'') != '' AND COALESCE(t.access_token,'') != ''"
        ).bind(hash_api_key(access)).bind(expires_at).bind(&device_key).bind(&credential.id).bind(&credential.account_id)
            .bind(hash_api_key(refresh)).execute(&mut *tx).await?;
        if result.rows_affected() == 0 {
            return Ok(false);
        }
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO oauth_devices (credential_id,device_key,installation_id,user_agent,first_login_at,last_login_at)
            VALUES (?,?,?,?,?,?) ON CONFLICT(credential_id,device_key) DO UPDATE SET user_agent=excluded.user_agent,last_login_at=excluded.last_login_at")
            .bind(&credential.id).bind(device_key).bind(&device.installation_id).bind(&device.user_agent).bind(&now).bind(&now)
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn lookup_oauth_access_hash(&self, hash: &str) -> Result<Option<OAuthAccess>> {
        Ok(sqlx::query_as(
            "SELECT c.id AS credential_id,c.account_id,c.name,t.token_hash,t.expires_at
             FROM oauth_access_tokens t JOIN oauth_credentials c ON c.id=t.credential_id
             JOIN accounts a ON a.id=c.account_id JOIN account_tokens auth ON auth.account_id=a.id
             WHERE t.token_hash=? AND t.expires_at>? AND c.paused_at IS NULL
             AND a.status='active' AND COALESCE(auth.access_token,'') != ''",
        )
        .bind(hash)
        .bind(Utc::now().timestamp())
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn touch_oauth_access(&self, hash: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let mut tx = self.pool().begin().await?;
        sqlx::query("UPDATE oauth_credentials SET last_used_at=? WHERE id IN (SELECT credential_id FROM oauth_access_tokens WHERE token_hash=? AND expires_at>?) AND paused_at IS NULL")
            .bind(&now).bind(hash).bind(Utc::now().timestamp()).execute(&mut *tx).await?;
        sqlx::query("UPDATE oauth_devices SET last_used_at=? WHERE (credential_id,device_key) IN (SELECT credential_id,device_key FROM oauth_access_tokens WHERE token_hash=? AND expires_at>?)")
            .bind(&now).bind(hash).bind(Utc::now().timestamp()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn set_oauth_paused(&self, id: &str, paused: bool) -> Result<bool> {
        let mut tx = self.pool().begin().await?;
        let result = sqlx::query("UPDATE oauth_credentials SET paused_at=? WHERE id=?")
            .bind(paused.then(|| Utc::now().to_rfc3339()))
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if paused {
            sqlx::query("DELETE FROM oauth_access_tokens WHERE credential_id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_oauth_credential(&self, id: &str) -> Result<bool> {
        Ok(sqlx::query("DELETE FROM oauth_credentials WHERE id=?")
            .bind(id)
            .execute(self.pool())
            .await?
            .rows_affected()
            > 0)
    }

    pub async fn delete_account_oauth_credentials(&self, account_id: &str) -> Result<u64> {
        Ok(
            sqlx::query("DELETE FROM oauth_credentials WHERE account_id=?")
                .bind(account_id)
                .execute(self.pool())
                .await?
                .rows_affected(),
        )
    }

    pub async fn revoke_oauth_token(&self, token: &str) -> Result<()> {
        let hash = hash_api_key(token);
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM oauth_credentials WHERE refresh_hash=?")
            .bind(&hash)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM oauth_access_tokens WHERE token_hash=?")
            .bind(&hash)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn oauth_signing_key(&self) -> Result<String> {
        sqlx::query("INSERT OR IGNORE INTO meta (key,value) VALUES ('oauth_signing_key',?)")
            .bind(random_secret())
            .execute(self.pool())
            .await?;
        Ok(
            sqlx::query_scalar("SELECT value FROM meta WHERE key='oauth_signing_key'")
                .fetch_one(self.pool())
                .await?,
        )
    }
}
