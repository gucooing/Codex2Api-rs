use crate::{
    AdminState,
    rest::{
        dto,
        dto::OptionsQuery,
        error::{ApiError, ApiResult, ok},
    },
};
use axum::{
    Json, Router,
    extract::{Query, State},
    routing::{get, post},
};
use codex2api_storage::{ImagePrice, ModelConfig, ModelPrice, decimal_units, format_units};
use serde::Deserialize;
use serde_json::json;

pub(super) fn router() -> Router<AdminState> {
    Router::new()
        .route(
            "/billing/chatgpt/search",
            get(crate::providers::chatgpt::billing::search_price)
                .put(crate::providers::chatgpt::billing::save_search_price),
        )
        .route(
            "/models/grok/sync",
            post(crate::providers::grok::sync_all_models),
        )
        .route("/models", get(models).post(save_model))
        .route("/models/options", get(model_options))
        .route("/models/presets", get(model_presets))
        .route("/models/status", post(model_status))
        .route("/models/delete", post(delete_model))
}

pub async fn model_options(
    State(s): State<AdminState>,
    Query(q): Query<OptionsQuery>,
) -> ApiResult {
    Ok(Json(
        json!({"items":s.storage.model_options(&q.provider_id,&q.search,&q.plan_id).await?}),
    ))
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

pub async fn models(
    State(s): State<AdminState>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    let page = s.storage.model_page(&q).await?;
    let mut items = Vec::with_capacity(page.items.len());
    for m in page.items {
        let tokens = s
            .storage
            .model_token_prices(&m.provider_id, &m.model)
            .await?;
        let images = s
            .storage
            .model_image_prices(&m.provider_id, &m.model)
            .await?;
        items.push(dto::Model {
            provider_id: m.provider_id,
            model: m.model,
            kind: m.kind,
            enabled: m.enabled,
            revision: m.revision,
            token_prices: tokens.iter().map(token_dto).collect(),
            image_prices: images
                .iter()
                .map(|p| dto::ImagePrice {
                    resolution: p.resolution.clone(),
                    price: format_units(p.price_nano_usd, 9),
                })
                .collect(),
        });
    }
    Ok(Json(
        json!({"items":items,"total":page.total,"page":page.page,"page_size":page.page_size}),
    ))
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
