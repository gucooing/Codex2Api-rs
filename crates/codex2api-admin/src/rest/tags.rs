use super::error::{ApiError, ApiResult, ok};
use crate::AdminState;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use serde_json::json;

pub async fn list(
    State(s): State<AdminState>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    Ok(Json(json!(s.storage.tag_page(&q).await?)))
}
pub async fn options(
    State(s): State<AdminState>,
    Query(q): Query<super::catalog::OptionsQuery>,
) -> ApiResult {
    Ok(Json(
        json!({"items":s.storage.tag_options(&q.provider_id).await?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagInput {
    provider_id: String,
    name: String,
}

pub async fn create(State(s): State<AdminState>, Json(f): Json<TagInput>) -> ApiResult {
    save(s, uuid::Uuid::new_v4().to_string(), f).await
}

pub async fn update(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(f): Json<TagInput>,
) -> ApiResult {
    if !s.storage.supplier_tags().await?.iter().any(|t| t.id == id) {
        return Err(ApiError::missing());
    }
    save(s, id, f).await
}

async fn save(s: AdminState, id: String, f: TagInput) -> ApiResult {
    if !codex2api_core::supported_provider(&f.provider_id) {
        return Err(ApiError::bad("不支持的提供商"));
    }
    s.storage
        .save_supplier_tag(&id, &f.provider_id, &f.name)
        .await?;
    Ok(ok())
}

pub async fn delete(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    s.storage.delete_supplier_tag(&id).await?;
    Ok(ok())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchInput {
    account_ids: Vec<String>,
    tag_ids: Vec<String>,
}

pub async fn batch(State(s): State<AdminState>, Json(f): Json<BatchInput>) -> ApiResult {
    s.storage
        .replace_supplier_tags(&f.account_ids, &f.tag_ids)
        .await?;
    Ok(ok())
}
