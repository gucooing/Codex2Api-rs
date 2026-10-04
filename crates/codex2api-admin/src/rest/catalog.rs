use super::dto;
use super::error::{ApiError, ApiResult, ok};
use crate::AdminState;
use axum::{
    Json,
    extract::{Path, State},
};
use codex2api_storage::{
    ImagePrice, ModelConfig, ModelPrice, VirtualPlan, decimal_units, format_units,
    plan_spending_windows,
};
use serde::Deserialize;
use serde_json::json;
fn plan_dto(p: &VirtualPlan) -> dto::Plan {
    dto::Plan {
        description: p.description().into(),
        id: p.id.clone(),
        provider_id: p.provider_id.clone(),
        name: p.name.clone(),
        plan_type: p.plan_type.clone(),
        allow_purchase: p.allow_purchase,
        revision: p.revision,
        updated_at_ms: p.updated_at_ms,
        sale_price_usd: p.config["sale_price_usd"].as_str().map(str::to_owned),
        duration_days: p.duration_days().unwrap_or(30),
        supplier_tag_id: p.config["supplier_tag_id"].as_str().map(str::to_owned),
        model_access: p.config["model_access"].as_str().unwrap_or("none").into(),
        models: p.config["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| {
                Some(dto::ModelRef {
                    provider_id: m["provider_id"].as_str()?.into(),
                    model: m["model"].as_str()?.into(),
                })
            })
            .collect(),
        spending_windows: plan_spending_windows(&p.config)
            .unwrap_or_default()
            .into_iter()
            .map(|w| dto::SpendingWindow {
                duration_seconds: w.duration_seconds,
                cost_limit_usd: w.cost_limit_usd,
            })
            .collect(),
    }
}
pub async fn plans(State(s): State<AdminState>) -> ApiResult {
    Ok(Json(dto::value(dto::Plans {
        items: s
            .storage
            .virtual_plans()
            .await?
            .iter()
            .map(plan_dto)
            .collect(),
    })))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanInput {
    description: Option<String>,
    plan_type: String,
    name: String,
    provider_id: String,
    model_access: String,
    models: Vec<ModelRef>,
    sale_price_usd: Option<String>,
    duration_days: i64,
    supplier_tag_id: Option<String>,
    spending_windows: Vec<SpendingWindowInput>,
    allow_purchase: bool,
    revision: Option<i64>,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct SpendingWindowInput {
    duration_seconds: i64,
    cost_limit_usd: Option<String>,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRef {
    provider_id: String,
    model: String,
}
pub async fn create_plan(State(s): State<AdminState>, Json(f): Json<PlanInput>) -> ApiResult {
    save_plan(s, None, f).await
}
pub async fn update_plan(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(f): Json<PlanInput>,
) -> ApiResult {
    save_plan(s, Some(id), f).await
}
async fn save_plan(s: AdminState, id: Option<String>, f: PlanInput) -> ApiResult {
    let old = match &id {
        Some(id) => Some(
            s.storage
                .virtual_plan(id)
                .await?
                .ok_or_else(ApiError::missing)?,
        ),
        None => None,
    };
    if !codex2api_core::supported_provider(&f.provider_id)
        || old.as_ref().is_some_and(|p| p.provider_id != f.provider_id)
    {
        return Err(ApiError::bad("套餐的提供商必须在创建时确定"));
    }
    if !matches!(f.model_access.as_str(), "all" | "selected" | "none")
        || (f.model_access == "selected" && f.models.is_empty())
    {
        return Err(ApiError::bad("请选择明确的模型权限范围"));
    }
    let available = s.storage.virtual_plan_model_choices(&f.provider_id).await?;
    for model in &f.models {
        let retained = old.as_ref().is_some_and(|p| {
            p.config["models"].as_array().is_some_and(|items| {
                items
                    .iter()
                    .any(|m| m["provider_id"] == model.provider_id && m["model"] == model.model)
            })
        });
        if model.provider_id != f.provider_id || (!available.contains(&model.model) && !retained) {
            return Err(ApiError::bad("请选择同提供商模型目录中的模型"));
        }
    }
    if old.is_some() && f.revision.is_none() {
        return Err(ApiError::bad("缺少套餐版本"));
    }
    let description = f
        .description
        .unwrap_or_else(|| old.as_ref().map_or("", VirtualPlan::description).to_owned());
    let config = json!({"description":description,"model_access":f.model_access,"models":if f.model_access=="selected"{f.models}else{vec![]},"spending_windows":f.spending_windows,
        "sale_price_usd":if f.plan_type=="free"{None}else{f.sale_price_usd.filter(|v|!v.trim().is_empty())},"duration_days":f.duration_days,
        "supplier_tag_id":f.supplier_tag_id.filter(|v|!v.is_empty())});
    let plan = VirtualPlan {
        id: id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        provider_id: f.provider_id,
        name: f.name,
        plan_type: f.plan_type,
        config,
        allow_purchase: f.allow_purchase,
        revision: 0,
        updated_at_ms: 0,
    };
    if !s.storage.save_virtual_plan(&plan, f.revision).await? {
        return Err(ApiError::conflict());
    }
    Ok(Json(dto::value(plan_dto(
        &s.storage
            .virtual_plan(&plan.id)
            .await?
            .ok_or_else(ApiError::missing)?,
    ))))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    revision: i64,
}
pub async fn delete_plan(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(input): Json<Revision>,
) -> ApiResult {
    if !s.storage.delete_virtual_plan(&id, input.revision).await? {
        return Err(ApiError::conflict());
    }
    Ok(ok())
}
fn token_dto(p: &ModelPrice) -> dto::TokenPrice {
    dto::TokenPrice {
        tier: p.tier.clone(),
        min_input_tokens: p.min_input_tokens,
        max_input_tokens: p.max_input_tokens,
        input_rate: format_units(p.input_rate, 6),
        cached_rate: format_units(p.cached_rate, 6),
        cache_write_rate: format_units(p.cache_write_rate, 6),
        output_rate: format_units(p.output_rate, 6),
    }
}
pub async fn models(State(s): State<AdminState>) -> ApiResult {
    let mut models = Vec::new();
    let mut tokens = Vec::new();
    let mut images = Vec::new();
    for (provider, _) in codex2api_core::PROVIDERS {
        models.extend(s.storage.model_configs(provider).await?);
        tokens.extend(s.storage.model_prices(provider).await?);
        images.extend(s.storage.image_prices(provider).await?);
    }
    Ok(Json(dto::value(dto::Items {
        items: models
            .iter()
            .map(|m| dto::Model {
                provider_id: m.provider_id.clone(),
                model: m.model.clone(),
                kind: m.kind.clone(),
                enabled: m.enabled,
                revision: m.revision,
                token_prices: tokens
                    .iter()
                    .filter(|p| p.model == m.model && p.provider_id == m.provider_id)
                    .map(token_dto)
                    .collect(),
                image_prices: images
                    .iter()
                    .filter(|p| p.model == m.model && p.provider_id == m.provider_id)
                    .map(|p| dto::ImagePrice {
                        resolution: p.resolution.clone(),
                        price: format_units(p.price_nano_usd, 9),
                    })
                    .collect(),
            })
            .collect(),
    })))
}

pub async fn model_presets(State(s): State<AdminState>) -> ApiResult {
    let mut models = codex2api_upstream::supported_models();
    models.extend(
        codex2api_core::providers::grok::PRICED_MODELS
            .iter()
            .map(|model| codex2api_core::SupportedModel {
                provider_id: codex2api_core::GROK.into(),
                model: (*model).into(),
                kind: "text".into(),
            }),
    );
    models.extend(
        s.storage
            .grok_model_descriptors()
            .await?
            .iter()
            .filter_map(|v| v["model"].as_str())
            .map(|model| codex2api_core::SupportedModel {
                provider_id: codex2api_core::GROK.into(),
                model: model.into(),
                kind: "text".into(),
            }),
    );
    models.sort_by(|a, b| (&a.provider_id, &a.model).cmp(&(&b.provider_id, &b.model)));
    models.dedup_by(|a, b| a.provider_id == b.provider_id && a.model == b.model);
    let items = models
        .into_iter()
        .map(|model| {
            let prices = codex2api_storage::preset_model_prices(&model.provider_id, &model.model);
            let (source, version) = codex2api_core::model_preset_source(&model.provider_id);
            dto::ModelPreset {
                source_url: prices.as_ref().map(|_| source.to_owned()),
                verified_at: prices.as_ref().map(|_| version),
                token_prices: prices.unwrap_or_default().iter().map(token_dto).collect(),
                provider_id: model.provider_id,
                model: model.model,
                kind: model.kind,
                version,
            }
        })
        .collect::<Vec<_>>();
    Ok(Json(dto::value(dto::Items { items })))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelInput {
    provider_id: String,
    model: String,
    kind: String,
    enabled: bool,
    revision: Option<i64>,
    #[serde(default)]
    pricing_preset: Option<String>,
    #[serde(default)]
    token_prices: Vec<TokenInput>,
    #[serde(default)]
    image_prices: Vec<ImageInput>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenInput {
    tier: String,
    min_input_tokens: i64,
    #[serde(default)]
    max_input_tokens: Option<i64>,
    input_rate: String,
    cached_rate: String,
    cache_write_rate: String,
    output_rate: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageInput {
    resolution: String,
    price: String,
}
pub async fn save_model(State(s): State<AdminState>, Json(f): Json<ModelInput>) -> ApiResult {
    if !codex2api_core::supported_provider(&f.provider_id) {
        return Err(ApiError::bad("提供商尚未接入"));
    }
    let mut tokens = if let Some(version) = f.pricing_preset.as_deref() {
        if version != codex2api_core::model_preset_source(&f.provider_id).1 {
            return Err(ApiError::conflict());
        }
        if f.kind != "text" || !f.token_prices.is_empty() || !f.image_prices.is_empty() {
            return Err(ApiError::bad("预设定价与自定义价格不能同时提交"));
        }
        codex2api_storage::preset_model_prices(&f.provider_id, f.model.trim())
            .ok_or_else(|| ApiError::bad("该模型没有完整预设价格，请填写自定义价格"))?
    } else {
        vec![]
    };
    let mut images = vec![];
    for p in f.token_prices {
        let rate = |v: &str| {
            decimal_units(v, 6)
                .ok_or_else(|| ApiError::bad("Token 单价须为非负美元金额，最多六位小数"))
        };
        tokens.push(ModelPrice {
            provider_id: f.provider_id.clone(),
            model: f.model.trim().into(),
            tier: p.tier,
            min_input_tokens: p.min_input_tokens,
            max_input_tokens: p.max_input_tokens,
            input_rate: rate(&p.input_rate)?,
            cached_rate: rate(&p.cached_rate)?,
            cache_write_rate: rate(&p.cache_write_rate)?,
            output_rate: rate(&p.output_rate)?,
            source: "custom".into(),
            revision: 0,
        });
    }
    for p in f.image_prices {
        images.push(ImagePrice {
            provider_id: f.provider_id.clone(),
            model: f.model.trim().into(),
            resolution: p.resolution.trim().to_uppercase(),
            price_nano_usd: decimal_units(&p.price, 9)
                .ok_or_else(|| ApiError::bad("图像价格须为非负美元金额，最多九位小数"))?,
        });
    }
    let config = ModelConfig {
        provider_id: f.provider_id,
        model: f.model.trim().into(),
        kind: f.kind,
        enabled: f.enabled,
        deleted: false,
        revision: f.revision.unwrap_or(0),
    };
    if !s
        .storage
        .save_model_config(&config, &tokens, &images, f.revision)
        .await?
    {
        return Err(ApiError::conflict());
    }
    Ok(ok())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelAction {
    provider_id: String,
    model: String,
    revision: i64,
    enabled: Option<bool>,
}
pub async fn model_status(State(s): State<AdminState>, Json(f): Json<ModelAction>) -> ApiResult {
    if !s
        .storage
        .set_model_enabled(
            &f.provider_id,
            &f.model,
            f.enabled.ok_or_else(|| ApiError::bad("缺少启停状态"))?,
            f.revision,
        )
        .await?
    {
        return Err(ApiError::conflict());
    }
    Ok(ok())
}
pub async fn delete_model(State(s): State<AdminState>, Json(f): Json<ModelAction>) -> ApiResult {
    if !s
        .storage
        .delete_model_config(&f.provider_id, &f.model, f.revision)
        .await?
    {
        return Err(ApiError::conflict());
    }
    Ok(ok())
}
