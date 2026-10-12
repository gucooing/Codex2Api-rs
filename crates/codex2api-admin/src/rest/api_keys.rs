//! Administrative inspection of regular users' API credentials and metered usage.
use crate::{AdminState, rest::error::ApiResult};
use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use codex2api_storage::ApiListQuery;
use serde_json::json;

pub(super) fn router() -> Router<AdminState> {
    Router::new()
        .route("/api-keys", get(keys))
        .route("/api-usage", get(usage))
}

async fn keys(State(state): State<AdminState>, Query(query): Query<ApiListQuery>) -> ApiResult {
    let page = state.storage.admin_api_keys(&query).await?.map(|key| {
        let active = (state.api_concurrency)(&key.id);
        let mut value = json!(key);
        value["active_requests"] = active.into();
        value
    });
    Ok(Json(json!(page)))
}

async fn usage(State(state): State<AdminState>, Query(query): Query<ApiListQuery>) -> ApiResult {
    Ok(Json(state.storage.admin_api_usage(&query).await?))
}
