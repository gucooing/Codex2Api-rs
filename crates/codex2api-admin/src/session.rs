use crate::{AdminState, rest::error::ApiError};
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use codex2api_storage::{AdminSession, Storage};
use sha2::{Digest, Sha256};
pub const SESSION_COOKIE: &str = "c2a_admin_session";
pub const REFRESH_COOKIE: &str = "c2a_admin_refresh";
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
pub(crate) async fn load_session(
    storage: &Storage,
    headers: &HeaderMap,
) -> Result<Option<AdminSession>, ApiError> {
    let token = if headers.contains_key(header::AUTHORIZATION) {
        if headers.get_all(header::AUTHORIZATION).iter().count() != 1 {
            return Err(ApiError::unauthorized());
        }
        Some(
            headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split_once(' '))
                .filter(|(scheme, token)| {
                    scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()
                })
                .map(|(_, token)| token.to_owned())
                .ok_or_else(ApiError::unauthorized)?,
        )
    } else {
        cookie_value(headers, SESSION_COOKIE)
    };
    let Some(token) = token else {
        return Ok(None);
    };
    storage
        .admin_session_from_jwt(&token)
        .await
        .map_err(|error| {
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
pub async fn session_cookies(
    state: &AdminState,
    tokens: Option<&codex2api_storage::WebSessionTokens>,
    csrf: &str,
) -> codex2api_storage::Result<axum::response::AppendHeaders<[(header::HeaderName, String); 3]>> {
    let secure = state
        .storage
        .public_url_settings()
        .await?
        .map(|settings| settings.admin_url.starts_with("https://"))
        .unwrap_or(state.secure_cookies);
    let secure = if secure { "; Secure" } else { "" };
    Ok(axum::response::AppendHeaders(
        [
            (
                SESSION_COOKIE,
                tokens.map_or("", |t| t.access_token.as_str()),
                "/admin",
                true,
                codex2api_storage::WEB_ACCESS_TTL_SECONDS,
            ),
            (
                REFRESH_COOKIE,
                tokens.map_or("", |t| t.refresh_token.as_str()),
                "/admin/api",
                true,
                codex2api_storage::WEB_REFRESH_TTL_SECONDS,
            ),
            (
                "c2a_admin_csrf",
                csrf,
                "/admin",
                false,
                codex2api_storage::WEB_REFRESH_TTL_SECONDS,
            ),
        ]
        .map(|(name, value, path, http_only, ttl)| {
            (
                header::SET_COOKIE,
                format!(
                    "{name}={value}; Path={path}; SameSite=Lax; Max-Age={}{}{secure}",
                    if tokens.is_some() { ttl } else { 0 },
                    if http_only { "; HttpOnly" } else { "" },
                ),
            )
        }),
    ))
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
