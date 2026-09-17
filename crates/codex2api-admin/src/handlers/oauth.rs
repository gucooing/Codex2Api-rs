use crate::state::InflightOauth;
use crate::{AdminState, session, views as html};
use axum::extract::{Form, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use codex2api_accounts::{AccountIdentity, HostRuntime, new_installation_id};
use codex2api_auth::{CompletedLogin, LoginFlow};
use serde::Deserialize;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginMethod {
    Callback,
    Device,
    RefreshToken,
}

#[derive(Deserialize)]
pub struct StartForm {
    csrf: String,
    method: LoginMethod,
    installation_id: String,
    os_type: String,
    os_version: String,
    arch: String,
    terminal: String,
    #[serde(default)]
    proxy_id: String,
    #[serde(default)]
    timezone: String,
    #[serde(default)]
    refresh_token: String,
}

#[derive(Deserialize)]
pub struct ReloginForm {
    csrf: String,
    method: LoginMethod,
}

#[derive(Default, Deserialize)]
pub struct PendingQuery {
    state: Option<String>,
}

#[derive(Deserialize)]
pub struct CallbackForm {
    csrf: String,
    state: String,
    callback_url: String,
}

#[derive(Deserialize)]
pub struct PollForm {
    csrf: String,
    state: String,
}

async fn csrf(state: &AdminState, headers: &HeaderMap) -> Result<String, Response> {
    session::load_session(&state.storage, headers)
        .await
        .map(|session| super::official::csrf_token(&session.id))
        .ok_or_else(|| Redirect::to("/admin/login").into_response())
}

fn error(status: StatusCode, message: &str) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        Html(html::error_page("授权失败", message)),
    )
        .into_response()
}

fn json_response(status: StatusCode, value: serde_json::Value) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(value),
    )
        .into_response()
}

fn json_error(status: StatusCode, message: &str) -> Response {
    json_response(status, serde_json::json!({"error": message}))
}

pub async fn oauth_start(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Form(form): Form<StartForm>,
) -> Response {
    let token = match csrf(&state, &headers).await {
        Ok(token) => token,
        Err(response) => return response,
    };
    if token != form.csrf {
        return json_error(StatusCode::FORBIDDEN, "请刷新页面后重试");
    }
    let installation_id =
        match codex2api_accounts::canonicalize_installation_id(&form.installation_id) {
            Ok(id) => id,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "Installation ID 无效"),
        };
    let fingerprint = super::fingerprint::FingerprintForm {
        csrf: form.csrf,
        os_type: form.os_type,
        os_version: form.os_version,
        arch: form.arch,
        terminal: form.terminal,
        proxy_id: form.proxy_id,
        timezone: form.timezone,
    };
    let identity =
        match fingerprint.build_identity(uuid::Uuid::new_v4().to_string(), installation_id) {
            Ok(identity) => identity,
            Err(message) => return json_error(StatusCode::BAD_REQUEST, &message),
        };
    match form.method {
        LoginMethod::Callback | LoginMethod::Device => match state
            .auth
            .begin_manual_login_with_identity(
                identity,
                matches!(form.method, LoginMethod::Device),
                Some(&fingerprint.proxy_id),
            )
            .await
        {
            Ok(pending) => {
                let flow = match state.auth.login_flow(&pending) {
                    Ok(flow) => flow,
                    Err(err) => return json_error(StatusCode::BAD_REQUEST, &err.to_string()),
                };
                let value = match flow {
                    LoginFlow::Callback { authorize_url } => serde_json::json!({
                        "status":"pending", "method":"callback", "state":pending.state,
                        "authorize_url":authorize_url,
                    }),
                    LoginFlow::Device {
                        verification_url,
                        user_code,
                        interval,
                        ..
                    } => serde_json::json!({
                        "status":"pending", "method":"device", "state":pending.state,
                        "verification_url":verification_url, "user_code":user_code, "interval":interval,
                    }),
                };
                json_response(StatusCode::OK, value)
            }
            Err(err) => json_error(StatusCode::BAD_REQUEST, &err.to_string()),
        },
        LoginMethod::RefreshToken => match state
            .auth
            .login_with_refresh_token(identity, Some(&fingerprint.proxy_id), &form.refresh_token)
            .await
        {
            Ok(done) => json_response(
                StatusCode::OK,
                serde_json::json!({
                    "status":"complete", "redirect":completed_account(&state, &done).await,
                }),
            ),
            Err(err) => json_error(StatusCode::BAD_REQUEST, &err.to_string()),
        },
    }
}

pub async fn oauth_setup(State(state): State<AdminState>, headers: HeaderMap) -> Response {
    let token = match csrf(&state, &headers).await {
        Ok(token) => token,
        Err(response) => return response,
    };
    let proxies = match state.storage.list_outbound_proxies().await {
        Ok(proxies) => proxies,
        Err(err) => return error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()),
    };
    let identity = AccountIdentity::new(
        uuid::Uuid::new_v4().to_string(),
        new_installation_id(),
        HostRuntime::generate(),
    );
    let form = super::fingerprint::FingerprintForm::from_identity(&identity, &token);
    (
        [(header::CACHE_CONTROL, "no-store")],
        Html(html::oauth::setup(&identity, &form, &proxies)),
    )
        .into_response()
}

