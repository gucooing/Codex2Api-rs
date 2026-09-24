use crate::{QuotaSnapshot, Result, Storage, SupplierAccount, SupplierTokens};
use chrono::{DateTime, Utc};

/// Credentials and their generation read together; never expose this record in an API.
#[derive(sqlx::FromRow)]
pub struct SupplierAuthSnapshot {
    #[sqlx(flatten)]
    pub tokens: SupplierTokens,
    pub auth_revision: i64,
    pub chatgpt_account_id: Option<String>,
}

impl Storage {
    pub async fn supplier_auth_snapshot(&self, id: &str) -> Result<Option<SupplierAuthSnapshot>> {
        Ok(sqlx::query_as("SELECT t.*, a.auth_revision, a.chatgpt_account_id FROM supplier_tokens t JOIN supplier_accounts a ON a.id=t.account_id WHERE a.id=?")
            .bind(id).fetch_optional(self.pool()).await?)
    }

    pub async fn supplier_auth_revision(&self, id: &str) -> Result<Option<i64>> {
        Ok(
            sqlx::query_scalar("SELECT auth_revision FROM supplier_accounts WHERE id=?")
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub async fn supplier_routing_snapshot(
        &self,
        id: &str,
    ) -> Result<Option<(QuotaSnapshot, Option<i64>)>> {
        let row: Option<(String, DateTime<Utc>, Option<i64>)> = sqlx::query_as(
            "SELECT response_json, observed_at, auth_revision FROM supplier_info_cache WHERE account_id=? AND section='details'",
        ).bind(id).fetch_optional(self.pool()).await?;
        row.map(|(json, observed_at, revision)| {
            Ok((
                QuotaSnapshot {
                    value: serde_json::from_str(&json)?,
                    observed_at,
                },
                revision,
            ))
        })
        .transpose()
    }

    /// A discovery made with superseded credentials must not replace the current snapshot.
    pub async fn store_supplier_routing_snapshot(
        &self,
        id: &str,
        revision: i64,
        snapshot: &QuotaSnapshot,
    ) -> Result<bool> {
        Ok(sqlx::query("INSERT INTO supplier_info_cache(account_id,section,response_json,observed_at,auth_revision) SELECT id,'details',?,?,auth_revision FROM supplier_accounts WHERE id=? AND auth_revision=? ON CONFLICT(account_id,section) DO UPDATE SET response_json=excluded.response_json,observed_at=excluded.observed_at,auth_revision=excluded.auth_revision")
            .bind(serde_json::to_string(&snapshot.value)?).bind(snapshot.observed_at).bind(id).bind(revision)
            .execute(self.pool()).await?.rows_affected() == 1)
    }

    /// Compare-and-set only derived UA fields; preserve concurrent fingerprint edits.
    pub async fn align_supplier_user_agent(
        &self,
        account: &SupplierAccount,
        user_agent: &str,
        fingerprint: &str,
    ) -> Result<bool> {
        Ok(sqlx::query("UPDATE supplier_accounts SET user_agent=?,http_fingerprint_json=? WHERE id=? AND user_agent=? AND http_fingerprint_json=?")
            .bind(user_agent).bind(fingerprint).bind(&account.id).bind(&account.user_agent).bind(&account.http_fingerprint_json)
            .execute(self.pool()).await?.rows_affected() == 1)
    }
}
