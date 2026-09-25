//! Internal OAuth primitives; refresh tokens are device credentials, not admin-managed keys.
use crate::{Result, Storage};
use rand::RngCore;
#[derive(Clone, Debug, Default)]
pub struct OAuthDeviceIdentity {
    pub installation_id: Option<String>,
    pub user_agent: String,
}
impl OAuthDeviceIdentity {
    pub fn new(id: Option<&str>, ua: &str) -> Self {
        Self {
            installation_id: id
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.chars().take(256).collect()),
            user_agent: ua.chars().take(1024).collect(),
        }
    }
}
pub fn oauth_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
impl Storage {
    pub async fn oauth_session_claim_times(
        &self,
        owner: &str,
        device: &str,
    ) -> Result<(i64, i64, Option<String>)> {
        Ok(sqlx::query_as("SELECT COALESCE(d.authenticated_at_ms,unixepoch(d.created_at)*1000),COALESCE(d.requested_at_ms,unixepoch(d.created_at)*1000),COALESCE(a.subscription_started_at,a.created_at) FROM virtual_devices d JOIN virtual_accounts a ON a.id=d.virtual_account_id WHERE a.id=? AND d.id=?")
            .bind(owner).bind(device).fetch_one(self.pool()).await?)
    }

    /// Separate asymmetric OAuth key; existing HMAC keys still sign local event tickets.
    pub async fn oauth_jwt_private_key(&self) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT value FROM meta WHERE key='oauth_jwt_private_key'")
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub async fn persist_oauth_jwt_private_key(&self, pem: &str) -> Result<String> {
        sqlx::query("INSERT OR IGNORE INTO meta(key,value) VALUES('oauth_jwt_private_key',?)")
            .bind(pem)
            .execute(self.pool())
            .await?;
        Ok(
            sqlx::query_scalar("SELECT value FROM meta WHERE key='oauth_jwt_private_key'")
                .fetch_one(self.pool())
                .await?,
        )
    }

    pub async fn oauth_signing_key(&self) -> Result<String> {
        if let Some(key) =
            sqlx::query_scalar("SELECT value FROM meta WHERE key='oauth_signing_key'")
                .fetch_optional(self.pool())
                .await?
        {
            return Ok(key);
        }
        sqlx::query("INSERT OR IGNORE INTO meta(key,value) VALUES('oauth_signing_key',?)")
            .bind(oauth_secret())
            .execute(self.pool())
            .await?;
        Ok(
            sqlx::query_scalar("SELECT value FROM meta WHERE key='oauth_signing_key'")
                .fetch_one(self.pool())
                .await?,
        )
    }
}
