use crate::{AdminState, response, session, views};
use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use codex2api_storage::StorageError;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SettingsForm {
    csrf: String,
    old_username: String,
    old_password: String,
    new_username: String,
    #[serde(default)]
    new_password: String,
}

fn render(
    status: StatusCode,
    csrf: &str,
    old_username: &str,
    new_username: &str,
    error: Option<&str>,
) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        Html(views::settings::render(
            csrf,
            old_username,
            new_username,
            error,
        )),
    )
        .into_response()
}

pub async fn page(State(state): State<AdminState>, headers: HeaderMap) -> Response {
    let Some(session) = session::load_session(&state.storage, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    match state.storage.require_admin_user().await {
        Ok(user) => render(
            StatusCode::OK,
            &super::official::csrf_token(&session.id),
            "",
            &user.username,
            None,
        ),
        Err(err) => response::storage_error("无法加载设置", err),
    }
}

pub async fn save(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Form(form): Form<SettingsForm>,
) -> Response {
    let Some(session) = session::load_session(&state.storage, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    let csrf = super::official::csrf_token(&session.id);
    if form.csrf != csrf {
        return render(
            StatusCode::FORBIDDEN,
            &csrf,
            &form.old_username,
            &form.new_username,
            Some("请刷新页面后重试"),
        );
    }
    match state
        .storage
        .change_admin_credentials(
            &form.old_username,
            &form.old_password,
            &form.new_username,
            &form.new_password,
        )
        .await
    {
        Ok(()) => response::redirect_with_cookie(
            "/admin/login?updated=true",
            session::clear_session_cookie(),
        ),
        Err(StorageError::InvalidCredentials) => render(
            StatusCode::BAD_REQUEST,
            &csrf,
            &form.old_username,
            &form.new_username,
            Some("原用户名或原密码错误"),
        ),
        Err(StorageError::InvalidAdminUpdate(message)) => render(
            StatusCode::BAD_REQUEST,
            &csrf,
            &form.old_username,
            &form.new_username,
            Some(message),
        ),
        Err(err) => {
            tracing::error!(%err, "failed to update admin credentials");
            render(
                StatusCode::INTERNAL_SERVER_ERROR,
                &csrf,
                &form.old_username,
                &form.new_username,
                Some("保存失败，请稍后重试"),
            )
        }
    }
}
