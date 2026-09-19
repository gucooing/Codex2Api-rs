use crate::{
    AdminState,
    response::{not_found_account, storage_error},
    session, views,
};
use axum::{
    extract::{Form, Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use codex2api_storage::{Account, TurnStateSettings};
use serde::Deserialize;

pub(crate) async fn content(
    state: &AdminState,
    account: &Account,
    csrf: &str,
) -> codex2api_storage::Result<String> {
    let (settings, _) = state.storage.turn_state_settings(&account.id).await?;
    let entries = state.storage.turn_state_entries(&account.id).await?;
    Ok(views::turn_state::render(
        account,
        csrf,
        &settings,
        &entries,
        chrono::Utc::now().timestamp(),
    ))
}

#[derive(Deserialize)]
pub struct SettingsForm {
    csrf: String,
    enabled: Option<String>,
    models: String,
    ttl: i64,
}
#[derive(Deserialize)]
pub struct ClearForm {
    csrf: String,
}

async fn authorize(
    state: &AdminState,
    headers: &HeaderMap,
    id: &str,
    csrf: &str,
) -> Result<(), Response> {
    let Some(session) = session::load_session(&state.storage, headers).await else {
        return Err(Redirect::to("/admin/login").into_response());
    };
    if csrf != super::official::csrf_token(&session.id) {
        return Err((
            StatusCode::FORBIDDEN,
            Html(views::error_page("请求无效", "请刷新页面后重试")),
        )
            .into_response());
    }
    match state.storage.get_account(id).await {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(not_found_account(id)),
        Err(error) => Err(storage_error("无法加载账户", error)),
    }
}

pub async fn save(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<SettingsForm>,
) -> Response {
    if let Err(response) = authorize(&state, &headers, &id, &form.csrf).await {
        return response;
    }
    let mut models: Vec<String> = form.models.split_whitespace().map(str::to_owned).collect();
    models.sort();
    models.dedup();
    let settings = TurnStateSettings {
        enabled: form.enabled.as_deref() == Some("on"),
        models,
        ttl: form.ttl,
    };
    if let Err(error) = settings.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Html(views::error_page("配置无效", &error.to_string())),
        )
            .into_response();
    }
    match state.storage.save_turn_state_settings(&id, &settings).await {
        Ok(()) => Redirect::to(&format!(
            "/admin/accounts/{id}?tab=turn-state&ok={}",
            crate::response::encode_query("已保存配置，旧缓存已清除")
        ))
        .into_response(),
        Err(error) => storage_error("无法保存状态复用配置", error),
    }
}

pub async fn clear(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<ClearForm>,
) -> Response {
    if let Err(response) = authorize(&state, &headers, &id, &form.csrf).await {
        return response;
    }
    match state.storage.clear_turn_state(&id).await {
        Ok(()) => Redirect::to(&format!(
            "/admin/accounts/{id}?tab=turn-state&ok={}",
            crate::response::encode_query("缓存已清除，下次匹配请求重新采集")
        ))
        .into_response(),
        Err(error) => storage_error("无法清除状态缓存", error),
    }
}
