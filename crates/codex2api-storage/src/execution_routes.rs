use crate::{Result, Storage};
use sqlx::FromRow;

#[derive(Clone, Debug, FromRow)]
pub struct ExecutionRoute {
    pub virtual_account_id: String,
    pub provider_id: String,
    pub tag_id: Option<String>,
    pub revision: i64,
}

impl Storage {
    pub async fn execution_routes(&self, owner: &str) -> Result<Vec<ExecutionRoute>> {
        Ok(sqlx::query_as(
            "SELECT * FROM execution_routes WHERE virtual_account_id=? ORDER BY provider_id",
        )
        .bind(owner)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn execution_route(
        &self,
        owner: &str,
        provider: &str,
    ) -> Result<Option<ExecutionRoute>> {
        let key = format!("execution-route:{owner}:{provider}");
        if let Some(value) = self.runtime.cached(&key) {
            return Ok(value);
        }
        let _fill = self.runtime.cache_fill(&key).await;
        if let Some(value) = self.runtime.cached(&key) {
            return Ok(value);
        }
        let epoch = self.runtime.epoch();
        let value = sqlx::query_as(
            "SELECT * FROM effective_execution_routes WHERE virtual_account_id=? AND provider_id=?",
        )
        .bind(owner)
        .bind(provider)
        .fetch_optional(self.pool())
        .await?;
        let ttl_ms = if value.is_some() {
            self.effective_entitlements(owner)
                .await?
                .expires_at
                .map_or(30_000, |at| {
                    at.saturating_mul(1000)
                        .saturating_sub(chrono::Utc::now().timestamp_millis())
                        .clamp(0, 30_000)
                })
        } else {
            30_000
        };
        self.runtime.cache(
            key,
            &value,
            epoch,
            std::time::Duration::from_millis(ttl_ms as u64),
        );
        Ok(value)
    }
}
