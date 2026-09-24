use crate::{ModelPrice, Result, Storage, StorageError};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::BTreeSet;

#[derive(Clone, Debug, FromRow, Serialize, Deserialize)]
pub struct ModelConfig {
    #[serde(default = "codex2api_core::default_provider")]
    pub provider_id: String,
    pub model: String,
    pub kind: String,
    pub enabled: bool,
    pub deleted: bool,
    pub revision: i64,
}

#[derive(Clone, Debug, FromRow, Serialize, Deserialize)]
pub struct ImagePrice {
    #[serde(default = "codex2api_core::default_provider")]
    pub provider_id: String,
    pub model: String,
    pub resolution: String,
    pub price_nano_usd: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageUsage {
    pub resolution: Option<String>,
    pub count: i64,
}

pub fn image_resolution(value: &str) -> Option<String> {
    let (width, height) = value.trim().split_once('x')?;
    let width = width.parse::<u32>().ok()?;
    let height = height.parse::<u32>().ok()?;
    ((1..=32768).contains(&width) && (1..=32768).contains(&height))
        .then(|| format!("{width}x{height}"))
}

/// Screen-style billing bands, independent of orientation and aspect ratio.
pub const IMAGE_RESOLUTION_TIERS: &[(&str, u32)] = &[
    ("0.5K", 512),
    ("1K", 1024),
    ("2K", 2560),
    ("4K", 4096),
    ("8K", 8192),
];

pub fn image_resolution_tier(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_uppercase();
    if let Some((tier, _)) = IMAGE_RESOLUTION_TIERS
        .iter()
        .find(|(tier, _)| *tier == normalized)
    {
        return Some((*tier).into());
    }
    let dimensions = image_resolution(value)?;
    let (width, height) = dimensions.split_once('x')?;
    let edge = width.parse::<u32>().ok()?.max(height.parse::<u32>().ok()?);
    IMAGE_RESOLUTION_TIERS
        .iter()
        .find(|(_, maximum)| edge <= *maximum)
        .map(|(tier, _)| (*tier).to_owned())
}

impl Storage {
    pub async fn model_configs(&self, provider: &str) -> Result<Vec<ModelConfig>> {
        Ok(sqlx::query_as(
            "SELECT * FROM model_catalog WHERE provider_id=? AND deleted=0 ORDER BY model",
        )
        .bind(provider)
        .fetch_all(self.pool())
        .await?)
    }
    pub async fn model_config(&self, provider: &str, model: &str) -> Result<Option<ModelConfig>> {
        Ok(
            sqlx::query_as("SELECT * FROM model_catalog WHERE provider_id=? AND model=?")
                .bind(provider)
                .bind(model)
                .fetch_optional(self.pool())
                .await?,
        )
    }
    pub async fn unavailable_models(&self, provider: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT model FROM model_catalog WHERE provider_id=? AND (enabled=0 OR deleted=1)",
        )
        .bind(provider)
        .fetch_all(self.pool())
        .await?)
    }
    pub async fn image_prices(&self, provider: &str) -> Result<Vec<ImagePrice>> {
        Ok(sqlx::query_as(
            "SELECT * FROM model_image_prices WHERE provider_id=? ORDER BY model,resolution",
        )
        .bind(provider)
        .fetch_all(self.pool())
        .await?)
    }
    /// Save the complete model form and its prices with one revision check and transaction.
    pub async fn save_model_config(
        &self,
        model: &ModelConfig,
        tokens: &[ModelPrice],
        images: &[ImagePrice],
        expected: Option<i64>,
    ) -> Result<bool> {
        if model.model.is_empty()
            || model.model.len() > 256
            || !model
                .model
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._:/".contains(&b))
            || !matches!(model.kind.as_str(), "text" | "image")
        {
            return Err(StorageError::InvalidAdminUpdate("模型名称或计费方式无效"));
        }
        if tokens.len() + images.len() > 128 {
            return Err(StorageError::InvalidAdminUpdate(
                "单个模型最多配置 128 条计费规则",
            ));
        }
        let mut keys = BTreeSet::new();
        if model.kind == "text" {
            if tokens.is_empty() || !images.is_empty() {
                return Err(StorageError::InvalidAdminUpdate(
                    "文本模型需要配置 Token 价格",
                ));
            }
            for price in tokens {
                crate::billing::validate_model_price(price)?;
                if price.provider_id != model.provider_id
                    || price.model != model.model
                    || !keys.insert((price.tier.as_str(), price.min_input_tokens))
                {
                    return Err(StorageError::InvalidAdminUpdate(
                        "服务档位和 Token 起点不能重复",
                    ));
                }
            }
            if tokens.iter().any(|p| !keys.contains(&(p.tier.as_str(), 0))) {
                return Err(StorageError::InvalidAdminUpdate(
                    "每个服务档位都需要 Token 起点为 0 的基础价格",
                ));
            }
        } else {
            if images.is_empty() || !tokens.is_empty() {
                return Err(StorageError::InvalidAdminUpdate(
                    "图像模型需要配置分辨率和每张价格",
                ));
            }
            let mut sizes = BTreeSet::new();
            for price in images {
                if price.provider_id != model.provider_id
                    || price.model != model.model
                    || image_resolution_tier(&price.resolution).as_deref()
                        != Some(&price.resolution)
                    || price.price_nano_usd < 0
                    || !sizes.insert(&price.resolution)
                {
                    return Err(StorageError::InvalidAdminUpdate(
                        "分辨率档位或图像价格无效，同一档位不能重复",
                    ));
                }
            }
        }
        let mut tx = self.pool().begin().await?;
        let changed = if let Some(revision) = expected {
            sqlx::query("UPDATE model_catalog SET kind=?,enabled=?,revision=revision+1 WHERE provider_id=? AND model=? AND revision=? AND deleted=0")
                .bind(&model.kind).bind(model.enabled).bind(&model.provider_id).bind(&model.model).bind(revision).execute(&mut *tx).await?.rows_affected()
        } else {
            sqlx::query("INSERT INTO model_catalog(provider_id,model,kind,enabled) VALUES(?,?,?,?) ON CONFLICT(provider_id,model) DO UPDATE SET kind=excluded.kind,enabled=excluded.enabled,deleted=0,revision=model_catalog.revision+1 WHERE model_catalog.deleted=1")
                .bind(&model.provider_id).bind(&model.model).bind(&model.kind).bind(model.enabled).execute(&mut *tx).await?.rows_affected()
        };
        if changed == 0 {
            return Ok(false);
        }
        sqlx::query("DELETE FROM model_prices WHERE provider_id=? AND model=?")
            .bind(&model.provider_id)
            .bind(&model.model)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM model_image_prices WHERE provider_id=? AND model=?")
            .bind(&model.provider_id)
            .bind(&model.model)
            .execute(&mut *tx)
            .await?;
        for price in tokens {
            sqlx::query("INSERT INTO model_prices(provider_id,model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source,revision) VALUES(?,?,?,?,?,?,?,?,'custom',?)")
                .bind(&model.provider_id).bind(&model.model).bind(&price.tier).bind(price.min_input_tokens).bind(price.input_rate).bind(price.cached_rate).bind(price.cache_write_rate).bind(price.output_rate).bind(expected.unwrap_or(0)+1).execute(&mut *tx).await?;
        }
        for price in images {
            sqlx::query(
                "INSERT INTO model_image_prices(provider_id,model,resolution,price_nano_usd) VALUES(?,?,?,?)",
            )
            .bind(&model.provider_id).bind(&model.model)
            .bind(&price.resolution)
            .bind(price.price_nano_usd)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(true)
    }
    pub async fn set_model_enabled(
        &self,
        provider: &str,
        model: &str,
        enabled: bool,
        revision: i64,
    ) -> Result<bool> {
        Ok(sqlx::query("UPDATE model_catalog SET enabled=?,revision=revision+1 WHERE provider_id=? AND model=? AND revision=? AND deleted=0").bind(enabled).bind(provider).bind(model).bind(revision).execute(self.pool()).await?.rows_affected()==1)
    }
    pub async fn delete_model_config(
        &self,
        provider: &str,
        model: &str,
        revision: i64,
    ) -> Result<bool> {
        let mut tx = self.pool().begin().await?;
        // Keep a tombstone so stale clients cannot continue using a deleted model.
        let changed=sqlx::query("UPDATE model_catalog SET deleted=1,enabled=0,revision=revision+1 WHERE provider_id=? AND model=? AND revision=? AND deleted=0").bind(provider).bind(model).bind(revision).execute(&mut *tx).await?.rows_affected();
        if changed == 0 {
            return Ok(false);
        }
        sqlx::query("DELETE FROM model_prices WHERE provider_id=? AND model=?")
            .bind(provider)
            .bind(model)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM model_image_prices WHERE provider_id=? AND model=?")
            .bind(provider)
            .bind(model)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }
}
