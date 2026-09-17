use crate::{AdminState, response, session, views};
use axum::extract::{Form, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use codex2api_storage::AccountStatus;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateForm {
    csrf: String,
    name: String,
    account_id: String,
}

#[derive(Deserialize)]
pub struct ActionForm {
    csrf: String,
    account_id: Option<String>,
}

#[derive(Default, Deserialize)]
pub struct PageQuery {
    ok: Option<String>,
    err: Option<String>,
}

pub async fn page(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Query(query): Query<PageQuery>,
) -> Response {
    let Some(session) = session::load_session(&state.storage, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    let (keys, accounts) = tokio::join!(
        state.storage.list_all_proxy_api_keys(),
        state.storage.list_accounts()
    );
    match (keys, accounts) {
        (Ok(keys), Ok(accounts)) => (
            [(header::CACHE_CONTROL, "no-store")],
            Html(views::keys::render(
                &accounts,
                &keys,
                &super::official::csrf_token(&session.id),
                query.ok.as_deref(),
                query.err.as_deref(),
            )),
        )
            .into_response(),
        (Err(error), _) | (_, Err(error)) => response::storage_error("无法加载 API Key", error),
    }
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

fn redirect(kind: &str, message: &str) -> Response {
    Redirect::to(&format!(
        "/admin/keys?{kind}={}",
        response::encode_query(message)
    ))
    .into_response()
}

pub async fn create(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Form(form): Form<CreateForm>,
) -> Response {
    if let Err(error) = validate_csrf(&state, &headers, &form.csrf).await {
        return error;
    }
    let account = match state.storage.get_account(&form.account_id).await {
        Ok(Some(account)) => account,
        Ok(None) => return redirect("err", "请选择有效的绑定账户"),
        Err(error) => return response::storage_error("无法加载账户", error),
    };
    if account.status == AccountStatus::Pending {
        return redirect("err", "待授权账户完成 OAuth 后才能绑定 API Key");
    }
    let name = form.name.trim();
    if name.is_empty() || name.chars().count() > 128 {
        return redirect("err", "名称不能为空，且不能超过 128 个字符");
    }
    match state
        .storage
        .create_proxy_api_key(&account.id, Some(name))
        .await
    {
        Ok(_) => redirect("ok", "API Key 已添加"),
        Err(error) => response::storage_error("无法添加 API Key", error),
    }
}

pub async fn action(
    State(state): State<AdminState>,
    Path((key_id, action)): Path<(String, String)>,
    headers: HeaderMap,
    Form(form): Form<ActionForm>,
) -> Response {
    if let Err(error) = validate_csrf(&state, &headers, &form.csrf).await {
        return error;
    }
    let key = match state.storage.get_proxy_api_key(&key_id).await {
        Ok(Some(key)) => key,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                [(header::CACHE_CONTROL, "no-store")],
                axum::Json(serde_json::json!({"error":"未找到该 API Key"})),
            )
                .into_response();
        }
        Err(error) => return response::storage_error("无法加载 API Key", error),
    };
    let account_id = key.account_id;
    if action == "copy" {
        let (status, value) = match state
            .storage
            .proxy_api_key_token(&account_id, &key_id)
            .await
        {
            Ok(Some(token)) => (StatusCode::OK, serde_json::json!({"token": token})),
            Ok(None) => (
                StatusCode::NOT_FOUND,
                serde_json::json!({"error": "该 Key 不存在或未保存明文，无法复制"}),
            ),
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": error.to_string()}),
            ),
        };
        return (
            status,
            [(header::CACHE_CONTROL, "no-store")],
            axum::Json(value),
        )
            .into_response();
    }
    let (result, message) = match action.as_str() {
        "bind" => {
            let Some(target) = form.account_id.as_deref().filter(|value| !value.is_empty()) else {
                return redirect("err", "请选择绑定账户");
            };
            match state.storage.get_account(target).await {
                Ok(Some(account)) if account.status != AccountStatus::Pending => {}
                Ok(_) => return redirect("err", "请选择已完成授权的账户"),
                Err(error) => return response::storage_error("无法加载绑定账户", error),
            }
            (
                state.storage.bind_proxy_api_key(&key_id, target).await,
                "绑定账户已更新",
            )
        }
        "enable" => (
            state
                .storage
                .set_proxy_api_key_paused(&account_id, &key_id, false)
                .await,
            "API Key 已启用",
        ),
        "pause" => (
            state
                .storage
                .set_proxy_api_key_paused(&account_id, &key_id, true)
                .await,
            "API Key 已暂停",
        ),
        "delete" => (
            state
                .storage
                .delete_proxy_api_key(&account_id, &key_id)
                .await,
            "API Key 已删除",
        ),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    match result {
        Ok(true) => redirect("ok", message),
        Ok(false) => redirect("err", "API Key 或绑定账户已变更，请刷新后重试"),
        Err(error) => response::storage_error("无法更新 API Key", error),
    }
}
