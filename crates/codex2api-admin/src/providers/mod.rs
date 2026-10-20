//! Provider dispatch only. Each channel owns its protocol and authentication DTOs.
pub(crate) mod antigravity;
pub(crate) mod api;
pub(crate) mod chatgpt;
pub(crate) mod grok;
use crate::{
    AdminState,
    rest::error::{ApiError, ApiResult},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use serde_json::Value;

pub(crate) fn request_identity(
    account: &codex2api_storage::SupplierAccount,
) -> Result<(&'static str, String), ApiError> {
    match account.provider_id.as_str() {
        codex2api_common::CHATGPT => chatgpt::request_identity(account),
        codex2api_common::GROK => grok::request_identity(account),
        codex2api_common::ANTIGRAVITY => antigravity::request_identity(account),
        codex2api_common::API => Ok(api::request_identity()),
        _ => Err(ApiError::bad("不支持的渠道")),
    }
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|_| ApiError::bad("渠道请求字段无效"))
}
pub async fn fingerprint_view(
    s: &AdminState,
    account: &codex2api_storage::SupplierAccount,
) -> Result<Value, ApiError> {
    match account.provider_id.as_str() {
        codex2api_common::GROK => grok::fingerprint_view(account),
        codex2api_common::ANTIGRAVITY => antigravity::fingerprint_view(account),
        codex2api_common::CHATGPT => Ok(serde_json::json!(chatgpt::Fingerprint::from_account(
            account
        )?)),
        codex2api_common::API => api::fingerprint_view(s, account).await,
        _ => Err(ApiError::bad("不支持的渠道")),
    }
}
pub async fn subscription_expiration(
    s: &AdminState,
    account: &codex2api_storage::SupplierAccount,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, ApiError> {
    match account.provider_id.as_str() {
        codex2api_common::CHATGPT => chatgpt::subscription_expiration(s, &account.id).await,
        _ => Ok(None),
    }
}
pub fn chatgpt_oauth_routes() -> Router<AdminState> {
    Router::new()
        .route("/setup", get(chatgpt::setup))
        .route("/start", post(chatgpt::start))
        .route("/callback", post(chatgpt::callback))
        .route("/poll", post(chatgpt::poll))
        .route("/cancel", post(chatgpt::cancel))
}
pub async fn quota(
    s: &AdminState,
    id: &str,
    refresh: bool,
) -> Result<codex2api_storage::QuotaSnapshot, String> {
    match s
        .storage
        .require_account(id)
        .await
        .map_err(|e| e.to_string())?
        .provider_id
        .as_str()
    {
        codex2api_common::CHATGPT => chatgpt::quota(s, id, refresh).await,
        codex2api_common::GROK => grok::quota(s, id, refresh).await,
        codex2api_common::ANTIGRAVITY => antigravity::quota(s, id, refresh).await,
        codex2api_common::API => Err("API 渠道不提供官方额度".into()),
        _ => Err("不支持的渠道".into()),
    }
}
pub async fn fingerprint(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(value): Json<Value>,
) -> ApiResult {
    match s.storage.require_account(&id).await?.provider_id.as_str() {
        codex2api_common::CHATGPT => {
            chatgpt::fingerprint(State(s), Path(id), Json(decode(value)?)).await
        }
        codex2api_common::GROK => grok::fingerprint(State(s), Path(id), Json(decode(value)?)).await,
        codex2api_common::ANTIGRAVITY => {
            antigravity::fingerprint(State(s), Path(id), Json(decode(value)?)).await
        }
        codex2api_common::API => api::fingerprint(State(s), Path(id), Json(decode(value)?)).await,
        _ => Err(ApiError::bad("不支持的渠道")),
    }
}
pub async fn relogin(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(value): Json<Value>,
) -> ApiResult {
    match s.storage.require_account(&id).await?.provider_id.as_str() {
        codex2api_common::CHATGPT => {
            chatgpt::relogin(State(s), Path(id), Json(decode(value)?)).await
        }
        codex2api_common::GROK => grok::relogin(State(s), Path(id), Json(decode(value)?)).await,
        codex2api_common::ANTIGRAVITY => {
            antigravity::relogin(State(s), Path(id), Json(decode(value)?)).await
        }
        codex2api_common::API => api::relogin(State(s), Path(id), Json(decode(value)?)).await,
        _ => Err(ApiError::bad("不支持的渠道")),
    }
}
#[derive(serde::Deserialize, serde::Serialize)]
pub struct OfficialQuery {
    section: String,
    #[serde(default)]
    refresh: bool,
}
pub async fn official(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Query(q): Query<OfficialQuery>,
) -> ApiResult {
    let value = serde_json::to_value(q).map_err(|_| ApiError::bad("请求无效"))?;
    match s.storage.require_account(&id).await?.provider_id.as_str() {
        codex2api_common::CHATGPT => {
            chatgpt::official(State(s), Path(id), Query(decode(value)?)).await
        }
        codex2api_common::GROK => grok::official(State(s), Path(id), Query(decode(value)?)).await,
        codex2api_common::ANTIGRAVITY => {
            antigravity::official(State(s), Path(id), Query(decode(value)?)).await
        }
        codex2api_common::API => Err(ApiError::bad("API 渠道不提供官方数据")),
        _ => Err(ApiError::bad("不支持的渠道")),
    }
}
pub async fn credit(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(value): Json<Value>,
) -> ApiResult {
    if s.storage.require_account(&id).await?.provider_id != codex2api_common::CHATGPT {
        return Err(ApiError::bad("该渠道不支持官方重置卡"));
    }
    chatgpt::credit(State(s), Path(id), Json(decode(value)?)).await
}
