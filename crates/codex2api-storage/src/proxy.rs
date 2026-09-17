use crate::{Result, Storage, StorageError};
use sqlx::FromRow;

#[derive(Clone, FromRow)]
pub struct OutboundProxy {
    pub id: String,
    pub name: String,
    pub url: String,
    pub created_at: String,
    pub exit_ip: Option<String>,
    pub country_code: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub timezone: Option<String>,
    pub connection_ok: Option<bool>,
    pub connection_latency_ms: Option<i64>,
    pub connection_error: Option<String>,
    pub connection_checked_at: Option<String>,
    pub quality_ok: Option<bool>,
    pub quality_latency_ms: Option<i64>,
    pub quality_http_status: Option<i64>,
    pub quality_error: Option<String>,
    pub quality_checked_at: Option<String>,
}

#[derive(Default)]
pub struct ProxyConnectionCheck {
    pub ok: bool,
    pub latency_ms: i64,
    pub error: Option<String>,
    pub exit_ip: Option<String>,
    pub country_code: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Default)]
pub struct ProxyQualityCheck {
    pub ok: bool,
    pub latency_ms: i64,
    pub http_status: Option<i64>,
    pub error: Option<String>,
}

/// Parse only explicit supported proxy URLs. Never include credentials in errors.
pub fn parse_proxy_url(value: &str) -> Result<url::Url> {
    let parsed = url::Url::parse(value).map_err(|_| StorageError::InvalidProxy)?;
    if !matches!(parsed.scheme(), "http" | "https" | "socks5" | "socks5h")
        || parsed.host_str().is_none()
        || parsed.port() == Some(0)
        || !matches!(parsed.path(), "" | "/")
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || value.chars().any(char::is_whitespace)
    {
        return Err(StorageError::InvalidProxy);
    }
    Ok(parsed)
}

impl OutboundProxy {
    pub fn display_url(&self) -> String {
        let Ok(mut url) = parse_proxy_url(&self.url) else {
            return "无效代理地址".into();
        };
        let _ = url.set_username("");
        let _ = url.set_password(None);
        url.to_string()
    }
}

