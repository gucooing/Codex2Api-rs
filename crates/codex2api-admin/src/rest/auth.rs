use super::dto;
use super::error::{ApiError, ApiResult, ok};
use crate::{AdminState, session};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Login {
    username: String,
    password: String,
}
pub async fn login(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(form): Json<Login>,
) -> Result<Response, ApiError> {
    if headers
        .get("sec-fetch-site")
        .is_some_and(|v| v == "cross-site")
    {
        return Err(ApiError::forbidden("不接受跨站登录请求"));
    }
    if form.username.len() > 128 || form.password.len() > 1024 {
        return Err(ApiError::bad("用户名或密码过长"));
    }
    let ttl = codex2api_storage::DEFAULT_ADMIN_SESSION_TTL;
    let value = state
        .storage
        .login_admin(form.username.trim(), &form.password, ttl)
        .await?
        .ok_or_else(|| {
            ApiError(
                axum::http::StatusCode::UNAUTHORIZED,
                "invalid_credentials".into(),
                "用户名或密码错误".into(),
            )
        })?;
    if let Some(old) = session::session_id_from_headers(&headers) {
        state.storage.delete_admin_session(&old).await?;
    }
    Ok((
        [(
            header::SET_COOKIE,
            session::set_session_cookie(&value.id, ttl),
        )],
        Json(dto::Session {
            authenticated: true,
            app_version: codex2api_version::APP_VERSION,
            codex_cli_version: codex2api_version::CODEX_PACKAGE_VERSION,
            username: Some(form.username.trim().into()),
            csrf_token: Some(session::csrf_token(&value.id)),
        }),
    )
        .into_response())
}
pub async fn current(State(state): State<AdminState>, headers: HeaderMap) -> ApiResult {
    let value = session::load_session(&state.storage, &headers)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    let user = state.storage.require_admin_user().await?;
    Ok(Json(dto::value(dto::Session {
        authenticated: true,
        app_version: codex2api_version::APP_VERSION,
        codex_cli_version: codex2api_version::CODEX_PACKAGE_VERSION,
        username: Some(user.username),
        csrf_token: Some(session::csrf_token(&value.id)),
    })))
}
pub async fn logout(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(id) = session::session_id_from_headers(&headers) {
        state.storage.delete_admin_session(&id).await?;
    }
    Ok((
        [(header::SET_COOKIE, session::clear_session_cookie())],
        ok(),
    )
        .into_response())
}
