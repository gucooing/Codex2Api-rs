use crate::{AdminState, rest::error::ApiError};
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use codex2api_storage::{AdminSession, Storage};
use sha2::{Digest, Sha256};
use std::time::Duration;
pub const SESSION_COOKIE: &str = "c2a_admin_session";
pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    for part in headers.get(header::COOKIE)?.to_str().ok()?.split(';') {
        if let Some((key, value)) = part.trim().split_once('=')
            && key == name
            && !value.is_empty()
        {
            return Some(value.into());
        }
    }
    None
}
pub fn session_id_from_headers(headers: &HeaderMap) -> Option<String> {
    cookie_value(headers, SESSION_COOKIE)
}
pub(crate) async fn load_session(
    storage: &Storage,
    headers: &HeaderMap,
) -> Result<Option<AdminSession>, ApiError> {
    let Some(id) = session_id_from_headers(headers) else {
        return Ok(None);
    };
    storage.get_admin_session(&id).await.map_err(|error| {
        tracing::error!(%error, "failed to read admin session");
        ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "session_unavailable".into(),
            "暂时无法验证登录会话，请稍后重试".into(),
        )
    })
}
pub fn csrf_token(id: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("codex2api-official-actions:{id}"))
    )
}
pub fn set_session_cookie(id: &str, ttl: Duration) -> String {
    format!(
        "{SESSION_COOKIE}={id}; HttpOnly; Path=/admin; SameSite=Lax; Max-Age={}",
        ttl.as_secs()
    )
}
pub fn clear_session_cookie() -> String {
    format!("{SESSION_COOKIE}=; HttpOnly; Path=/admin; SameSite=Lax; Max-Age=0")
}
pub(crate) async fn require_session(
    State(state): State<AdminState>,
    req: Request,
    next: Next,
) -> Response {
    let session = match load_session(&state.storage, req.headers()).await {
        Ok(Some(session)) => session,
        Ok(None) => return ApiError::unauthorized().into_response(),
        Err(error) => return error.into_response(),
    };
    if !matches!(
        *req.method(),
        axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
    ) {
        let expected = csrf_token(&session.id);
        if req
            .headers()
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
            != Some(expected.as_str())
            || req
                .headers()
                .get("sec-fetch-site")
                .is_some_and(|v| v == "cross-site")
        {
            return crate::rest::error::ApiError::forbidden("CSRF 校验失败，请重新加载页面")
                .into_response();
        }
    }
    next.run(req).await
}
