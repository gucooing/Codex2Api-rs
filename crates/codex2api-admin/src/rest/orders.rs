use super::error::{ApiError, ApiResult};
use crate::AdminState;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use codex2api_storage::OrderFilter;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Filters {
    user_id: Option<String>,
    plan_id: String,
    search: String,
    status: String,
    page: i64,
    limit: i64,
}
impl Default for Filters {
    fn default() -> Self {
        Self {
            user_id: None,
            plan_id: String::new(),
            search: String::new(),
            status: String::new(),
            page: 1,
            limit: 20,
        }
    }
}

pub async fn list(State(state): State<AdminState>, Query(filter): Query<Filters>) -> ApiResult {
    Ok(Json(
        state
            .storage
            .admin_subscription_orders(
                filter.user_id.as_deref(),
                &OrderFilter {
                    plan_id: filter.plan_id,
                    search: filter.search,
                    status: filter.status,
                    page: filter.page,
                    limit: filter.limit,
                },
            )
            .await?,
    ))
}
pub async fn plans(State(state): State<AdminState>) -> ApiResult {
    Ok(Json(
        serde_json::json!({"items":state.storage.order_plan_options(None).await?}),
    ))
}
pub async fn detail(State(state): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    Ok(Json(
        state
            .storage
            .subscription_order(None, &id)
            .await?
            .ok_or_else(ApiError::missing)?
            .view(true)?,
    ))
}
pub async fn cancel(State(state): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    Ok(Json(
        state
            .storage
            .cancel_subscription_order(None, &id)
            .await?
            .view(true)?,
    ))
}
