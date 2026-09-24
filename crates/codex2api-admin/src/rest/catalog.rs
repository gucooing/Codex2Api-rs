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
    let amount = |key: &str| {
        p.config.get(key).filter(|v| !v.is_null()).map(|v| {
            v.as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| v.to_string())
        })
    };
    let refs = |key: &str| {
        p.config[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| {
                Some(dto::ModelRef {
                    provider_id: m["provider_id"].as_str()?.into(),
                    model: m["model"].as_str()?.into(),
                })
            })
            .collect()
    };
    let windows = |free| {
        plan_spending_windows(&p.config, free)
            .unwrap_or_default()
            .into_iter()
            .map(|window| dto::SpendingWindow {
                duration_seconds: window.duration_seconds,
                cost_limit_usd: window.cost_limit_usd,
            })
            .collect()
    };
    dto::Plan {
        id: p.id.clone(),
        provider_id: p.provider_id.clone(),
        name: p.name.clone(),
        enabled: p.enabled,
        revision: p.revision,
        updated_at_ms: p.updated_at_ms,
        model_access: p.config["model_access"]
            .as_str()
            .unwrap_or("selected")
            .into(),
        models: refs("models"),
        free_model_access: p.config["free_model_access"]
            .as_str()
            .unwrap_or("none")
            .into(),
        free_models: refs("free_models"),
        free_access_enabled: p.config["free_access_enabled"].as_bool().unwrap_or(false),
        primary_cost_limit_usd: amount("primary_cost_limit_usd"),
        weekly_cost_limit_usd: amount("weekly_cost_limit_usd"),
        free_primary_cost_limit_usd: amount("free_primary_cost_limit_usd"),
        free_weekly_cost_limit_usd: amount("free_weekly_cost_limit_usd"),
        spending_windows: windows(false),
        free_spending_windows: windows(true),
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
    name: String,
    provider_id: String,
    model_access: String,
    models: Vec<ModelRef>,
    free_model_access: String,
    free_models: Vec<ModelRef>,
    free_access_enabled: bool,
    primary_cost_limit_usd: Option<String>,
    weekly_cost_limit_usd: Option<String>,
    free_primary_cost_limit_usd: Option<String>,
    free_weekly_cost_limit_usd: Option<String>,
    #[serde(default)]
    spending_windows: Option<Vec<SpendingWindowInput>>,
    #[serde(default)]
    free_spending_windows: Option<Vec<SpendingWindowInput>>,
    enabled: bool,
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
    let old = if let Some(id) = &id {
        Some(
            s.storage
                .virtual_plan(id)
                .await?
                .ok_or_else(ApiError::missing)?,
        )
    } else {
        None
    };
    if !codex2api_core::supported_provider(&f.provider_id)
        || old.as_ref().is_some_and(|p| p.provider_id != f.provider_id)
    {
        return Err(ApiError::bad("套餐的提供商必须在创建时确定"));
    }
    if !matches!(f.model_access.as_str(), "all" | "selected")
        || !matches!(f.free_model_access.as_str(), "none" | "all" | "selected")
    {
        return Err(ApiError::bad("模型访问范围无效"));
    }
    let available = s.storage.virtual_plan_model_choices(&f.provider_id).await?;
    for (key, access, selected) in [
        ("models", &f.model_access, &f.models),
        ("free_models", &f.free_model_access, &f.free_models),
    ] {
        if access == "selected" && selected.is_empty() {
            return Err(ApiError::bad("指定模型范围必须选择至少一个模型"));
        }
        for m in selected {
            let retained = old.as_ref().is_some_and(|p| {
                p.config[key].as_array().is_some_and(|items| {
                    items
                        .iter()
                        .any(|v| v["provider_id"] == m.provider_id && v["model"] == m.model)
                })
            });
            if m.provider_id != f.provider_id || (!available.contains(&m.model) && !retained) {
                return Err(ApiError::bad("请选择同提供商模型目录中的模型"));
            }
        }
    }
    let mut config = json!({"model_access":f.model_access,"models":if f.model_access=="selected"{f.models}else{vec![]},"free_model_access":f.free_model_access,"free_models":if f.free_model_access=="selected"{f.free_models}else{vec![]},"free_access_enabled":f.free_access_enabled});
    let to_windows = |input: Option<Vec<SpendingWindowInput>>,
                      outer: Option<&str>,
                      inner: Option<&str>| {
        input.map(|windows| json!(windows)).unwrap_or_else(|| json!([
            {"duration_seconds":604800,"cost_limit_usd":outer.and_then(|v| (!v.trim().is_empty()).then_some(v))},
            {"duration_seconds":18000,"cost_limit_usd":inner.and_then(|v| (!v.trim().is_empty()).then_some(v))}
        ]))
    };
    config["spending_windows"] = to_windows(
        f.spending_windows,
        f.weekly_cost_limit_usd.as_deref(),
        f.primary_cost_limit_usd.as_deref(),
    );
    config["free_spending_windows"] = to_windows(
        f.free_spending_windows,
        f.free_weekly_cost_limit_usd.as_deref(),
        f.free_primary_cost_limit_usd.as_deref(),
    );
    if old.is_some() && f.revision.is_none() {
        return Err(ApiError::bad("缺少套餐版本"));
    }
    let plan = VirtualPlan {
        id: id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        provider_id: f.provider_id,
        name: f.name,
        plan_type: old.map(|p| p.plan_type).unwrap_or_else(|| "plus".into()),
        config,
        enabled: f.enabled,
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
    Json(f): Json<Revision>,
) -> ApiResult {
    if !s.storage.delete_virtual_plan(&id, f.revision).await? {
        return Err(ApiError::conflict());
    }
    Ok(ok())
}
fn token_dto(p: &ModelPrice) -> dto::TokenPrice {
    dto::TokenPrice {
        tier: p.tier.clone(),
        min_input_tokens: p.min_input_tokens,
        input_rate: format_units(p.input_rate, 6),
        cached_rate: format_units(p.cached_rate, 6),
        cache_write_rate: format_units(p.cache_write_rate, 6),
        output_rate: format_units(p.output_rate, 6),
    }
}
pub async fn models(State(s): State<AdminState>) -> ApiResult {
    let models = s.storage.model_configs("chatgpt").await?;
    let tokens = s.storage.model_prices("chatgpt").await?;
    let images = s.storage.image_prices("chatgpt").await?;
    Ok(Json(dto::value(dto::Items {
        items: models
            .iter()
            .map(|m| dto::Model {
                provider_id: m.provider_id.clone(),
                model: m.model.clone(),
                kind: m.kind.clone(),
                enabled: m.enabled,
                revision: m.revision,
                codex_metadata_status: if m.kind == "image" {
                    "not_applicable"
                } else if codex2api_upstream::codex_model_descriptor(&m.model).is_some() {
                    "verified"
                } else {
                    "unavailable"
                },
                codex_metadata_source: if codex2api_upstream::codex_model_descriptor(&m.model)
                    .is_some()
                {
                    Some(codex2api_version::CODEX_REF_COMMIT)
                } else {
                    None
                },
                token_prices: tokens
                    .iter()
                    .filter(|p| p.model == m.model)
                    .map(token_dto)
                    .collect(),
                image_prices: images
                    .iter()
                    .filter(|p| p.model == m.model)
                    .map(|p| dto::ImagePrice {
                        resolution: p.resolution.clone(),
                        price: format_units(p.price_nano_usd, 9),
                    })
                    .collect(),
            })
            .collect(),
    })))
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
    token_prices: Vec<TokenInput>,
    #[serde(default)]
    image_prices: Vec<ImageInput>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenInput {
    tier: String,
    min_input_tokens: i64,
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
    let mut tokens = vec![];
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
