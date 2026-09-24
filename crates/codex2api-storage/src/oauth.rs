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
