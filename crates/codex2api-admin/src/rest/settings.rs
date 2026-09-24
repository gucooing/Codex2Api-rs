use super::error::{ApiError, ApiResult, ok};
use crate::{AdminState, session};
use axum::{
    Json,
    extract::{Query, State},
    http::header,
    response::{IntoResponse, Response},
};
use codex2api_storage::{DesktopSupportSettings, GatewaySettings};
use serde::Deserialize;
use serde_json::json;
pub async fn overview(State(s): State<AdminState>) -> ApiResult {
    let accounts = s.storage.virtual_accounts().await?;
    Ok(Json(super::dto::value(super::dto::Overview {
        supplier_count: s.storage.list_accounts().await?.len(),
        consumer_count: accounts.len(),
        enabled_consumers: accounts.iter().filter(|a| a.enabled).count(),
        models_count: s.storage.model_configs("chatgpt").await?.len(),
    })))
}
pub async fn gateway(State(s): State<AdminState>) -> ApiResult {
    Ok(Json(json!(s.storage.gateway_settings().await?)))
}
pub async fn save_gateway(
    State(s): State<AdminState>,
    Json(f): Json<GatewaySettings>,
) -> ApiResult {
    if f.ua_rules.len() > 1000 || f.ua_rules.iter().any(|v| v.len() > 1024) {
        return Err(ApiError::bad("User-Agent 规则过长"));
    }
    s.storage.save_gateway_settings(&f).await?;
    Ok(Json(json!(f)))
}
pub async fn security(State(s): State<AdminState>) -> ApiResult {
    Ok(Json(
        json!({"username":s.storage.require_admin_user().await?.username}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Security {
    old_username: String,
    old_password: String,
    new_username: String,
    #[serde(default)]
    new_password: String,
}
pub async fn save_security(
    State(s): State<AdminState>,
    Json(f): Json<Security>,
) -> Result<Response, ApiError> {
    s.storage
        .change_admin_credentials(
            &f.old_username,
            &f.old_password,
            &f.new_username,
            &f.new_password,
        )
        .await?;
    Ok((
        [(header::SET_COOKIE, session::clear_session_cookie())],
        ok(),
    )
        .into_response())
}
pub async fn desktop(State(s): State<AdminState>) -> ApiResult {
    Ok(Json(json!(s.storage.desktop_support_settings().await?)))
}
pub async fn save_desktop(
    State(s): State<AdminState>,
    Json(f): Json<DesktopSupportSettings>,
) -> ApiResult {
    if !(1..=1440).contains(&f.resource_cache_minutes) {
        return Err(ApiError::bad("缓存时间须在 1 到 1440 分钟之间"));
    }
    if let Some(id) = &f.proxy_id {
        s.storage.require_outbound_proxy(id).await?;
    }
    s.storage.save_desktop_support_settings(&f).await?;
    Ok(Json(json!(f)))
}
#[derive(Deserialize)]
pub struct PageQuery {
    page: Option<u32>,
    page_size: Option<u32>,
}
pub async fn diagnostics(State(s): State<AdminState>, Query(q): Query<PageQuery>) -> ApiResult {
    codex2api_storage::table_page_size(q.page_size).map_err(|e| ApiError::bad(e.to_string()))?;
    Ok(Json(
        s.storage
            .desktop_diagnostic_page(q.page.unwrap_or(1), q.page_size)
            .await?,
    ))
}
pub async fn resources(State(s): State<AdminState>) -> ApiResult {
    Ok(Json(json!({"items":s.storage.desktop_resources().await?})))
}
pub async fn missing(State(s): State<AdminState>) -> ApiResult {
    Ok(Json(json!({"items":s.storage.missing_endpoints().await?})))
}
