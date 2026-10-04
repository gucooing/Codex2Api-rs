use crate::ApiState;
use axum::{
    Json,
    body::Bytes,
    extract::{OriginalUri, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use codex2api_storage::{CodeRedemption, GrokDevicePoll, OAuthDeviceIdentity, oauth_secret};
use codex2api_version::grok as wire;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn reply(status: StatusCode, value: Value) -> Response {
    (
        status,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        Json(value),
    )
        .into_response()
}
fn error(code: &str) -> Response {
    reply(
        StatusCode::BAD_REQUEST,
        json!({"error":code,"error_description":"The Grok authorization is unavailable or invalid."}),
    )
}
fn failure(error: impl std::fmt::Display) -> Response {
    tracing::error!(%error,"Grok OAuth failed");
    reply(
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({"error":"server_error"}),
    )
}
fn parse<T: serde::de::DeserializeOwned>(headers: &HeaderMap, body: &[u8]) -> Option<T> {
    let content_type = headers
        .get(header::CONTENT_TYPE)?
        .to_str()
        .ok()?
        .split(';')
        .next()?
        .trim();
    if content_type == "application/json" {
        return serde_json::from_slice(body).ok();
    }
    if content_type != "application/x-www-form-urlencoded" {
        return None;
    }
    let mut values = serde_json::Map::new();
    for (key, value) in url::form_urlencoded::parse(body) {
        if values
            .insert(key.into_owned(), Value::String(value.into_owned()))
            .is_some()
        {
            return None;
        }
    }
    serde_json::from_value(Value::Object(values)).ok()
}
fn origin(state: &ApiState, headers: &HeaderMap) -> Option<String> {
    state.public_base_url.clone().or_else(|| {
        let host = headers.get(header::HOST)?.to_str().ok()?;
        let url = url::Url::parse(&format!("http://{host}")).ok()?;
        (url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none())
        .then(|| url.origin().ascii_serialization())
    })
}
pub async fn discovery(
    State(state): State<ApiState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Response {
    let Some(origin) = origin(&state, &headers) else {
        return error("invalid_request");
    };
    let issuer = format!(
        "{origin}{}",
        if uri.path().starts_with("/api/oauth/grok/") {
            "/api/oauth/grok"
        } else {
            "/grok"
        }
    );
    reply(
        StatusCode::OK,
        json!({"issuer":issuer,"authorization_endpoint":format!("{issuer}/oauth2/authorize"),"token_endpoint":format!("{issuer}/oauth2/token"),
        "device_authorization_endpoint":format!("{issuer}/oauth2/device/code"),"revocation_endpoint":format!("{issuer}/oauth2/revoke"),"userinfo_endpoint":format!("{issuer}/oauth2/userinfo"),
        "jwks_uri":format!("{issuer}/.well-known/jwks.json"),"response_types_supported":["code"],"subject_types_supported":["public"],"id_token_signing_alg_values_supported":["RS256"],
        "scopes_supported":wire::SCOPE.split_whitespace().collect::<Vec<_>>(),"token_endpoint_auth_methods_supported":["none"],"grant_types_supported":["authorization_code","refresh_token","urn:ietf:params:oauth:grant-type:device_code"],"code_challenge_methods_supported":["S256"]}),
    )
}
pub async fn jwks(State(state): State<ApiState>) -> Response {
    match super::jwt::jwks(&state.storage).await {
        Ok(value) => reply(StatusCode::OK, value),
        Err(e) => failure(e),
    }
}
pub async fn authorize(
    State(state): State<ApiState>,
    Query(request): Query<codex2api_user::AuthorizationRequest>,
    OriginalUri(uri): OriginalUri,
) -> Response {
    if !request.valid() || request.provider() != codex2api_core::GROK {
        return error("invalid_request");
    }
    Redirect::to(&format!(
        "{}/user/authorize/?{}",
        state.user_base_url,
        uri.query().unwrap_or_default()
    ))
    .into_response()
}
#[derive(Deserialize)]
struct DeviceRequest {
    client_id: String,
    #[serde(default)]
    scope: String,
}
pub async fn device(State(state): State<ApiState>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(input) = parse::<DeviceRequest>(&headers, &body) else {
        return error("invalid_request");
    };
    if input.client_id != wire::CLIENT_ID
        || !input
            .scope
            .split_whitespace()
            .all(|s| wire::SCOPE.split_whitespace().any(|a| a == s))
    {
        return error("invalid_scope");
    }
    let id = oauth_secret();
    let raw = oauth_secret()[..12].to_ascii_uppercase();
    let code = format!("{}-{}-{}", &raw[..4], &raw[4..8], &raw[8..]);
    let verifier = oauth_secret();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    match state
        .storage
        .create_device_authorization_scoped(
            codex2api_storage::DeviceAuthorization {
                id: &id,
                user_code: &raw,
                client_id: wire::CLIENT_ID,
                redirect_uri: "urn:grok:device",
                verifier: &verifier,
                challenge: &challenge,
            },
            Some(if input.scope.is_empty() {
                wire::SCOPE
            } else {
                &input.scope
            }),
        )
        .await
    {
        Ok(true) => reply(
            StatusCode::OK,
            json!({"device_code":id,"user_code":code,"expires_in":900,"interval":5,
            "verification_uri":format!("{}/user/device/?provider=grok",state.user_base_url),"verification_uri_complete":format!("{}/user/device/?provider=grok&user_code={code}",state.user_base_url)}),
        ),
        Ok(false) => reply(StatusCode::TOO_MANY_REQUESTS, json!({"error":"slow_down"})),
        Err(e) => failure(e),
    }
}
#[derive(Deserialize)]
struct TokenRequest {
    client_id: String,
    grant_type: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    redirect_uri: String,
    #[serde(default)]
    code_verifier: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    device_code: String,
    scope: Option<String>,
}
pub async fn token(
    State(state): State<ApiState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(mut input) = parse::<TokenRequest>(&headers, &body) else {
        return error("invalid_request");
    };
    if input.client_id != wire::CLIENT_ID {
        return error("invalid_client");
    }
    let Some(origin) = origin(&state, &headers) else {
        return error("invalid_request");
    };
    let issuer = format!(
        "{origin}{}",
        if uri.path().starts_with("/api/oauth/grok/") {
            "/api/oauth/grok"
        } else {
            "/grok"
        }
    );
    let device = OAuthDeviceIdentity::new(
        None,
        headers
            .get(header::USER_AGENT)
            .and_then(|s| s.to_str().ok())
            .unwrap_or(""),
    );
    if input.grant_type == "urn:ietf:params:oauth:grant-type:device_code" {
        if input.device_code.len() != 64 {
            return error("invalid_grant");
        }
        match state.storage.poll_grok_device(&input.device_code).await {
            Ok(GrokDevicePoll::Pending) => return error("authorization_pending"),
            Ok(GrokDevicePoll::SlowDown) => return error("slow_down"),
            Ok(GrokDevicePoll::Expired) => return error("expired_token"),
            Ok(GrokDevicePoll::Authorized(code)) => {
                input.grant_type = "authorization_code".into();
                input.code = code.authorization_code;
                input.code_verifier = code.code_verifier;
                input.redirect_uri = "urn:grok:device".into();
            }
            Err(e) => return failure(e),
        }
    }
    let mut nonce = None;
    let session = match input.grant_type.as_str() {
        "authorization_code" => {
            if !(43..=128).contains(&input.code_verifier.len())
                || !input
                    .code_verifier
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
            {
                return error("invalid_grant");
            }
            nonce = match state.storage.grok_code_nonce(&input.code).await {
                Ok(v) => v,
                Err(e) => return failure(e),
            };
            input.refresh_token = format!("grok_rt_{}", oauth_secret());
            state
                .storage
                .redeem_oauth_code(CodeRedemption {
                    provider: codex2api_core::GROK,
                    code: &input.code,
                    client_id: wire::CLIENT_ID,
                    redirect_uri: &input.redirect_uri,
                    challenge: &URL_SAFE_NO_PAD
                        .encode(Sha256::digest(input.code_verifier.as_bytes())),
                    refresh: &input.refresh_token,
                    device: &device,
                })
                .await
        }
        "refresh_token" => match state
            .storage
            .virtual_refresh_device(&input.refresh_token)
            .await
        {
            Ok(Some(device)) if device.provider_id == codex2api_core::GROK => match state
                .storage
                .effective_virtual_account(&device.virtual_account_id)
                .await
            {
                Ok(Some(account)) if account.enabled => Ok(Some((account, device.id))),
                Ok(_) => Ok(None),
                Err(e) => Err(e),
            },
            Ok(_) => Ok(None),
            Err(e) => Err(e),
        },
        _ => return error("unsupported_grant_type"),
    };
    let (account, device_id) = match session {
        Ok(Some(v)) => v,
        Ok(None) => return error("invalid_grant"),
        Err(e) => return failure(e),
    };
    let grant = match state.storage.virtual_devices(&account.id).await {
        Ok(v) => v.into_iter().find(|d| d.id == device_id),
        Err(e) => return failure(e),
    };
    let Some(grant) = grant else {
        return error("invalid_grant");
    };
    let scopes = input.scope.as_deref().unwrap_or(&grant.scopes);
    if !scopes
        .split_whitespace()
        .all(|s| grant.scopes.split_whitespace().any(|g| g == s))
    {
        return error("invalid_scope");
    }
    let now = chrono::Utc::now().timestamp();
    let mut identity = json!({"iss":issuer,"aud":wire::CLIENT_ID,"sub":account.id,"iat":now,"exp":now+wire::ACCESS_TOKEN_TTL});
    if scopes.split_whitespace().any(|s| s == "email") {
        identity["email"] = account.email.clone().into();
        identity["email_verified"] = false.into();
    }
    if scopes.split_whitespace().any(|s| s == "profile") {
        identity["name"] = account.name.clone().into();
        identity["given_name"] = account.name.clone().into();
    }
    if let Some(nonce) = nonce {
        identity["nonce"] = nonce.into();
    }
    let mut access = json!({"iss":issuer,"aud":"codex2api-grok","sub":account.id,"provider_id":"grok","token_use":"access","scope":scopes,"iat":now,"exp":now+wire::ACCESS_TOKEN_TTL,"jti":oauth_secret()});
    if let Some(tier) = codex2api_core::providers::grok::subscription(account.effective_plan()) {
        access["tier"] = tier.jwt_tier.into();
    }
    let (access, id_token) = match super::jwt::issue(&state.storage, [access, identity]).await {
        Ok(v) => v,
        Err(e) => return failure(e),
    };
    let refresh = format!("grok_rt_{}", oauth_secret());
    match state
        .storage
        .rotate_grok_access(
            &device_id,
            &input.refresh_token,
            &refresh,
            &access,
            now + wire::ACCESS_TOKEN_TTL,
            scopes,
        )
        .await
    {
        Ok(true) => reply(
            StatusCode::OK,
            json!({"access_token":access,"id_token":id_token,"refresh_token":refresh,"token_type":"Bearer","expires_in":wire::ACCESS_TOKEN_TTL,"scope":scopes}),
        ),
        Ok(false) => error("invalid_grant"),
        Err(e) => failure(e),
    }
}
#[derive(Deserialize)]
struct Revoke {
    token: String,
    client_id: Option<String>,
}
pub async fn revoke(State(state): State<ApiState>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(input) = parse::<Revoke>(&headers, &body) else {
        return error("invalid_request");
    };
    if input
        .client_id
        .as_deref()
        .is_some_and(|v| v != wire::CLIENT_ID)
    {
        return error("invalid_client");
    }
    match state
        .storage
        .revoke_virtual_token(&input.token, codex2api_core::GROK)
        .await
    {
        Ok(()) => reply(StatusCode::OK, json!({})),
        Err(e) => failure(e),
    }
}
