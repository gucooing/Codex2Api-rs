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
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{ApiError, ApiState};
use codex2api_storage::{OAuthDeviceIdentity, hash_token};

pub const PREFIX: &str = "/api/oauth/chatgpt";
const ACCESS_TTL: i64 = codex2api_version::OAUTH_ACCESS_TOKEN_TTL;

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
        "The login session or account is unavailable.",
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

fn parse<T: DeserializeOwned>(
    headers: &HeaderMap,
    body: &[u8],
) -> Result<T, (StatusCode, &'static str)> {
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
                return Err((StatusCode::BAD_REQUEST, "Duplicate form parameter."));
            }
        }
        serde_json::from_value(Value::Object(map))
    } else {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Use JSON or form-encoded content.",
        ));
    };
    result.map_err(|_| (StatusCode::BAD_REQUEST, "Invalid OAuth request body."))
}

#[derive(Deserialize)]
struct TokenRequest {
    grant_type: String,
    #[serde(default)]
    refresh_token: String,
    client_id: Option<String>,
    code: Option<String>,
    redirect_uri: Option<String>,
    code_verifier: Option<String>,
    scope: Option<String>,
}

pub async fn token(State(state): State<ApiState>, headers: HeaderMap, body: Bytes) -> Response {
    let mut request: TokenRequest = match parse(&headers, &body) {
        Ok(v) => v,
        Err((status, message)) => return error(status, "invalid_request", message),
    };
    if request.grant_type != "refresh_token" && request.grant_type != "authorization_code" {
        return error(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "Supported grant types are refresh_token and authorization_code.",
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
    let device_info = OAuthDeviceIdentity::new(
        headers
            .get("x-codex-installation-id")
            .and_then(|v| v.to_str().ok()),
        headers
            .get(header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .unwrap_or(""),
    );
    let (account, device_id) = if request.grant_type == "authorization_code" {
        let (Some(code), Some(redirect), Some(verifier), Some(client)) = (
            &request.code,
            &request.redirect_uri,
            &request.code_verifier,
            &request.client_id,
        ) else {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "Authorization code, redirect URI, client ID and PKCE verifier are required.",
            );
        };
        if !(43..=128).contains(&verifier.len())
            || !verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
        {
            return invalid_grant();
        }
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        request.refresh_token = format!("c2rt_{}", codex2api_storage::oauth_secret());
        match state
            .storage
            .redeem_oauth_code(codex2api_storage::CodeRedemption {
                provider: "chatgpt",
                code,
                client_id: client,
                redirect_uri: redirect,
                challenge: &challenge,
                refresh: &request.refresh_token,
                device: &device_info,
            })
            .await
        {
            Ok(Some(session)) => session,
            Ok(None) => return invalid_grant(),
            Err(e) => return internal(e),
        }
    } else {
        let device = match state
            .storage
            .virtual_refresh_device(&request.refresh_token)
            .await
        {
            Ok(Some(v))
                if v.provider_id == codex2api_core::CHATGPT
                    && v.scopes.split_whitespace().any(|s| s == "offline_access") =>
            {
                v
            }
            Ok(_) => return invalid_grant(),
            Err(e) => return internal(e),
        };
        let account = match state
            .storage
            .virtual_account(&device.virtual_account_id)
            .await
        {
            Ok(Some(v)) if v.enabled && v.provider_id == codex2api_core::CHATGPT => v,
            Ok(_) => return invalid_grant(),
            Err(e) => return internal(e),
        };
        (account, device.id)
    };
    let device = match state.storage.virtual_devices(&account.id).await {
        Ok(devices) => match devices
            .into_iter()
            .find(|d| d.id == device_id && d.provider_id == codex2api_core::CHATGPT)
        {
            Some(d) => d,
            None => return invalid_grant(),
        },
        Err(error) => return internal(error),
    };
    let granted_scopes = if let Some(requested) = request.scope {
        if !requested.split_whitespace().all(|scope| {
            device
                .scopes
                .split_whitespace()
                .any(|allowed| scope == allowed)
        }) {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid_scope",
                "The requested scope exceeds the original authorization.",
            );
        }
        requested.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        device.scopes.clone()
    };
    let (authenticated_at_ms, requested_at_ms, subscription_started_at) = match state
        .storage
        .oauth_session_claim_times(&account.id, &device_id)
        .await
    {
        Ok(times) => times,
        Err(e) => return internal(e),
    };
    let now = chrono::Utc::now().timestamp();
    let claims = super::oauth_jwt::claims(
        &account,
        &device_id,
        &granted_scopes,
        authenticated_at_ms,
        requested_at_ms,
        subscription_started_at.as_deref(),
        now,
    );
    let (access_token, id_token) = match super::oauth_jwt::issue_pair(&state.storage, claims).await
    {
        Ok(pair) => pair,
        Err(e) => return internal(e),
    };
    match state
        .storage
        .register_virtual_access_scoped(
            &device_id,
            &request.refresh_token,
            &access_token,
            now + ACCESS_TTL,
            Some(&granted_scopes),
        )
        .await
    {
        Ok(true) => {
            let mut tokens = json!({"access_token":access_token,"token_type":"Bearer","expires_in":ACCESS_TTL,"scope":granted_scopes});
            if granted_scopes
                .split_whitespace()
                .any(|scope| scope == "openid")
            {
                tokens["id_token"] = id_token.into();
            }
            if granted_scopes
                .split_whitespace()
                .any(|scope| scope == "offline_access")
            {
                tokens["refresh_token"] = request.refresh_token.into();
            }
            reply(StatusCode::OK, tokens)
        }
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
        Err((status, message)) => return error(status, "invalid_request", message),
    };
    match state
        .storage
        .revoke_virtual_token(&request.token, codex2api_core::CHATGPT)
        .await
    {
        Ok(()) => reply(StatusCode::OK, json!({})),
        Err(e) => internal(e),
    }
}

pub async fn require_oauth(
    State(state): State<ApiState>,
    mut request: Request,
    next: Next,
) -> crate::Result<Response> {
    let bearer = crate::providers::chatgpt::access::extract_bearer(request.headers())
        .map_err(|_| invalid_access())?;
    let credential = state
        .storage
        .virtual_access(&hash_token(bearer))
        .await?
        .ok_or_else(invalid_access)?;
    if credential.provider_id != codex2api_core::CHATGPT {
        return Err(codex2api_service::ServiceError::Policy(
            codex2api_core::PolicyError::ProviderMismatch,
        )
        .into());
    }
    if request
        .headers()
        .get_all("chatgpt-account-id")
        .iter()
        .any(|v| v.to_str().ok() != Some(credential.virtual_account_id.as_str()))
    {
        return Err(ApiError::openai(
            StatusCode::FORBIDDEN,
            "permission_error",
            "The account does not match this OAuth credential.",
            Some("account_mismatch"),
        ));
    }
    state
        .storage
        .touch_virtual_access(&credential.token_hash)
        .await?;
    for (name, value) in url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
    {
        if name == "account_id" && value != credential.virtual_account_id {
            return Err(ApiError::openai(
                StatusCode::FORBIDDEN,
                "permission_error",
                "The account does not match this credential.",
                Some("account_mismatch"),
            ));
        }
    }
    let path = request.uri().path();
    let required = if path.ends_with("/ps/mcp") {
        Some("api.connectors.invoke")
    } else if path.contains("/connectors/") || path.ends_with("/apps") {
        Some("api.connectors.read")
    } else {
        None
    };
    if required.is_some_and(|scope| {
        !credential
            .scopes
            .split_whitespace()
            .any(|granted| granted == scope)
    }) {
        return Err(ApiError::openai(
            StatusCode::FORBIDDEN,
            "permission_error",
            "This OAuth authorization does not include the required scope.",
            Some("insufficient_scope"),
        ));
    }
    // Query/body/header secrets are never part of the activity log.
    let method = request.method().to_string();
    let path = request
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|p| p.as_str())
        .unwrap_or(request.uri().path())
        .to_owned();
    let started = std::time::Instant::now();
    request.extensions_mut().insert(credential.clone());
    let response = next.run(request).await;
    if let Err(error) = state
        .storage
        .record_virtual_request(
            &credential.virtual_account_id,
            &credential.device_id,
            &method,
            &path,
            response.status().as_u16(),
            started.elapsed().as_millis().min(i64::MAX as u128) as i64,
        )
        .await
    {
        tracing::error!(%error,"Could not save virtual client request log");
    }
    Ok(response)
}

fn invalid_access() -> ApiError {
    ApiError::openai(
        StatusCode::UNAUTHORIZED,
        "authentication_error",
        "Invalid or expired OAuth access token.",
        Some("invalid_token"),
    )
}
