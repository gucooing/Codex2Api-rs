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
    extract::{Path, Query, State},
    routing::{get, put},
};
use codex2api_storage::{VirtualPlan, plan_spending_windows};
use serde::Deserialize;
use serde_json::json;

pub(super) fn router() -> Router<AdminState> {
    Router::new()
        .route("/plans", get(plans).post(create_plan))
        .route("/plans/options", get(plan_options))
        .route("/plans/{id}", put(update_plan).delete(delete_plan))
}

pub async fn plan_options(State(s): State<AdminState>, Query(q): Query<OptionsQuery>) -> ApiResult {
    Ok(Json(
        json!({"items":s.storage.plan_options(&q.provider_id,q.paid_only).await?}),
    ))
}

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

pub async fn plans(
    State(s): State<AdminState>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    Ok(Json(json!(
        s.storage.plan_page(&q).await?.map(|p| plan_dto(&p))
    )))
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
