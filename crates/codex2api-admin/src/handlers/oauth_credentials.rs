use crate::{AdminState, response, session, views};
use axum::{
    extract::{Form, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use serde::Deserialize;

#[derive(Default, Deserialize)]
pub struct PageQuery {
    ok: Option<String>,
    err: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateForm {
    csrf: String,
    name: String,
    account_id: String,
}

#[derive(Deserialize)]
pub struct ActionForm {
    csrf: String,
}

fn redirect(kind: &str, message: &str) -> Response {
    Redirect::to(&format!(
        "/admin/oauth/credentials?{kind}={}",
        response::encode_query(message)
    ))
    .into_response()
}

fn detail_redirect(account_id: &str, kind: &str, message: &str) -> Response {
    Redirect::to(&format!(
        "/admin/oauth/accounts/{}?{kind}={}",
        response::encode_query(account_id),
        response::encode_query(message)
    ))
    .into_response()
}

async fn validate_csrf(
    state: &AdminState,
    headers: &HeaderMap,
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
    Ok(())
}

pub async fn page(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Query(query): Query<PageQuery>,
) -> Response {
    let Some(session) = session::load_session(&state.storage, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    let (summaries, accounts) = tokio::join!(
        state.storage.oauth_account_summaries(),
        state.storage.list_accounts()
    );
    match (summaries, accounts) {
        (Ok(summaries), Ok(accounts)) => (
            [(header::CACHE_CONTROL, "no-store")],
            Html(views::oauth_credentials::render(
                &accounts,
                &summaries,
                &super::official::csrf_token(&session.id),
                query.ok.as_deref(),
                query.err.as_deref(),
            )),
        )
            .into_response(),
        (Err(e), _) | (_, Err(e)) => response::storage_error("无法加载 OAuth", e),
    }
}

pub async fn detail(
    State(state): State<AdminState>,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<PageQuery>,
) -> Response {
    let Some(session) = session::load_session(&state.storage, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    let account = match state.storage.get_account(&account_id).await {
        Ok(Some(account)) => account,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => return response::storage_error("无法加载账户", e),
    };
    let (credentials, devices) = tokio::join!(
        state.storage.oauth_credentials_for_account(&account_id),
        state.storage.oauth_devices_for_account(&account_id)
    );
    match (credentials, devices) {
        (Ok(credentials), Ok(devices)) => (
            [(header::CACHE_CONTROL, "no-store")],
            Html(views::oauth_credentials::detail(
                &account,
                &credentials,
                &devices,
                &super::official::csrf_token(&session.id),
                query.ok.as_deref(),
                query.err.as_deref(),
            )),
        )
            .into_response(),
        (Err(e), _) | (_, Err(e)) => response::storage_error("无法加载 OAuth 详情", e),
    }
}

pub async fn create(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Form(form): Form<CreateForm>,
) -> Response {
    if let Err(e) = validate_csrf(&state, &headers, &form.csrf).await {
        return e;
    }
    let name = form.name.trim();
    if name.is_empty() || name.chars().count() > 128 {
        return redirect("err", "名称不能为空，且不能超过 128 个字符");
    }
    match state
        .storage
        .create_oauth_credential(&form.account_id, name)
        .await
    {
        Ok(credential) => detail_redirect(
            &credential.account_id,
            "ok",
            "RT 已添加，请点击复制用于第三方客户端登录",
        ),
        Err(codex2api_storage::StorageError::OAuthCredentialUnavailable) => {
            redirect("err", "请选择已启用且完成授权的账户")
        }
        Err(e) => response::storage_error("无法添加 RT", e),
    }
}

pub async fn delete_account(
    State(state): State<AdminState>,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<ActionForm>,
) -> Response {
    if let Err(error) = validate_csrf(&state, &headers, &form.csrf).await {
        return error;
    }
    match state
        .storage
        .delete_account_oauth_credentials(&account_id)
        .await
    {
        Ok(_) => redirect("ok", "该账户的 OAuth RT、令牌和设备记录已删除"),
        Err(error) => response::storage_error("无法删除 OAuth 配置", error),
    }
}

pub async fn action(
    State(state): State<AdminState>,
    Path((id, action)): Path<(String, String)>,
    headers: HeaderMap,
    Form(form): Form<ActionForm>,
) -> Response {
    if let Err(e) = validate_csrf(&state, &headers, &form.csrf).await {
        return e;
    }
    if action == "copy" {
        return match state.storage.oauth_refresh_token(&id).await {
            Ok(Some(token)) => (
                [(header::CACHE_CONTROL, "no-store")],
                axum::Json(serde_json::json!({"token":token})),
            )
                .into_response(),
            Ok(None) => (
                StatusCode::NOT_FOUND,
                [(header::CACHE_CONTROL, "no-store")],
                axum::Json(serde_json::json!({"error":"未找到该 RT"})),
            )
                .into_response(),
            Err(e) => response::storage_error("无法复制 RT", e),
        };
    }
    let credential = match state.storage.get_oauth_credential(&id).await {
        Ok(Some(credential)) => credential,
        Ok(None) => return redirect("err", "未找到该 RT，请刷新后重试"),
        Err(e) => return response::storage_error("无法加载 RT", e),
    };
    let (result, message) = match action.as_str() {
        "pause" => (
            state.storage.set_oauth_paused(&id, true).await,
            "RT 已暂停，已签发的 Access Token 已失效",
        ),
        "enable" => (
            state.storage.set_oauth_paused(&id, false).await,
            "RT 已启用，请在客户端重新刷新令牌",
        ),
        "delete" => (
            state.storage.delete_oauth_credential(&id).await,
            "RT 已删除",
        ),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    match result {
        Ok(true) => detail_redirect(&credential.account_id, "ok", message),
        Ok(false) => redirect("err", "未找到该 RT，请刷新后重试"),
        Err(e) => response::storage_error("无法更新 RT", e),
    }
}
