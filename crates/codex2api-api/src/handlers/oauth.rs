//! OAuth facade for third-party clients; upstream credentials never leave the proxy.
use axum::body::Bytes;
use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::Sha256;

use crate::{ApiError, ApiState};
use codex2api_storage::{OAuthDeviceIdentity, hash_api_key};

pub const PREFIX: &str = "/api/oauth/chatgpt";
const ACCESS_TTL: i64 = 3600;

fn reply(status: StatusCode, body: Value) -> Response {
    (
        status,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        Json(body),
    )
        .into_response()
}

fn error(status: StatusCode, code: &str, description: &str) -> Response {
    reply(
        status,
        json!({"error":code,"error_description":description}),
    )
}

fn invalid_grant() -> Response {
    error(
        StatusCode::BAD_REQUEST,
        "invalid_grant",
        "The refresh token or its bound account is unavailable.",
    )
}

fn internal(err: impl std::fmt::Display) -> Response {
    tracing::error!(error = %err, "proxy OAuth operation failed");
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "server_error",
        "Unable to process the OAuth request.",
    )
}

fn parse<T: DeserializeOwned>(headers: &HeaderMap, body: &[u8]) -> Result<T, Response> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .unwrap_or("")
        .trim();
    let result = if content_type.eq_ignore_ascii_case("application/json") {
        serde_json::from_slice(body)
    } else if content_type.eq_ignore_ascii_case("application/x-www-form-urlencoded") {
        let mut map = serde_json::Map::new();
        for (key, value) in url::form_urlencoded::parse(body) {
            if map
                .insert(key.into_owned(), Value::String(value.into_owned()))
                .is_some()
            {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "Duplicate form parameter.",
                ));
            }
        }
        serde_json::from_value(Value::Object(map))
    } else {
        return Err(error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "invalid_request",
            "Use JSON or form-encoded content.",
        ));
    };
    result.map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Invalid OAuth request body.",
        )
    })
}

#[derive(Deserialize)]
struct TokenRequest {
    grant_type: String,
    refresh_token: String,
    client_id: Option<String>,
}

fn sign(claims: &Value, secret: &str) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(claims.to_string());
    let input = format!("{header}.{payload}");
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(input.as_bytes());
    format!(
        "{input}.{}",
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    )
}

pub async fn token(State(state): State<ApiState>, headers: HeaderMap, body: Bytes) -> Response {
    let request: TokenRequest = match parse(&headers, &body) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if request.grant_type != "refresh_token" {
        return error(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "Only refresh_token is supported.",
        );
    }
    if request
        .client_id
        .as_deref()
        .is_some_and(|id| id != codex2api_version::OAUTH_CLIENT_ID)
    {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_client",
            "Unknown client_id.",
        );
    }
    let credential = match state
        .storage
        .lookup_oauth_refresh(&request.refresh_token)
        .await
    {
        Ok(Some(v)) => v,
        Ok(None) => return invalid_grant(),
        Err(e) => return internal(e),
    };
    let account = match state.storage.get_account(&credential.account_id).await {
        Ok(Some(v)) => v,
        Ok(None) => return invalid_grant(),
        Err(e) => return internal(e),
    };
    let Some(account_id) = account
        .chatgpt_account_id
        .as_deref()
        .filter(|v| !v.is_empty())
    else {
        return invalid_grant();
    };
    let secret = match state.storage.oauth_signing_key().await {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    let now = chrono::Utc::now().timestamp();
    let auth = json!({
        "chatgpt_account_id": account_id,
        "chatgpt_user_id": account.chatgpt_user_id,
        "user_id": account.chatgpt_user_id,
        "chatgpt_plan_type": account.plan_type,
    });
    let mut claims = json!({
        "iss": "codex2api", "aud": codex2api_version::OAUTH_CLIENT_ID,
        "sub": account.chatgpt_user_id.as_deref().unwrap_or(account_id),
        "iat": now, "exp": now + ACCESS_TTL, "jti": uuid::Uuid::new_v4().to_string(),
        "email": account.email,
        "https://api.openai.com/auth": auth,
        "https://api.openai.com/profile": {"email":account.email},
        "token_use":"access",
    });
    let access_token = sign(&claims, &secret);
    claims["token_use"] = "id".into();
    claims["jti"] = uuid::Uuid::new_v4().to_string().into();
    let id_token = sign(&claims, &secret);
    let device = OAuthDeviceIdentity::new(
        headers
            .get("x-codex-installation-id")
            .and_then(|v| v.to_str().ok()),
        headers
            .get(header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .unwrap_or(""),
    );
    match state
        .storage
        .register_oauth_access(
            &credential,
            &request.refresh_token,
            &access_token,
            now + ACCESS_TTL,
            &device,
        )
        .await
    {
        Ok(true) => reply(
            StatusCode::OK,
            json!({
                "access_token":access_token, "id_token":id_token, "refresh_token":request.refresh_token,
                "token_type":"Bearer", "expires_in":ACCESS_TTL,
                "scope":codex2api_version::OAUTH_SCOPE,
            }),
        ),
        Ok(false) => invalid_grant(),
        Err(e) => internal(e),
    }
}

#[derive(Deserialize)]
struct RevokeRequest {
    token: String,
}

pub async fn revoke(State(state): State<ApiState>, headers: HeaderMap, body: Bytes) -> Response {
    let request: RevokeRequest = match parse(&headers, &body) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match state.storage.revoke_oauth_token(&request.token).await {
        Ok(()) => reply(StatusCode::OK, json!({})),
        Err(e) => internal(e),
    }
}

pub async fn require_oauth(
    State(state): State<ApiState>,
    mut request: Request,
    next: Next,
) -> crate::Result<Response> {
    let bearer = crate::auth::extract_bearer(request.headers()).map_err(|_| invalid_access())?;
    let credential = state
        .storage
        .lookup_oauth_access_hash(&hash_api_key(bearer))
        .await?
        .ok_or_else(invalid_access)?;
    request.extensions_mut().insert(credential);
    Ok(next.run(request).await)
}

fn invalid_access() -> ApiError {
    ApiError::openai(
        StatusCode::UNAUTHORIZED,
        "authentication_error",
        "Invalid or expired OAuth access token.",
        Some("invalid_token"),
    )
}

#[cfg(test)]
#[path = "oauth_tests.rs"]
mod tests;
