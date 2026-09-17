use crate::response::redirect_with_cookie;
use crate::views as html;
use crate::{AdminState, session};
use axum::extract::{Form, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use codex2api_storage::DEFAULT_ADMIN_SESSION_TTL;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

#[derive(Default, Deserialize)]
pub struct LoginQuery {
    #[serde(default)]
    updated: bool,
}

pub async fn login_page(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Query(query): Query<LoginQuery>,
) -> Response {
    if session::load_session(&state.storage, &headers)
        .await
        .is_some()
    {
        return Redirect::to("/admin").into_response();
    }
    Html(html::login(
        None,
        query
            .updated
            .then_some("用户名和密码设置已更新，请重新登录"),
    ))
    .into_response()
}

pub async fn login_post(State(state): State<AdminState>, Form(form): Form<LoginForm>) -> Response {
    let username = form.username.trim();
    let password = form.password;
    match state
        .storage
        .login_admin(username, &password, DEFAULT_ADMIN_SESSION_TTL)
        .await
    {
        Ok(Some(session)) => {
            tracing::info!(username, "admin logged in");
            redirect_with_cookie(
                "/admin",
                session::set_session_cookie(&session.id, DEFAULT_ADMIN_SESSION_TTL),
            )
        }
        Ok(None) => Html(html::login(Some("用户名或密码错误"), None)).into_response(),
        Err(err) => {
            tracing::error!(%err, "admin login failed");
            Html(html::login(Some("登录失败，请稍后重试"), None)).into_response()
        }
    }
}

pub async fn logout(State(state): State<AdminState>, headers: HeaderMap) -> Response {
    if let Some(id) = session::session_id_from_headers(&headers) {
        let _ = state.storage.delete_admin_session(&id).await;
    }
    redirect_with_cookie("/admin/login", session::clear_session_cookie())
}
