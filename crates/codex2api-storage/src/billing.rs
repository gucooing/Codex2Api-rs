use crate::{Result, Storage, StorageError, UsageRecord};
use serde::{Deserialize, Serialize};

/// Integer micro-USD per million tokens; snapshot with each request.
#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct ModelPrice {
    #[serde(default = "codex2api_core::default_provider")]
    pub provider_id: String,
    pub model: String,
    pub tier: String,
    pub min_input_tokens: i64,
    pub input_rate: i64,
    pub cached_rate: i64,
    pub cache_write_rate: i64,
    pub output_rate: i64,
    pub source: String,
    pub revision: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum BillingSnapshot {
    Legacy(Vec<ModelPrice>),
    Current {
        tokens: Vec<ModelPrice>,
        images: Vec<crate::ImagePrice>,
    },
}

impl BillingSnapshot {
    pub(crate) fn charge(
        &self,
        record: &UsageRecord,
    ) -> (Option<i64>, &'static str, Option<String>, Option<String>) {
        let (tokens, images) = match self {
            Self::Legacy(tokens) => (tokens.as_slice(), None),
            Self::Current { tokens, images } => (tokens.as_slice(), Some(images)),
        };
        if !record.endpoint.contains("/images/") {
            return charge(record, tokens);
        }
        let model = record.actual_model.clone().or(record.model.clone());
        let Some(images) = images else {
            return (None, "unsupported", model, None);
        };
        let Some(usage) = record
            .image_usage_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<Vec<crate::ImageUsage>>(s).ok())
        else {
            return (None, "missing_usage", model, None);
        };
        let mut total = 0_i64;
        let mut count = 0_i64;
        let mut billing_tiers = std::collections::BTreeSet::new();
        for item in usage {
            if item.count <= 0 {
                return (None, "invalid_usage", model, None);
            }
            let Some(next) = count.checked_add(item.count) else {
                return (None, "overflow", model, None);
            };
            count = next;
            let Some(size) = item.resolution else {
                return (None, "missing_resolution", model, None);
            };
            let tier = crate::image_resolution_tier(&size);
            // Existing fixed-size snapshots keep their original exact prices.
            let same_model = |p: &&crate::ImagePrice| {
                p.provider_id == record.provider_id && Some(&p.model) == model.as_ref()
            };
            let price = images
                .iter()
                .find(|p| same_model(p) && p.resolution == size)
                .or_else(|| {
                    images
                        .iter()
                        .find(|p| same_model(p) && Some(p.resolution.as_str()) == tier.as_deref())
                })
                .or_else(|| {
                    // A partial image price table still charges using the closest
                    // configured resolution tier. Only a model with no image
                    // prices remains explicitly unpriced.
                    let requested = tier.as_deref().and_then(image_tier_rank);
                    images.iter().filter(|p| same_model(p)).min_by_key(|p| {
                        let rank = image_tier_rank(&p.resolution).unwrap_or(usize::MAX);
                        requested
                            .map(|wanted| rank.abs_diff(wanted))
                            .unwrap_or(usize::MAX)
                    })
                });
            let Some(price) = price else {
                return (None, "unpriced", model, None);
            };
            if let Some(hit_tier) = crate::image_resolution_tier(&price.resolution) {
                billing_tiers.insert(hit_tier);
            }
            let Some(value) = price
                .price_nano_usd
                .checked_mul(item.count)
                .and_then(|v| total.checked_add(v))
            else {
                return (None, "overflow", model, None);
            };
            total = value;
        }
        if record.image_count != Some(count) {
            return (None, "invalid_usage", model, None);
        }
        (
            Some(total),
            if count == 0 && record.status == "failed" {
                "not_charged"
            } else {
                "priced"
            },
            model,
            (!billing_tiers.is_empty())
                .then(|| billing_tiers.into_iter().collect::<Vec<_>>().join(", ")),
        )
    }
}

pub(crate) fn validate_model_price(price: &ModelPrice) -> Result<()> {
    if price.model.is_empty()
        || price.model.len() > 256
        || !price
            .model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._:/".contains(&b))
        || !matches!(price.tier.as_str(), "standard" | "fast" | "flex")
        || !(0..=10_000_000).contains(&price.min_input_tokens)
        || [
            price.input_rate,
            price.cached_rate,
            price.cache_write_rate,
            price.output_rate,
        ]
        .iter()
        .any(|r| !(0..=1_000_000_000_000_i64).contains(r))
    {
        return Err(StorageError::InvalidAdminUpdate("模型或价格无效"));
    }
    Ok(())
}

