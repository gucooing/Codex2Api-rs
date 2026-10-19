//! One catalog snapshot for every platform. PostgreSQL revisions fence cached prices at admission.
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
    pub fn model(&self, model: &str) -> Option<&ModelConfig> {
        self.models.iter().find(|m| m.model == model)
    }
    /// Operations are platform-specific; model prices are keyed by model name only.
    pub fn available(
        &self,
        provider: &str,
        model: &str,
        requirement: crate::PriceRequirement<'_>,
    ) -> bool {
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
                            || (crate::IMAGE_RESOLUTION_TIERS
                                .iter()
                                .any(|(tier, _)| *tier == p.resolution)
                                && size.is_none_or(|s| {
                                    crate::image_resolution_tier(s).as_deref()
                                        == Some(p.resolution.as_str())
                                })))
                })
            }
            Operation(operation) => self.operations.iter().any(|p| {
                p.provider_id == provider && p.operation == operation && p.price_nano_usd.is_some()
            }),
            ReadOnly => true,
        }
    }
}

pub(crate) async fn revision_on(conn: &mut sqlx::PgConnection) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT COALESCE((SELECT revision FROM billing_cache_revision),0)")
            .fetch_one(conn)
            .await?,
    )
}

async fn load_on(conn: &mut sqlx::PgConnection) -> Result<Arc<BillingCatalog>> {
    let revision = revision_on(conn).await?;
    let models = sqlx::query_as("SELECT * FROM model_catalog ORDER BY platform,model")
        .fetch_all(&mut *conn)
        .await?;
    let tokens: Vec<ModelPrice> =
        sqlx::query_as("SELECT * FROM model_prices ORDER BY model,tier,min_input_tokens")
            .fetch_all(&mut *conn)
            .await?;
    let images: Vec<ImagePrice> =
        sqlx::query_as("SELECT * FROM model_image_prices ORDER BY model,resolution")
            .fetch_all(&mut *conn)
            .await?;
    let operations: Vec<OperationPrice> =
        sqlx::query_as("SELECT * FROM operation_prices ORDER BY provider_id,operation")
            .fetch_all(&mut *conn)
            .await?;
    let journal = Arc::new(serde_json::to_string(&crate::billing::BillingSnapshot {
        tokens: tokens.clone(),
        images: images.clone(),
        operations: operations.clone(),
        cost_operations: None,
    })?);
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
    pub(crate) async fn billing_catalog(&self) -> Result<Arc<BillingCatalog>> {
        self.runtime
            .snapshots
            .read("billing-snapshot:all".to_owned(), async {
                let mut tx = self
                    .pool()
                    .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
                    .await?;
                let value = load_on(&mut tx).await?;
                tx.commit().await?;
                Ok((value, Duration::from_secs(300)))
            })
            .await
    }
    pub(crate) async fn billing_catalog_on(
        &self,
        conn: &mut sqlx::PgConnection,
        cached: Arc<BillingCatalog>,
    ) -> Result<Arc<BillingCatalog>> {
        sqlx::query("SELECT id FROM billing_cache_revision FOR SHARE")
            .execute(&mut *conn)
            .await?;
        if cached.revision == revision_on(conn).await? {
            return Ok(cached);
        }
        // The current transaction is authoritative. Never charge a newer request
        // from a snapshot loaded before a concurrent price/catalog edit.
        self.runtime
            .snapshots
            .invalidate_key("billing-snapshot:all");
        load_on(conn).await
    }
}
