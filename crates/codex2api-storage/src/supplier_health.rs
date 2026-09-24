use crate::{Result, Storage};
use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, Default, FromRow, Serialize)]
pub struct SupplierHealth {
    pub error_message: Option<String>,
    pub error_at: Option<String>,
    pub revision: i64,
}

impl Storage {
    pub async fn supplier_health(&self, id: &str) -> Result<SupplierHealth> {
        Ok(sqlx::query_as(
            "SELECT error_message,error_at,revision FROM supplier_health WHERE account_id=?",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?
        .unwrap_or_default())
    }

    /// Callers pass a fixed, credential-free description, never an upstream body or URL.
    pub async fn record_supplier_error(&self, id: &str, message: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO supplier_health(account_id,error_message,error_at,revision)
            SELECT id,?,strftime('%Y-%m-%dT%H:%M:%fZ','now'),1 FROM supplier_accounts WHERE id=?
            ON CONFLICT(account_id) DO UPDATE SET error_message=excluded.error_message,
            error_at=excluded.error_at,revision=supplier_health.revision+1",
        )
        .bind(message)
        .bind(id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// A successful recovery cannot erase a newer failure from another request.
    pub async fn recover_supplier(&self, id: &str, revision: i64) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE supplier_health SET error_message=NULL,error_at=NULL,revision=revision+1
            WHERE account_id=? AND revision=? AND error_message IS NOT NULL",
        )
        .bind(id)
        .bind(revision)
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected() == 1)
    }
}
