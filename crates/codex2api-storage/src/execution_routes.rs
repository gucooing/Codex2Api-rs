use crate::{Result, Storage, StorageError};
use sqlx::FromRow;

#[derive(Clone, Debug, FromRow)]
pub struct ExecutionRoute {
    pub virtual_account_id: String,
    pub provider_id: String,
    pub supplier_account_id: Option<String>,
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
        Ok(sqlx::query_as(
            "SELECT * FROM execution_routes WHERE virtual_account_id=? AND provider_id=?",
        )
        .bind(owner)
        .bind(provider)
        .fetch_optional(self.pool())
        .await?)
    }

    /// The administrator selects execution supply separately from consumer identity.
    pub async fn save_execution_route(
        &self,
        owner: &str,
        provider: &str,
        supplier: Option<&str>,
        expected: Option<i64>,
    ) -> Result<bool> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let owned: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM virtual_accounts WHERE id=? AND provider_id=?)",
        )
        .bind(owner)
        .bind(provider)
        .fetch_one(&mut *tx)
        .await?;
        if !owned {
            return Err(StorageError::Constraint(
                "Consumer and provider do not match".into(),
            ));
        }
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT revision FROM execution_routes WHERE virtual_account_id=? AND provider_id=?",
        )
        .bind(owner)
        .bind(provider)
        .fetch_optional(&mut *tx)
        .await?;
        if current != expected {
            return Ok(false);
        }
        if let Some(supplier) = supplier {
            let available: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM supplier_accounts s JOIN supplier_tokens t ON t.account_id=s.id WHERE s.id=? AND s.provider_id=? AND s.status!='pending' AND COALESCE(t.access_token,'')!='')")
                .bind(supplier).bind(provider).fetch_one(&mut *tx).await?;
            if !available {
                return Err(StorageError::OAuthCredentialUnavailable);
            }
        }
        sqlx::query("INSERT INTO execution_routes(virtual_account_id,provider_id,supplier_account_id,revision) VALUES(?,?,?,?) ON CONFLICT(virtual_account_id,provider_id) DO UPDATE SET supplier_account_id=excluded.supplier_account_id,revision=excluded.revision")
            .bind(owner).bind(provider).bind(supplier).bind(current.unwrap_or(0)+1).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }
}