pub fn decimal_units(text: &str, places: u32) -> Option<i64> {
    if let Some((base, exponent)) = text.trim().split_once(['e', 'E']) {
        let exponent = exponent.parse::<i32>().ok()?;
        let precision = i32::try_from(places).ok()?.checked_add(exponent)?;
        if !(0..=18).contains(&precision) {
            return None;
        }
        return decimal_units(base, precision as u32);
    }
    let (whole, fraction) = text.trim().split_once('.').unwrap_or((text.trim(), ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > places as usize
    {
        return None;
    }
    whole
        .parse::<i64>()
        .ok()?
        .checked_mul(10i64.pow(places))?
        .checked_add(if fraction.is_empty() {
            0
        } else {
            fraction
                .parse::<i64>()
                .ok()?
                .checked_mul(10i64.pow(places - fraction.len() as u32))?
        })
}

pub fn format_units(value: i64, places: u32) -> String {
    let scale = 10i64.pow(places);
    let text = format!(
        "{}.{:0width$}",
        value / scale,
        value % scale,
        width = places as usize
    );
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

pub fn price_tier(tier: Option<&str>) -> Option<&'static str> {
    match tier {
        None | Some("auto" | "default" | "standard") => Some("standard"),
        Some("priority" | "fast") => Some("fast"),
        Some("flex") => Some("flex"),
        _ => None,
    }
}

/// No estimates for missing counters, unknown models, or unsupported modalities.
pub(crate) fn charge(
    record: &UsageRecord,
    prices: &[ModelPrice],
) -> (Option<i64>, &'static str, Option<String>, Option<String>) {
    let model = record
        .actual_model
        .as_ref()
        .or(record.model.as_ref())
        .cloned();
    if !record.endpoint.ends_with("/responses") && !record.endpoint.ends_with("/responses/compact")
    {
        return (None, "unsupported", model, None);
    }
    let (Some(input), Some(output)) = (record.input_tokens, record.output_tokens) else {
        return (None, "missing_usage", model, None);
    };
    let cached = record.cached_tokens.unwrap_or(0);
    let writes = record.cache_write_tokens.unwrap_or(0);
    if input < 0
        || output < 0
        || cached < 0
        || writes < 0
        || i128::from(cached) + i128::from(writes) > i128::from(input)
    {
        return (None, "invalid_usage", model, None);
    }
    let tier = price_tier(record.service_tier.as_deref());
    let price = prices
        .iter()
        .filter(|p| {
            p.provider_id == record.provider_id
                && Some(&p.model) == model.as_ref()
                && Some(p.tier.as_str()) == tier
                && p.min_input_tokens <= input
        })
        .max_by_key(|p| p.min_input_tokens);
    let Some(price) = price else {
        return (None, "unpriced", model, tier.map(str::to_owned));
    };
    let units = i128::from(input - cached - writes) * i128::from(price.input_rate)
        + i128::from(cached) * i128::from(price.cached_rate)
        + i128::from(writes) * i128::from(price.cache_write_rate)
        + i128::from(output) * i128::from(price.output_rate);
    // Round once to nano-USD; reasoning tokens are already included in output.
    match i64::try_from((units + 500) / 1000) {
        Ok(value) => (Some(value), "priced", model, Some(price.tier.clone())),
        Err(_) => (None, "overflow", model, Some(price.tier.clone())),
    }
}

fn image_tier_rank(tier: &str) -> Option<usize> {
    crate::IMAGE_RESOLUTION_TIERS
        .iter()
        .position(|(name, _)| *name == tier)
}

impl Storage {
    pub(crate) async fn billing_snapshot(&self, provider: &str) -> Result<String> {
        let mut tx = self.pool().begin().await?;
        let tokens = sqlx::query_as(
            "SELECT * FROM model_prices WHERE provider_id=? ORDER BY model,tier,min_input_tokens",
        )
        .bind(provider)
        .fetch_all(&mut *tx)
        .await?;
        let images = sqlx::query_as(
            "SELECT * FROM model_image_prices WHERE provider_id=? ORDER BY model,resolution",
        )
        .bind(provider)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(serde_json::to_string(&BillingSnapshot::Current {
            tokens,
            images,
        })?)
    }
    pub async fn model_prices(&self, provider: &str) -> Result<Vec<ModelPrice>> {
        Ok(sqlx::query_as(
            "SELECT * FROM model_prices WHERE provider_id=? ORDER BY model,tier,min_input_tokens",
        )
        .bind(provider)
        .fetch_all(self.pool())
        .await?)
    }
    pub async fn save_model_price(&self, price: &ModelPrice) -> Result<bool> {
        validate_model_price(price)?;
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "INSERT OR IGNORE INTO model_catalog(provider_id,model,kind) VALUES(?,?,'text')",
        )
        .bind(&price.provider_id)
        .bind(&price.model)
        .execute(&mut *tx)
        .await?;
        let usable: bool = sqlx::query_scalar(
            "SELECT kind='text' AND deleted=0 FROM model_catalog WHERE provider_id=? AND model=?",
        )
        .bind(&price.provider_id)
        .bind(&price.model)
        .fetch_one(&mut *tx)
        .await?;
        if !usable {
            return Err(StorageError::InvalidAdminUpdate(
                "请在模型配置中恢复模型或选择正确的计费方式",
            ));
        }
        let mut affected=sqlx::query("INSERT INTO model_prices(provider_id,model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source,revision) SELECT ?,?,?,?,?,?,?,?,'custom',1 WHERE ?=0 ON CONFLICT(provider_id,model,tier,min_input_tokens) DO NOTHING")
            .bind(&price.provider_id).bind(&price.model).bind(&price.tier).bind(price.min_input_tokens).bind(price.input_rate).bind(price.cached_rate).bind(price.cache_write_rate).bind(price.output_rate).bind(price.revision).execute(&mut *tx).await?.rows_affected();
        if affected == 0 {
            affected=sqlx::query("UPDATE model_prices SET input_rate=?,cached_rate=?,cache_write_rate=?,output_rate=?,source='custom',revision=revision+1 WHERE provider_id=? AND model=? AND tier=? AND min_input_tokens=? AND revision=?")
                .bind(price.input_rate).bind(price.cached_rate).bind(price.cache_write_rate).bind(price.output_rate).bind(&price.provider_id).bind(&price.model).bind(&price.tier).bind(price.min_input_tokens).bind(price.revision).execute(&mut *tx).await?.rows_affected();
        }
        if affected == 0 {
            return Ok(false);
        }
        sqlx::query("UPDATE model_catalog SET revision=revision+1 WHERE provider_id=? AND model=?")
            .bind(&price.provider_id)
            .bind(&price.model)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }
    pub async fn billing_summary(&self, owner: &str) -> Result<serde_json::Value> {
        let (used,pending,unknown,legacy):(i64,i64,i64,i64)=sqlx::query_as("SELECT COALESCE(SUM(cost_nano_usd),0),COALESCE(SUM(billing_status='pending'),0),COALESCE(SUM(billing_status NOT IN ('priced','pending','legacy','not_charged')),0),COALESCE(SUM(billing_status='legacy'),0) FROM usage_records WHERE subject_id=?")
            .bind(owner).fetch_one(self.pool()).await?;
        let since: Option<String> =
            sqlx::query_scalar("SELECT value FROM meta WHERE key='billing_started_at'")
                .fetch_optional(self.pool())
                .await?;
        Ok(
            serde_json::json!({"currency":"USD","used_nano_usd":used,"used_usd":format_units(used,9),"pending_requests":pending,"unpriced_requests":unknown,"legacy_requests":legacy,"started_at":since}),
        )
    }
    pub async fn unpriced_models(&self, provider: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar("SELECT DISTINCT COALESCE(actual_model,model) FROM usage_records WHERE provider_id=? AND COALESCE(actual_model,model) IS NOT NULL AND NOT EXISTS(SELECT 1 FROM model_prices p WHERE p.provider_id=usage_records.provider_id AND p.model=COALESCE(usage_records.actual_model,usage_records.model)) ORDER BY 1").bind(provider).fetch_all(self.pool()).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_billing_falls_back_to_the_nearest_configured_tier() {
        let record = UsageRecord {
            provider_id: "chatgpt".into(),
            model: Some("image-test".into()),
            endpoint: "/v1/images/generations".into(),
            image_count: Some(1),
            image_usage_json: Some(r#"[{"resolution":"3840x2160","count":1}]"#.into()),
            status: "completed".into(),
            ..Default::default()
        };
        let snapshot = BillingSnapshot::Current {
            tokens: vec![],
            images: vec![crate::ImagePrice {
                provider_id: "chatgpt".into(),
                model: "image-test".into(),
                resolution: "1K".into(),
                price_nano_usd: 42,
            }],
        };
        let charged = snapshot.charge(&record);
        assert_eq!(charged.0, Some(42));
        assert_eq!(charged.1, "priced");
        assert_eq!(charged.3.as_deref(), Some("1K"));
    }

    async fn set_test_quota(
        storage: &crate::Storage,
        value: &serde_json::Value,
        revision: i64,
    ) -> crate::Result<Option<i64>> {
        let mut plan = storage.virtual_plan("pro").await?.unwrap();
        for key in ["primary_cost_limit_usd", "weekly_cost_limit_usd"] {
            plan.config[key] = value[key].clone();
        }
        Ok(storage
            .save_virtual_plan(&plan, Some(revision))
            .await?
            .then_some(revision + 1))
    }
    #[tokio::test]
    async fn spending_windows_reset_independently_and_preserve_usage_and_owner() {
        use serde_json::json;
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("windows.sqlite");
        let storage = Storage::open(&db).await.unwrap();
        let account = crate::VirtualAccount {
            provider_id: "chatgpt".into(),
            id: "a".into(),
            username: "a".into(),
            password_hash: "unused".into(),
            name: "A".into(),
            email: "a@example.test".into(),
            plan_type: "pro".into(),
            plan_id: "pro".into(),
            subscription_expires_at: None,
            enabled: true,
            created_at: "2026-09-21".into(),
        };
        storage.save_virtual_account(&account).await.unwrap();
        // 35 days is a common boundary of the 5h and 7d windows.
        let boundary = 35 * 86400_i64;
        for (id, owner, at, input) in [
            ("old", "a", boundary - 1, 100000),
            ("new", "a", boundary, 100000),
            ("foreign", "b", boundary, 900000),
        ] {
            let mut record = UsageRecord {
                id: id.into(),
                subject_id: owner.into(),
                model: Some("gpt-6-astra".into()),
                endpoint: "/v1/responses".into(),
                requested_at_ms: at * 1000,
                status: "in_progress".into(),
                ..Default::default()
            };
            storage.insert_usage(&record).await.unwrap();
            record.input_tokens = Some(input);
            record.output_tokens = Some(0);
            record.status = "completed".into();
            storage.finish_usage(&record).await.unwrap();
        }
        let config = storage.virtual_config("a", "quota").await.unwrap();
        let mut revision = set_test_quota(
            &storage,
            &json!({"primary_cost_limit_usd":1,"weekly_cost_limit_usd":2}),
            config.revision,
        )
        .await
        .unwrap()
        .unwrap();
        let quota = storage.virtual_quota_at("a", boundary + 1).await.unwrap();
        assert_eq!(quota["billing"]["used_usd"], "2");
        assert_eq!(quota["rate_limit"]["primary_window"]["used_usd"], "1");
        assert_eq!(quota["rate_limit"]["primary_window"]["used_percent"], 100);
        assert_eq!(quota["rate_limit"]["secondary_window"]["used_percent"], 50);
        assert_eq!(quota["rate_limit"]["allowed"], false);
        let next = storage
            .virtual_quota_at("a", boundary + 18000)
            .await
            .unwrap();
        assert_eq!(next["rate_limit"]["allowed"], true);
        assert_eq!(next["rate_limit"]["primary_window"]["used_usd"], "1");
        assert_eq!(
            next["rate_limit"]["primary_window"]["reset_at"],
            boundary + 604800
        );
        assert!(next["rate_limit"]["secondary_window"].is_null());
        revision = set_test_quota(
            &storage,
            &json!({"primary_cost_limit_usd":null,"weekly_cost_limit_usd":1}),
            revision,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            storage
                .virtual_quota_at("a", boundary + 18000)
                .await
                .unwrap()["rate_limit"]["allowed"],
            false
        );
        let reset = storage
            .virtual_quota_at("a", boundary + 604800)
            .await
            .unwrap();
        assert_eq!(reset["rate_limit"]["allowed"], true);
        assert_eq!(reset["rate_limit"]["primary_window"]["used_usd"], "0");
        assert!(reset["rate_limit"]["secondary_window"].is_null());
        set_test_quota(
            &storage,
            &json!({"primary_cost_limit_usd":null,"weekly_cost_limit_usd":null}),
            revision,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            storage
                .virtual_quota_at("a", boundary + 604800)
                .await
                .unwrap()["rate_limit"]["allowed"],
            true
        );
        storage.close().await;
        let reopened = Storage::open(&db).await.unwrap();
        assert_eq!(
            reopened
                .virtual_quota_at("a", boundary + 604800)
                .await
                .unwrap()["billing"]["used_usd"],
            "2"
        );
        for key in ["primary_cost_limit_usd", "weekly_cost_limit_usd"] {
            let mut invalid = json!({"primary_cost_limit_usd":null,"weekly_cost_limit_usd":null});
            invalid[key] = json!(-1);
            assert!(crate::validate_virtual_config("quota", &invalid).is_err());
        }
    }
    #[tokio::test]
    async fn prices_snapshot_actual_model_cache_buckets_and_idempotent_charges() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("billing.sqlite");
        let storage = Storage::open(&db).await.unwrap();
        let prices = storage.model_prices("chatgpt").await.unwrap();
        let mut record = UsageRecord {
            id: "one".into(),
            subject_id: "virtual-a".into(),
            model: Some("gpt-5.6-sol".into()),
            endpoint: "/v1/responses".into(),
            status: "in_progress".into(),
            ..Default::default()
        };
        storage.insert_usage(&record).await.unwrap();
        let mut price = prices
            .iter()
            .find(|p| p.model == "gpt-6-astra" && p.tier == "standard" && p.min_input_tokens == 0)
            .unwrap()
            .clone();
        price.input_rate = 99_000_000;
        assert!(storage.save_model_price(&price).await.unwrap());
        assert!(!storage.save_model_price(&price).await.unwrap());
        record.actual_model = Some("gpt-6-astra".into());
        record.input_tokens = Some(10000);
        record.cached_tokens = Some(2000);
        record.cache_write_tokens = Some(3000);
        record.output_tokens = Some(1000);
        record.reasoning_tokens = Some(900);
        record.status = "completed".into();
        storage.finish_usage(&record).await.unwrap();
        storage.finish_usage(&record).await.unwrap();
        // 5000*10 + 2000*1 + 3000*12.5 + 1000*50 per million = $0.1395.
        assert_eq!(
            storage.billing_summary("virtual-a").await.unwrap()["used_usd"],
            "0.1395"
        );
        assert_eq!(
            storage.billing_summary("virtual-b").await.unwrap()["used_usd"],
            "0"
        );
        assert_eq!(charge(&record, &prices).0, Some(139_500_000));
        record.service_tier = Some("priority".into());
        assert_eq!(charge(&record, &prices).0, Some(279_000_000));
        record.input_tokens = Some(272001);
        assert_eq!(charge(&record, &prices).0, Some(10_988_040_000));
        record.actual_model = Some("unknown".into());
        assert_eq!(charge(&record, &prices).1, "unpriced");
        record.actual_model = None;
        record.cached_tokens = Some(999999);
        assert_eq!(charge(&record, &prices).1, "invalid_usage");
        record.input_tokens = None;
        assert_eq!(charge(&record, &prices).1, "missing_usage");
        storage.close().await;
        let reopened = Storage::open(&db).await.unwrap();
        assert_eq!(
            reopened.billing_summary("virtual-a").await.unwrap()["used_usd"],
            "0.1395"
        );
        assert_eq!(
            reopened
                .model_prices("chatgpt")
                .await
                .unwrap()
                .iter()
                .find(|p| p.model == price.model && p.tier == price.tier && p.min_input_tokens == 0)
                .unwrap()
                .input_rate,
            99_000_000
        );
    }
}
