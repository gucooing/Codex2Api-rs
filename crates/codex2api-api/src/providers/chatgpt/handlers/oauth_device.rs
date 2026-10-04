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

pub async fn page(State(state): State<ApiState>) -> Redirect {
    Redirect::to(&format!("{}/user/device/", state.user_base_url))
}
