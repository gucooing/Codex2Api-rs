use crate::{
    AdminState,
    rest::error::{ApiError, ApiResult, ok},
    session,
};
use axum::{
    Json, Router,
    extract::{Query, State},
    response::{IntoResponse, Response},
    routing::get,
};
use codex2api_storage::{DesktopSupportSettings, GatewaySettings, PublicUrlSettings};
use serde::Deserialize;
use serde_json::json;

pub(super) fn router() -> Router<AdminState> {
    Router::new()
        .route("/settings/gateway", get(gateway).put(save_gateway))
        .route(
            "/settings/public-urls",
            get(public_urls).put(save_public_urls),
        )
        .route("/settings/security", get(security).put(save_security))
        .route("/settings/desktop", get(desktop).put(save_desktop))
        .route("/resources", get(resources))
        .route("/missing-endpoints", get(missing))
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

pub async fn gateway(State(s): State<AdminState>) -> ApiResult {
    Ok(Json(json!(s.storage.gateway_settings().await?)))
}

pub async fn save_gateway(
    State(s): State<AdminState>,
    Json(f): Json<GatewaySettings>,
) -> ApiResult {
    if f.default_rpm > 1_000_000 {
        return Err(ApiError::bad("默认 RPM 须为 0 到 1000000 的整数"));
    }
    if f.ua_rules.len() > 1000 || f.ua_rules.iter().any(|v| v.len() > 1024) {
        return Err(ApiError::bad("User-Agent 规则过长"));
    }
    s.storage.save_gateway_settings(&f).await?;
    Ok(Json(json!(f)))
}

pub async fn missing(
    State(s): State<AdminState>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    Ok(Json(json!(s.storage.missing_endpoint_page(&q).await?)))
}

pub async fn public_urls(State(s): State<AdminState>) -> ApiResult {
    Ok(Json(json!(
        s.storage
            .public_url_settings()
            .await?
            .unwrap_or(s.public_url_defaults)
    )))
}

pub async fn save_public_urls(
    State(s): State<AdminState>,
    Json(settings): Json<PublicUrlSettings>,
) -> ApiResult {
    let saved = s
        .storage
        .save_public_url_settings(&settings)
        .await?
        .ok_or_else(ApiError::conflict)?;
    Ok(Json(json!(saved)))
}

pub async fn resources(
    State(s): State<AdminState>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    Ok(Json(json!(s.storage.desktop_resource_page(&q).await?)))
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
    Ok((session::session_cookies(&s, None, "").await?, ok()).into_response())
}