impl Storage {
    pub async fn outbound_proxy_account_counts(
        &self,
    ) -> Result<std::collections::HashMap<String, i64>> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT proxy_id,COUNT(*) FROM accounts WHERE proxy_id IS NOT NULL GROUP BY proxy_id",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().collect())
    }

    pub async fn save_proxy_connection_check(
        &self,
        proxy: &OutboundProxy,
        check: &ProxyConnectionCheck,
    ) -> Result<OutboundProxy> {
        let result = sqlx::query("UPDATE outbound_proxies SET connection_ok=?, connection_latency_ms=?, connection_error=?, connection_checked_at=?, exit_ip=?, country_code=?, country=?, region=?, city=?, timezone=? WHERE id=? AND url=?")
            .bind(check.ok).bind(check.latency_ms).bind(&check.error).bind(chrono::Utc::now().to_rfc3339())
            .bind(&check.exit_ip).bind(&check.country_code).bind(&check.country).bind(&check.region).bind(&check.city).bind(&check.timezone).bind(&proxy.id).bind(&proxy.url)
            .execute(self.pool()).await?;
        let current = self.require_outbound_proxy(&proxy.id).await?;
        if result.rows_affected() == 0 {
            return Err(StorageError::ProxyChanged);
        }
        Ok(current)
    }

    pub async fn save_proxy_quality_check(
        &self,
        proxy: &OutboundProxy,
        check: &ProxyQualityCheck,
    ) -> Result<OutboundProxy> {
        let result = sqlx::query("UPDATE outbound_proxies SET quality_ok=?, quality_latency_ms=?, quality_http_status=?, quality_error=?, quality_checked_at=? WHERE id=? AND url=?")
            .bind(check.ok).bind(check.latency_ms).bind(check.http_status).bind(&check.error).bind(chrono::Utc::now().to_rfc3339()).bind(&proxy.id).bind(&proxy.url)
            .execute(self.pool()).await?;
        let current = self.require_outbound_proxy(&proxy.id).await?;
        if result.rows_affected() == 0 {
            return Err(StorageError::ProxyChanged);
        }
        Ok(current)
    }

    pub async fn create_outbound_proxy(&self, name: &str, url: &str) -> Result<OutboundProxy> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 128 {
            return Err(StorageError::InvalidProxy);
        }
        let url = parse_proxy_url(url.trim())?;
        Ok(sqlx::query_as(
            "INSERT INTO outbound_proxies (id,name,url,created_at) VALUES (?,?,?,?) RETURNING *",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(name)
        .bind(url.as_str())
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(self.pool())
        .await?)
    }

    pub async fn list_outbound_proxies(&self) -> Result<Vec<OutboundProxy>> {
        Ok(
            sqlx::query_as("SELECT * FROM outbound_proxies ORDER BY created_at,id")
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub async fn update_outbound_proxy(
        &self,
        id: &str,
        name: &str,
        url: &str,
    ) -> Result<OutboundProxy> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 128 {
            return Err(StorageError::InvalidProxy);
        }
        let url = parse_proxy_url(url.trim())?;
        sqlx::query_as(
            "UPDATE outbound_proxies SET name=?1, url=?2,
            exit_ip=CASE WHEN url=?2 THEN exit_ip END,
            country_code=CASE WHEN url=?2 THEN country_code END,
            country=CASE WHEN url=?2 THEN country END,
            region=CASE WHEN url=?2 THEN region END,
            city=CASE WHEN url=?2 THEN city END,
            timezone=CASE WHEN url=?2 THEN timezone END,
            connection_ok=CASE WHEN url=?2 THEN connection_ok END,
            connection_latency_ms=CASE WHEN url=?2 THEN connection_latency_ms END,
            connection_error=CASE WHEN url=?2 THEN connection_error END,
            connection_checked_at=CASE WHEN url=?2 THEN connection_checked_at END,
            quality_ok=CASE WHEN url=?2 THEN quality_ok END,
            quality_latency_ms=CASE WHEN url=?2 THEN quality_latency_ms END,
            quality_http_status=CASE WHEN url=?2 THEN quality_http_status END,
            quality_error=CASE WHEN url=?2 THEN quality_error END,
            quality_checked_at=CASE WHEN url=?2 THEN quality_checked_at END
            WHERE id=?3 RETURNING *",
        )
        .bind(name)
        .bind(url.as_str())
        .bind(id)
        .fetch_optional(self.pool())
        .await?
        .ok_or(StorageError::ProxyNotFound)
    }

    pub async fn require_outbound_proxy(&self, id: &str) -> Result<OutboundProxy> {
        sqlx::query_as("SELECT * FROM outbound_proxies WHERE id=?")
            .bind(id)
            .fetch_optional(self.pool())
            .await?
            .ok_or(StorageError::ProxyNotFound)
    }

    pub async fn delete_outbound_proxy(&self, id: &str, confirm_unbind: bool) -> Result<bool> {
        let mut tx = self.pool().begin().await?;
        if !confirm_unbind {
            let result = sqlx::query("DELETE FROM outbound_proxies WHERE id=? AND NOT EXISTS (SELECT 1 FROM accounts WHERE proxy_id=?)")
                .bind(id).bind(id).execute(&mut *tx).await?;
            if result.rows_affected() > 0 {
                tx.commit().await?;
                return Ok(true);
            }
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE proxy_id=?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
            if count > 0 {
                return Err(StorageError::ProxyHasBindings(count));
            }
            tx.commit().await?;
            return Ok(false);
        }
        sqlx::query("UPDATE accounts SET proxy_id=NULL, updated_at=? WHERE proxy_id=?")
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let result = sqlx::query("DELETE FROM outbound_proxies WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn set_account_proxy(
        &self,
        account_id: &str,
        proxy_id: Option<&str>,
    ) -> Result<crate::Account> {
        if let Some(id) = proxy_id {
            self.require_outbound_proxy(id).await?;
        }
        let result = sqlx::query("UPDATE accounts SET proxy_id=?, updated_at=? WHERE id=?")
            .bind(proxy_id)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(account_id)
            .execute(self.pool())
            .await?;
        if result.rows_affected() == 0 {
            return Err(StorageError::AccountNotFound(account_id.to_owned()));
        }
        self.require_account(account_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn proxies_and_account_binding_persist_and_invalid_urls_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("proxies.sqlite");
        let storage = Storage::open(&path).await.unwrap();
        for scheme in ["http", "https", "socks5", "socks5h"] {
            let proxy = storage
                .create_outbound_proxy(scheme, &format!("{scheme}://user:secret@localhost:1080"))
                .await
                .unwrap();
            assert!(!proxy.display_url().contains("secret"));
            assert!(!proxy.display_url().contains("user"));
        }
        for value in [
            "ftp://localhost:80",
            "localhost:1080",
            "http://",
            "http://localhost:0",
            "http://localhost/path",
            "http://localhost?x=1",
            "socks5h://localhost#x",
        ] {
            assert!(
                storage.create_outbound_proxy("bad", value).await.is_err(),
                "{value}"
            );
        }
        let proxy = storage.list_outbound_proxies().await.unwrap().remove(0);
        let mut account = storage
            .create_account(crate::NewAccount::pending_identity(
                uuid::Uuid::new_v4().to_string(),
                "codex_cli_rs",
                "ua",
                "Windows",
                "10",
                "x86_64",
                "",
                "{}",
            ))
            .await
            .unwrap();
        account.proxy_id = Some(proxy.id.clone());
        storage.save_account_fingerprint(&account).await.unwrap();
        storage.close().await;
        let storage = Storage::open(&path).await.unwrap();
        let mut loaded = storage.require_account(&account.id).await.unwrap();
        assert_eq!(loaded.proxy_id.as_deref(), Some(proxy.id.as_str()));
        assert_eq!(
            storage.require_outbound_proxy(&proxy.id).await.unwrap().url,
            proxy.url
        );
        loaded.proxy_id = Some("missing".into());
        assert!(storage.save_account_fingerprint(&loaded).await.is_err());
        loaded.proxy_id = None;
        storage.save_account_fingerprint(&loaded).await.unwrap();
        assert!(
            storage
                .require_account(&account.id)
                .await
                .unwrap()
                .proxy_id
                .is_none()
        );
        storage.close().await;
    }
}
