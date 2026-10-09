use crate::{Result, Storage};
use sqlx::FromRow;

#[derive(Clone, Debug, FromRow)]
pub struct ExecutionRoute {
    pub virtual_account_id: String,
    pub provider_id: String,
    pub supplier_account_id: Option<String>,
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
        Ok(sqlx::query_as(
            "SELECT * FROM execution_routes WHERE virtual_account_id=? AND provider_id=?",
        )
        .bind(owner)
        .bind(provider)
        .fetch_optional(self.pool())
        .await?)
    }
}
