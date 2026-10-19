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
use codex2api_common::ModelRule;
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
        json!({"items":s.storage.plan_options(&q.provider_id).await?}),
    ))
}

fn plan_dto(p: &VirtualPlan) -> Result<dto::Plan, ApiError> {
    Ok(dto::Plan {
        rpm_limit: p.rpm_limit(),
        concurrency_limit: p.concurrency_limit(),
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
        tag_ids: p.tag_ids.clone(),
        models: p.models()?,
        spending_windows: plan_spending_windows(&p.config)
            .unwrap_or_default()
            .into_iter()
            .map(|w| dto::SpendingWindow {
                duration_seconds: w.duration_seconds,
                cost_limit_usd: w.cost_limit_usd,
            })
            .collect(),
    })
}

pub async fn plans(
    State(s): State<AdminState>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    let page = s.storage.plan_page(&q).await?;
    let mut items = Vec::with_capacity(page.items.len());
    for plan in &page.items {
        items.push(plan_dto(plan)?);
    }
    Ok(Json(
        json!({"items":items,"total":page.total,"page":page.page,"page_size":page.page_size}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanInput {
    rpm_limit: u32,
    concurrency_limit: u32,
    description: Option<String>,
    plan_type: String,
    name: String,
    provider_id: String,
    models: Vec<ModelRule>,
    sale_price_usd: Option<String>,
    duration_days: i64,
    tag_ids: Vec<String>,
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
    // Plans belong to a client platform with subscription tiers: ChatGPT or Grok.
    if codex2api_common::subscription_tiers(&f.provider_id).is_empty()
        || old.as_ref().is_some_and(|p| p.provider_id != f.provider_id)
    {
        return Err(ApiError::bad(
            "套餐的平台只能是 ChatGPT 或 Grok，且创建后不可更改",
        ));
    }
    if old.is_some() && f.revision.is_none() {
        return Err(ApiError::bad("缺少套餐版本"));
    }
    let description = f
        .description
        .unwrap_or_else(|| old.as_ref().map_or("", VirtualPlan::description).to_owned());
    let config = json!({"rpm_limit":f.rpm_limit,"concurrency_limit":f.concurrency_limit,"description":description,"models":f.models,"spending_windows":f.spending_windows,
        "sale_price_usd":f.sale_price_usd.filter(|v|!v.trim().is_empty()),"duration_days":f.duration_days});
    let plan = VirtualPlan {
        id: id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        provider_id: f.provider_id,
        name: f.name,
        plan_type: f.plan_type,
        config,
        allow_purchase: f.allow_purchase,
        revision: 0,
        updated_at_ms: 0,
        tag_ids: f.tag_ids,
    };
    if !s.storage.save_virtual_plan(&plan, f.revision).await? {
        return Err(ApiError::conflict());
    }
    Ok(Json(dto::value(plan_dto(
        &s.storage
            .virtual_plan(&plan.id)
            .await?
            .ok_or_else(ApiError::missing)?,
    )?)))
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
