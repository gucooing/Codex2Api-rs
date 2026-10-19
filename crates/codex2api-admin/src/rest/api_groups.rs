//! Administrator-owned API routing and pricing policy. OAuth plans are separate.
use crate::{
    AdminState,
    rest::error::{ApiError, ApiResult, ok},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use codex2api_common::ModelRule;
use codex2api_storage::{ApiGroup, ApiListQuery};
use serde::Deserialize;
use serde_json::json;

pub(super) fn router() -> Router<AdminState> {
    Router::new()
        .route("/api-groups", get(list).post(create))
        .route("/api-groups/options", get(options))
        .route("/api-groups/{id}", get(detail).put(update).delete(delete))
}

async fn list(State(state): State<AdminState>, Query(query): Query<ApiListQuery>) -> ApiResult {
    Ok(Json(json!(state.storage.api_group_page(&query).await?)))
}

async fn options(State(state): State<AdminState>) -> ApiResult {
    let items = state
        .storage
        .api_groups()
        .await?
        .into_iter()
        .map(|group| json!({"id":group.id,"name":group.name,"enabled":group.enabled}))
        .collect::<Vec<_>>();
    Ok(Json(json!({"items":items})))
}

async fn detail(State(state): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    let group = state
        .storage
        .api_group(&id)
        .await?
        .ok_or_else(ApiError::missing)?;
    Ok(Json(json!(group)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupInput {
    name: String,
    #[serde(default)]
    description: String,
    enabled: bool,
    tag_ids: Vec<String>,
    models: Vec<ModelRule>,
    multiplier: String,
    search_price_usd: Option<String>,
    concurrency_limit: u32,
    rpm_limit: u32,
    revision: Option<i64>,
}

impl GroupInput {
    fn into_group(self, id: String) -> ApiGroup {
        ApiGroup {
            id,
            name: self.name,
            description: self.description,
            enabled: self.enabled,
            tag_ids: self.tag_ids,
            models: self.models,
            multiplier: self.multiplier,
            search_price_usd: self.search_price_usd,
            concurrency_limit: self.concurrency_limit,
            rpm_limit: self.rpm_limit,
            revision: self.revision.unwrap_or(0),
        }
    }
}

async fn create(State(state): State<AdminState>, Json(input): Json<GroupInput>) -> ApiResult {
    let group = input.into_group(uuid::Uuid::new_v4().to_string());
    if !state.storage.save_api_group(&group, None).await? {
        return Err(ApiError::conflict());
    }
    Ok(Json(json!(
        state
            .storage
            .api_group(&group.id)
            .await?
            .ok_or_else(ApiError::missing)?
    )))
}

async fn update(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    Json(input): Json<GroupInput>,
) -> ApiResult {
    let revision = input.revision.ok_or_else(|| ApiError::bad("缺少版本号"))?;
    let group = input.into_group(id);
    if !state.storage.save_api_group(&group, Some(revision)).await? {
        return Err(ApiError::conflict());
    }
    Ok(Json(json!(
        state
            .storage
            .api_group(&group.id)
            .await?
            .ok_or_else(ApiError::missing)?
    )))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    revision: i64,
}

async fn delete(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    Json(input): Json<Revision>,
) -> ApiResult {
    if !state.storage.delete_api_group(&id, input.revision).await? {
        return Err(ApiError::conflict());
    }
    Ok(ok())
}
