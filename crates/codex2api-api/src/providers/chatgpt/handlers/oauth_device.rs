//! Official CLI device-code flow backed by virtual accounts and normal device sessions.
use super::oauth::PREFIX;
use crate::ApiState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use base64::Engine;
use codex2api_storage::{DeviceAuthorization, DeviceAuthorizationPoll, oauth_secret};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

fn reply(status: StatusCode, value: serde_json::Value) -> Response {
    (
        status,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        Json(value),
    )
        .into_response()
}
fn failure(status: StatusCode, message: &str) -> Response {
    reply(status, json!({"error":{"message":message}}))
}

#[derive(Deserialize)]
pub struct UserCodeRequest {
    client_id: String,
}

pub async fn user_code(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<UserCodeRequest>,
) -> Response {
    if request.client_id != codex2api_version::OAUTH_CLIENT_ID {
        return failure(StatusCode::BAD_REQUEST, "Unknown client_id.");
    }
    let origin = state.public_base_url.clone().or_else(|| {
        let host = headers.get(header::HOST)?.to_str().ok()?;
        let url = url::Url::parse(&format!("http://{host}")).ok()?;
        (url.username().is_empty() && url.password().is_none() && url.path() == "/")
            .then(|| url.origin().ascii_serialization())
    });
    let Some(origin) = origin else {
        return failure(StatusCode::BAD_REQUEST, "Invalid service origin.");
    };
    let id = oauth_secret();
    let raw = oauth_secret()[..12].to_ascii_uppercase();
    let code = format!("{}-{}-{}", &raw[..4], &raw[4..8], &raw[8..]);
    let verifier = oauth_secret();
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    match state
        .storage
        .create_device_authorization(DeviceAuthorization {
            id: &id,
            user_code: &raw,
            client_id: &request.client_id,
            redirect_uri: &format!("{origin}{PREFIX}/deviceauth/callback"),
            verifier: &verifier,
            challenge: &challenge,
        })
        .await
    {
        Ok(true) => reply(
            StatusCode::OK,
            json!({"device_auth_id":id,"user_code":code,"interval":"5"}),
        ),
        Ok(false) => failure(StatusCode::TOO_MANY_REQUESTS, "Please retry later."),
        Err(error) => {
            tracing::error!(%error,"device authorization creation failed");
            failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to start device authorization.",
            )
        }
    }
}

#[derive(Deserialize)]
pub struct PollRequest {
    device_auth_id: String,
    user_code: String,
}

fn normalize_code(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase()
}

pub async fn poll(State(state): State<ApiState>, Json(request): Json<PollRequest>) -> Response {
    if request.device_auth_id.len() != 64 || request.user_code.len() > 32 {
        return failure(StatusCode::BAD_REQUEST, "Invalid device authorization.");
    }
    match state
        .storage
        .poll_device_authorization(&request.device_auth_id, &normalize_code(&request.user_code))
        .await
    {
        Ok(DeviceAuthorizationPoll::Pending) => {
            failure(StatusCode::FORBIDDEN, "Authorization pending.")
        }
        Ok(DeviceAuthorizationPoll::Expired) => {
            failure(StatusCode::GONE, "Device authorization expired.")
        }
        Ok(DeviceAuthorizationPoll::Authorized(code)) => reply(StatusCode::OK, json!(code)),
        Err(error) => {
            tracing::error!(%error,"device authorization polling failed");
            failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to read device authorization.",
            )
        }
    }
}

pub async fn page() -> Redirect {
    Redirect::to("/admin/device/")
}

pub async fn bootstrap(State(state): State<ApiState>) -> Response {
    let flow = uuid::Uuid::new_v4().simple().to_string();
    let browser = oauth_secret();
    let csrf = oauth_secret();
    if let Err(error) = state
        .storage
        .create_oauth_browser_flow(&flow, &browser, &csrf, "device")
        .await
    {
        tracing::error!(%error,"device authorization browser flow failed");
        return failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            "暂时无法登录，请稍后重试。",
        );
    }
    let mut response = reply(StatusCode::OK, json!({"request_id":flow,"csrf_token":csrf}));
    response.headers_mut().insert(header::SET_COOKIE,format!("c2a_device_{flow}={browser}; HttpOnly; SameSite=Strict; Path={PREFIX}/oauth/device; Max-Age=600").parse().unwrap());
    response
}

#[derive(Deserialize)]
pub struct ApproveRequest {
    request_id: String,
    csrf_token: String,
    user_code: String,
    username: String,
    password: String,
}

pub async fn approve(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<ApproveRequest>,
) -> Response {
    if request.request_id.len() != 32
        || !request.request_id.bytes().all(|b| b.is_ascii_hexdigit())
        || request.csrf_token.len() != 64
        || request.user_code.len() > 32
        || request.username.len() > 128
        || request.password.len() > 1024
    {
        return failure(StatusCode::BAD_REQUEST, "授权请求无效，请重新加载。");
    }
    let cookie_name = format!("c2a_device_{}", request.request_id);
    let browser = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.split(';').find_map(|p| {
                let (k, v) = p.trim().split_once('=')?;
                (k == cookie_name).then_some(v)
            })
        });
    let Some(browser) = browser else {
        return failure(StatusCode::BAD_REQUEST, "授权请求无效，请重新加载。");
    };
    if !matches!(state.storage.oauth_browser_flow(&request.request_id,browser,&request.csrf_token).await,Ok(Some(value)) if value=="device")
    {
        return failure(StatusCode::BAD_REQUEST, "授权请求已过期，请重新加载。");
    }
    if !matches!(
        state
            .storage
            .allow_virtual_login_attempt(request.username.trim())
            .await,
        Ok(true)
    ) {
        return failure(StatusCode::TOO_MANY_REQUESTS, "登录尝试过多，请稍后重试。");
    }
    let account = match state
        .storage
        .virtual_account_by_username(request.username.trim())
        .await
    {
        Ok(Some(account)) if account.enabled && account.provider_id == codex2api_core::CHATGPT => {
            account
        }
        _ => {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            return failure(StatusCode::BAD_REQUEST, "用户名或密码错误，或账号已停用。");
        }
    };
    let hash = account.password_hash.clone();
    let password = request.password;
    if !matches!(
        tokio::task::spawn_blocking(move || codex2api_storage::verify_password(&password, &hash))
            .await,
        Ok(Ok(true))
    ) {
        return failure(StatusCode::BAD_REQUEST, "用户名或密码错误，或账号已停用。");
    }
    let code = oauth_secret();
    let authorization = codex2api_storage::DeviceAuthorizationApproval {
        id: &request.request_id,
        cookie: browser,
        csrf: &request.csrf_token,
        account: &account,
        code: &code,
        scopes: codex2api_version::OAUTH_SCOPE,
    };
    match state
        .storage
        .approve_device_authorization(&normalize_code(&request.user_code), authorization)
        .await
    {
        Ok(true) => {
            let mut response = reply(StatusCode::OK, json!({"authorized":true}));
            response.headers_mut().insert(header::SET_COOKIE,format!("{cookie_name}=; HttpOnly; SameSite=Strict; Path={PREFIX}/oauth/device; Max-Age=0").parse().unwrap());
            response
        }
        Ok(false) => failure(StatusCode::BAD_REQUEST, "设备码无效、已使用或已过期。"),
        Err(error) => {
            tracing::error!(%error,"device authorization approval failed");
            failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "暂时无法授权，请稍后重试。",
            )
        }
    }
}
