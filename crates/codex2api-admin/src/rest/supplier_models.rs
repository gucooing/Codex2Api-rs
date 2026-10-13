use crate::{
    AdminState,
    rest::error::{ApiError, ApiResult, ok},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post, put},
};
use serde::Deserialize;
use serde_json::json;
pub(super) fn router() -> Router<AdminState> {
    Router::new()
        .route("/suppliers/models", put(replace))
        .route(
            "/suppliers/{id}/scheduling",
            get(scheduling).put(save_scheduling),
        )
        .route(
            "/suppliers/{id}/models",
            get(models).post(add).delete(remove),
        )
        .route("/suppliers/{id}/models/options", get(options))
        .route("/suppliers/{id}/models/sync", post(sync))
}
async fn scheduling(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    s.storage.require_account(&id).await?;
    let mut value = serde_json::to_value(s.storage.supplier_scheduling(&id).await?)
        .map_err(|_| ApiError::bad("调度配置无效"))?;
    value["runtime"] = json!(s.storage.supplier_metrics(&id));
    Ok(Json(value))
}
async fn save_scheduling(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(value): Json<codex2api_storage::SupplierScheduling>,
) -> ApiResult {
    s.storage.require_account(&id).await?;
    if !s.storage.save_supplier_scheduling(&id, &value).await? {
        return Err(ApiError::conflict());
    }
    scheduling(State(s), Path(id)).await
}
async fn models(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Query(query): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    s.storage.require_account(&id).await?;
    Ok(Json(json!(
        s.storage.supplier_model_page(&id, &query).await?
    )))
}
#[derive(Deserialize)]
struct Search {
    #[serde(default)]
    search: String,
}
async fn options(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Query(q): Query<Search>,
) -> ApiResult {
    s.storage.require_account(&id).await?;
    if q.search.len() > 1024 {
        return Err(ApiError::bad("查询过长"));
    }
    Ok(Json(
        json!({"items":s.storage.supplier_model_options(&id,&q.search).await?,
            "selected":s.storage.supplier_models(&id).await?.into_iter().map(|m|m.model).collect::<Vec<_>>()}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Model {
    model: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelBatch {
    account_ids: Vec<String>,
    models: Vec<String>,
}
async fn replace(State(s): State<AdminState>, Json(value): Json<ModelBatch>) -> ApiResult {
    s.storage
        .replace_supplier_models(&value.account_ids, &value.models)
        .await?;
    Ok(ok())
}
async fn add(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(value): Json<Model>,
) -> ApiResult {
    s.storage.require_account(&id).await?;
    s.storage.add_supplier_model(&id, &value.model).await?;
    Ok(ok())
}
async fn remove(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(value): Json<Model>,
) -> ApiResult {
    s.storage.require_account(&id).await?;
    s.storage.remove_supplier_model(&id, &value.model).await?;
    Ok(ok())
}
async fn sync(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    let account = s.storage.require_account(&id).await?;
    match account.provider_id.as_str() {
        codex2api_core::CHATGPT => crate::providers::chatgpt::sync_models(&s, &id)
            .await
            .map_err(ApiError::upstream)?,
        _ => s
            .upstream
            .sync_supplier_models(&account)
            .await
            .map_err(|e| ApiError::upstream(e.to_string()))?,
    }
    Ok(ok())
}
