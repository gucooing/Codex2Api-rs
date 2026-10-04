use crate::{ModelPrice, Result, Storage, StorageError};
use codex2api_core::SupportedModel;

pub fn preset_model_prices(provider: &str, model: &str) -> Option<Vec<ModelPrice>> {
    Some(
        codex2api_core::model_price_preset(provider, model)?
            .into_iter()
            .map(|price| ModelPrice {
                provider_id: provider.into(),
                model: model.into(),
                tier: price.tier.into(),
                min_input_tokens: price.min_input_tokens,
                max_input_tokens: price.max_input_tokens,
                input_rate: price.input_rate,
                cached_rate: price.cached_rate,
                cache_write_rate: price.cache_write_rate,
                output_rate: price.output_rate,
                source: format!("preset:{}", codex2api_core::model_preset_source(provider).1),
                revision: 1,
            })
            .collect(),
    )
}

impl Storage {
    /// Register new provider models once, preserving every administrator decision.
    /// Run before serving requests, never from a catalog GET or a UI refresh.
    pub async fn sync_supported_models(&self, models: &[SupportedModel]) -> Result<usize> {
        let mut tx = self.pool().begin().await?;
        let mut changed = 0;
        for model in models {
            if !codex2api_core::supported_provider(&model.provider_id)
                || !codex2api_core::valid_model(&model.model)
                || !matches!(model.kind.as_str(), "text" | "image")
            {
                return Err(StorageError::InvalidAdminUpdate("内置模型配置无效"));
            }
            let prices = (model.kind == "text")
                .then(|| preset_model_prices(&model.provider_id, &model.model))
                .flatten();
            let created = sqlx::query("INSERT INTO model_catalog(provider_id,model,kind,enabled) VALUES(?,?,?,?) ON CONFLICT(provider_id,model) DO NOTHING")
                .bind(&model.provider_id).bind(&model.model).bind(&model.kind).bind(prices.is_some())
                .execute(&mut *tx).await?.rows_affected() == 1;
            let pending_key = format!("model_preset_pending:{}:{}", model.provider_id, model.model);
            let Some(prices) = prices else {
                if created {
                    sqlx::query(
                        "INSERT INTO meta(key,value) VALUES(?,'1') ON CONFLICT(key) DO NOTHING",
                    )
                    .bind(&pending_key)
                    .execute(&mut *tx)
                    .await?;
                    changed += 1;
                }
                continue;
            };
            // A disabled row is only eligible when we previously created it as
            // pending. User edits increment revision; nonempty price sets are final.
            let eligible: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM model_catalog c WHERE provider_id=? AND model=? AND kind=? AND deleted=0 AND revision=1 AND (enabled=1 OR EXISTS(SELECT 1 FROM meta WHERE key=? AND value='1')) AND NOT EXISTS(SELECT 1 FROM model_prices p WHERE p.provider_id=c.provider_id AND p.model=c.model) AND NOT EXISTS(SELECT 1 FROM model_image_prices p WHERE p.provider_id=c.provider_id AND p.model=c.model))")
                .bind(&model.provider_id).bind(&model.model).bind(&model.kind).bind(&pending_key)
                .fetch_one(&mut *tx).await?;
            if !eligible {
                continue;
            }
            for price in &prices {
                crate::billing::validate_model_price(price)?;
                sqlx::query("INSERT INTO model_prices(provider_id,model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source,max_input_tokens,revision) VALUES(?,?,?,?,?,?,?,?,?,?,1)")
                    .bind(&price.provider_id).bind(&price.model).bind(&price.tier).bind(price.min_input_tokens)
                    .bind(price.input_rate).bind(price.cached_rate).bind(price.cache_write_rate).bind(price.output_rate).bind(&price.source).bind(price.max_input_tokens)
                    .execute(&mut *tx).await?;
            }
            sqlx::query("UPDATE model_catalog SET enabled=1,revision=revision+? WHERE provider_id=? AND model=?")
                .bind(i64::from(!created)).bind(&model.provider_id).bind(&model.model).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM meta WHERE key=?")
                .bind(pending_key)
                .execute(&mut *tx)
                .await?;
            changed += 1;
        }
        tx.commit().await?;
        Ok(changed)
    }
}
