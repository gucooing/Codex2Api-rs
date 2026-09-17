use crate::{Result, Storage};
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct QuotaSnapshot {
    pub value: Value,
    pub observed_at: DateTime<Utc>,
}

impl Storage {
    pub async fn get_account_quota(&self, account_id: &str) -> Result<Option<QuotaSnapshot>> {
        let row = sqlx::query_as::<_, (String, DateTime<Utc>)>(
            "SELECT response_json, observed_at FROM account_quota_cache WHERE account_id = ?",
        )
        .bind(account_id)
        .fetch_optional(self.pool())
        .await?;
        row.map(|(json, observed_at)| {
            Ok(QuotaSnapshot {
                value: serde_json::from_str(&json)?,
                observed_at,
            })
        })
        .transpose()
    }

    pub async fn store_account_quota(
        &self,
        account_id: &str,
        snapshot: &QuotaSnapshot,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO account_quota_cache (account_id, response_json, observed_at) VALUES (?, ?, ?)
             ON CONFLICT(account_id) DO UPDATE SET
                 response_json = excluded.response_json, observed_at = excluded.observed_at",
        )
        .bind(account_id)
        .bind(serde_json::to_string(&snapshot.value)?)
        .bind(snapshot.observed_at)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn delete_account_quota(&self, account_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM account_quota_cache WHERE account_id = ?")
            .bind(account_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
