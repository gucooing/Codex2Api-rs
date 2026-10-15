//! One catalog snapshot per platform. PostgreSQL revisions fence cached prices at admission.
use crate::{ImagePrice, ModelConfig, ModelPrice, OperationPrice, Result, Storage};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct BillingCatalog {
    pub revision: i64,
    pub models: Vec<ModelConfig>,
    pub tokens: Vec<ModelPrice>,
    pub images: Vec<ImagePrice>,
    pub operations: Vec<OperationPrice>,
    pub journal: Arc<String>,
}
impl BillingCatalog {
    pub fn available(&self, model: &str, requirement: crate::PriceRequirement<'_>) -> bool {
        use crate::PriceRequirement::*;
        match requirement {
            Tokens(tier) => self.tokens.iter().any(|p| {
                p.model == model && Some(p.tier.as_str()) == tier && p.min_input_tokens == 0
            }),
            Image(size) => {
                let size = size.filter(|s| *s != "auto");
                self.images.iter().any(|p| {
                    p.model == model
                        && (p.resolution == crate::IMAGE_UNIFORM_RESOLUTION
                            || size.is_none_or(|s| {
                                p.resolution == s
                                    || crate::image_resolution_tier(s).as_deref()
                                        == Some(p.resolution.as_str())
                            }))
                })
            }
            Operation(operation) => self
                .operations
                .iter()
                .any(|p| p.operation == operation && p.price_nano_usd.is_some()),
            ReadOnly => true,
        }
    }
}

pub(crate) async fn revision_on(conn: &mut sqlx::PgConnection, provider: &str) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE((SELECT revision FROM billing_cache_revisions WHERE provider_id=$1),0)",
    )
    .bind(provider)
    .fetch_one(conn)
    .await?)
}

async fn load_on(conn: &mut sqlx::PgConnection, provider: &str) -> Result<Arc<BillingCatalog>> {
    let revision = revision_on(conn, provider).await?;
    let models = sqlx::query_as("SELECT * FROM model_catalog WHERE provider_id=$1 ORDER BY model")
        .bind(provider)
        .fetch_all(&mut *conn)
        .await?;
    let tokens: Vec<ModelPrice> = sqlx::query_as(
        "SELECT * FROM model_prices WHERE provider_id=$1 ORDER BY model,tier,min_input_tokens",
    )
    .bind(provider)
    .fetch_all(&mut *conn)
    .await?;
    let images: Vec<ImagePrice> = sqlx::query_as(
        "SELECT * FROM model_image_prices WHERE provider_id=$1 ORDER BY model,resolution",
    )
    .bind(provider)
    .fetch_all(&mut *conn)
    .await?;
    let operations: Vec<OperationPrice> =
        sqlx::query_as("SELECT * FROM operation_prices WHERE provider_id=$1 ORDER BY operation")
            .bind(provider)
            .fetch_all(&mut *conn)
            .await?;
    let journal = Arc::new(serde_json::to_string(
        &crate::billing::BillingSnapshot::Current {
            tokens: tokens.clone(),
            images: images.clone(),
            operations: operations.clone(),
            cost_operations: None,
        },
    )?);
    Ok(Arc::new(BillingCatalog {
        revision,
        models,
        tokens,
        images,
        operations,
        journal,
    }))
}

impl Storage {
    pub(crate) async fn billing_catalog(&self, provider: &str) -> Result<Arc<BillingCatalog>> {
        self.runtime
            .snapshots
            .read(format!("billing-snapshot:{provider}"), async {
                let mut tx = self
                    .pool()
                    .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
                    .await?;
                let value = load_on(&mut tx, provider).await?;
                tx.commit().await?;
                Ok((value, Duration::from_secs(300)))
            })
            .await
    }
    pub(crate) async fn billing_catalog_on(
        &self,
        conn: &mut sqlx::PgConnection,
        provider: &str,
        cached: Arc<BillingCatalog>,
    ) -> Result<Arc<BillingCatalog>> {
        sqlx::query(
            "SELECT provider_id FROM billing_cache_revisions WHERE provider_id=$1 FOR SHARE",
        )
        .bind(provider)
        .execute(&mut *conn)
        .await?;
        if cached.revision == revision_on(conn, provider).await? {
            return Ok(cached);
        }
        // The current transaction is authoritative. Never charge a newer request
        // from a snapshot loaded before a concurrent price/catalog edit.
        self.runtime
            .snapshots
            .invalidate_key(&format!("billing-snapshot:{provider}"));
        load_on(conn, provider).await
    }
}
