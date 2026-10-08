use crate::{Result, Storage, StorageError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct OperationPrice {
    pub provider_id: String,
    pub operation: String,
    pub price_nano_usd: Option<i64>,
    pub revision: i64,
}

impl Storage {
    pub async fn operation_price(
        &self,
        provider: &str,
        operation: &str,
    ) -> Result<Option<OperationPrice>> {
        Ok(
            sqlx::query_as("SELECT * FROM operation_prices WHERE provider_id=? AND operation=?")
                .bind(provider)
                .bind(operation)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub async fn save_operation_price(
        &self,
        price: &OperationPrice,
        expected: Option<i64>,
    ) -> Result<bool> {
        if !codex2api_core::supported_provider(&price.provider_id)
            || price.operation.is_empty()
            || price.operation.len() > 64
            || !price
                .operation
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || price.price_nano_usd.is_some_and(|price| price < 0)
        {
            return Err(StorageError::InvalidAdminUpdate("按次价格无效"));
        }
        let changed = if let Some(revision) = expected {
            sqlx::query("UPDATE operation_prices SET price_nano_usd=?,revision=revision+1 WHERE provider_id=? AND operation=? AND revision=?")
                .bind(price.price_nano_usd).bind(&price.provider_id).bind(&price.operation).bind(revision).execute(self.pool()).await?
        } else {
            sqlx::query("INSERT INTO operation_prices(provider_id,operation,price_nano_usd) VALUES(?,?,?) ON CONFLICT(provider_id,operation) DO NOTHING")
                .bind(&price.provider_id).bind(&price.operation).bind(price.price_nano_usd).execute(self.pool()).await?
        };
        Ok(changed.rows_affected() == 1)
    }
}