pub async fn oauth_relogin(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<ReloginForm>,
) -> Response {
    let token = match csrf(&state, &headers).await {
        Ok(token) => token,
        Err(response) => return response,
    };
    if token != form.csrf {
        return error(StatusCode::FORBIDDEN, "请刷新页面后重试");
    }
    if matches!(form.method, LoginMethod::RefreshToken) {
        return error(StatusCode::BAD_REQUEST, "重新登录不支持此授权方式");
    }
    match state
        .auth
        .begin_manual_login_with_proxy(Some(&id), matches!(form.method, LoginMethod::Device), None)
        .await
    {
        Ok(pending) => {
            state.store_last_oauth(InflightOauth {
                account_id: pending.account_id.unwrap_or_default(),
                state: pending.state.clone(),
            });
            Redirect::to(&format!("/admin/oauth?state={}", pending.state)).into_response()
        }
        Err(err) => error(StatusCode::BAD_REQUEST, &err.to_string()),
    }
}

pub async fn oauth_pending_page(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Query(query): Query<PendingQuery>,
) -> Response {
    let token = match csrf(&state, &headers).await {
        Ok(token) => token,
        Err(response) => return response,
    };
    let Some(state_id) = query
        .state
        .or_else(|| state.last_oauth().map(|info| info.state))
    else {
        return Redirect::to("/admin").into_response();
    };
    let pending = match state.auth.pending_login(&state_id).await {
        Ok(Some(pending)) => pending,
        Ok(None) => {
            return error(
                StatusCode::BAD_REQUEST,
                "授权已过期或已完成，请重新发起授权",
            );
        }
        Err(err) => return error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()),
    };
    let flow = match state.auth.login_flow(&pending) {
        Ok(flow) => flow,
        Err(err) => return error(StatusCode::BAD_REQUEST, &err.to_string()),
    };
    (
        [(header::CACHE_CONTROL, "no-store")],
        Html(html::oauth::pending(&pending, &flow, &token, None)),
    )
        .into_response()
}

async fn completed(state: &AdminState, state_id: &str, done: &CompletedLogin) -> String {
    state.clear_oauth_if(state_id);
    completed_account(state, done).await
}

async fn completed_account(state: &AdminState, done: &CompletedLogin) -> String {
    state.quota_cache.invalidate(&done.account.id).await;
    state.upstream.evict(&done.account.id).await;
    format!(
        "/admin/accounts/{}?ok={}",
        done.account.id,
        crate::response::encode_query("授权完成")
    )
}

pub async fn submit_callback(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Form(form): Form<CallbackForm>,
) -> Response {
    let json = headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("application/json"));
    let token = match csrf(&state, &headers).await {
        Ok(token) => token,
        Err(response) => return response,
    };
    if token != form.csrf {
        if json {
            return json_error(StatusCode::FORBIDDEN, "请刷新页面后重试");
        }
        return error(StatusCode::FORBIDDEN, "请刷新页面后重试");
    }
    match state
        .auth
        .complete_manual_callback(&form.state, &form.callback_url)
        .await
    {
        Ok(done) => {
            let redirect = completed(&state, &form.state, &done).await;
            if json {
                json_response(
                    StatusCode::OK,
                    serde_json::json!({"status":"complete", "redirect":redirect}),
                )
            } else {
                Redirect::to(&redirect).into_response()
            }
        }
        Err(err) => {
            let message = err.to_string();
            if json {
                return json_error(StatusCode::BAD_REQUEST, &message);
            }
            if let Ok(Some(pending)) = state.auth.pending_login(&form.state).await
                && let Ok(flow) = state.auth.login_flow(&pending)
            {
                return (
                    StatusCode::BAD_REQUEST,
                    [(header::CACHE_CONTROL, "no-store")],
                    Html(html::oauth::pending(
                        &pending,
                        &flow,
                        &token,
                        Some(&message),
                    )),
                )
                    .into_response();
            }
            error(StatusCode::BAD_REQUEST, &message)
        }
    }
}

pub async fn cancel_draft(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Form(form): Form<PollForm>,
) -> Response {
    let token = match csrf(&state, &headers).await {
        Ok(token) => token,
        Err(response) => return response,
    };
    if token != form.csrf {
        return json_error(StatusCode::FORBIDDEN, "请刷新页面后重试");
    }
    state.auth.cancel_draft_login(&form.state).await;
    json_response(StatusCode::OK, serde_json::json!({"status":"cancelled"}))
}

pub async fn poll_device(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Form(form): Form<PollForm>,
) -> Response {
    let token = match csrf(&state, &headers).await {
        Ok(token) => token,
        Err(response) => return response,
    };
    if token != form.csrf {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({"error": "请刷新页面后重试"})),
        )
            .into_response();
    }
    let result = state.auth.poll_device_login(&form.state).await;
    let (status, value) = match result {
        Ok(Some(done)) => (
            StatusCode::OK,
            serde_json::json!({"status":"complete","redirect":completed(&state, &form.state, &done).await}),
        ),
        Ok(None) => (StatusCode::OK, serde_json::json!({"status":"pending"})),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            serde_json::json!({"error":err.to_string()}),
        ),
    };
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(value),
    )
        .into_response()
}
