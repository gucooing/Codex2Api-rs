//! Local Grok OAuth secrets and account-owned protocol state.
use crate::{Result, Storage, hash_token};
mod models;

impl Storage {
    pub async fn grok_oauth_key(&self, candidate: Option<&str>) -> Result<Option<String>> {
        if let Some(pem) = candidate {
            sqlx::query("INSERT OR IGNORE INTO meta(key,value) VALUES('grok_oauth_private_key',?)")
                .bind(pem)
                .execute(self.pool())
                .await?;
        }
        Ok(
            sqlx::query_scalar("SELECT value FROM meta WHERE key='grok_oauth_private_key'")
                .fetch_optional(self.pool())
                .await?,
        )
    }
    pub async fn grok_code_nonce(&self, code: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar::<_,Option<String>>("SELECT oidc_nonce FROM virtual_authorization_codes WHERE code_hash=? AND provider_id='grok'")
            .bind(hash_token(code)).fetch_optional(self.pool()).await?.flatten())
    }
    /// Rotate refresh and register access together. A consumed refresh token cannot race a
    /// disable, password change, revocation, or another successful refresh.
    pub async fn rotate_grok_access(
        &self,
        device: &str,
        old_refresh: &str,
        refresh: &str,
        access: &str,
        expires: i64,
        scopes: &str,
    ) -> Result<bool> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let granted:Option<String> = sqlx::query_scalar("SELECT d.scopes FROM virtual_devices d JOIN platform_principals v ON v.id=d.virtual_account_id WHERE d.id=? AND d.refresh_hash=? AND d.provider_id='grok' AND v.provider_id='grok' AND v.enabled=1")
            .bind(device).bind(hash_token(old_refresh)).fetch_optional(&mut *tx).await?;
        let Some(granted) = granted else {
            return Ok(false);
        };
        if !scopes
            .split_whitespace()
            .all(|s| granted.split_whitespace().any(|g| g == s))
        {
            return Ok(false);
        }
        sqlx::query("UPDATE virtual_devices SET refresh_hash=?,last_login_at=? WHERE id=?")
            .bind(hash_token(refresh))
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(device)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO virtual_access_tokens(token_hash,device_id,expires_at,scopes) VALUES(?,?,?,?)")
            .bind(hash_token(access)).bind(device).bind(expires).bind(scopes).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM virtual_access_tokens WHERE expires_at<=?")
            .bind(chrono::Utc::now().timestamp())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }
}
