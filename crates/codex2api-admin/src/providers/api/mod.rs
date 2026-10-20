//! API-key supplier administration: keys are entered one per line, each becoming an
//! account with its Base URL, wire and authentication scheme; a key is replaced
//! through the same reauthorization entry as other channels. There is no OAuth, no
//! device fingerprint and no official quota read; the channel's "fingerprint" is its
//! endpoint configuration.
pub(crate) mod transfer;
use crate::{
    AdminState,
    rest::error::{ApiError, ApiResult},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use codex2api_common::providers::api as channel;
use codex2api_storage::SupplierAccount;
use codex2api_suppliers::api::ApiCredentials;
use serde::Deserialize;
use serde_json::{Value, json};

/// The endpoint configuration an administrator edits; the key itself is never shown.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    base_url: String,
    wire: String,
    auth_scheme: String,
    #[serde(default)]
    retry_statuses: Vec<u16>,
    proxy_id: Option<String>,
}

fn config_view(credentials: &ApiCredentials, proxy_id: Option<&str>) -> Value {
    json!({"base_url":credentials.base_url,"wire":credentials.wire,"auth_scheme":credentials.auth_scheme,
        "retry_statuses":credentials.retry_statuses,"key_suffix":credentials.key_suffix(),"proxy_id":proxy_id})
}

pub(crate) async fn fingerprint_view(
    s: &AdminState,
    account: &SupplierAccount,
) -> Result<Value, ApiError> {
    let (credentials, _) = s
        .upstream
        .api()
        .credentials(&account.id)
        .await
        .map_err(|_| ApiError::bad("API 账户凭据无效"))?;
    Ok(config_view(&credentials, account.proxy_id.as_deref()))
}

/// The form defaults of a new account.
pub async fn setup() -> ApiResult {
    let wire = channel::WIRE_CHAT_COMPLETIONS;
    Ok(Json(
        json!({"fingerprint":{"base_url":channel::default_base_url(wire),"wire":wire,
        "auth_scheme":channel::default_auth_scheme(wire),"retry_statuses":[],"proxy_id":null}}),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Method {
    ApiKey,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Start {
    #[allow(dead_code)]
    method: Method,
    fingerprint: Config,
    api_key: String,
    #[serde(default)]
    name: Option<String>,
}
fn authorized(id: &str, reused: bool) -> ApiResult {
    Ok(Json(
        json!({"status":"complete","supplier_id":id,"reused_existing":reused}),
    ))
}

/// One key becomes one account; the same key at the same Base URL is the same account.
pub async fn start(State(s): State<AdminState>, Json(f): Json<Start>) -> ApiResult {
    let proxy = f.fingerprint.proxy_id.as_deref().filter(|v| !v.is_empty());
    if let Some(proxy) = proxy {
        s.storage.require_outbound_proxy(proxy).await?;
    }
    let credentials = ApiCredentials {
        api_key: f.api_key,
        base_url: f.fingerprint.base_url,
        wire: f.fingerprint.wire,
        auth_scheme: f.fingerprint.auth_scheme,
        retry_statuses: f.fingerprint.retry_statuses,
    };
    let (account, reused) = s
        .upstream
        .api()
        .auth()
        .create(f.name.as_deref(), credentials, proxy)
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    s.supplier_cache.invalidate_all(&account.id).await;
    authorized(&account.id, reused)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relogin {
    #[allow(dead_code)]
    method: Method,
    api_key: String,
}
/// Replace the key; the endpoint configuration and the account identity stay.
pub async fn relogin(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(f): Json<Relogin>,
) -> ApiResult {
    let account = s.storage.require_account(&id).await?;
    if account.provider_id != codex2api_common::API {
        return Err(ApiError::bad("提供商不匹配"));
    }
    let (current, _) = s
        .upstream
        .api()
        .credentials(&id)
        .await
        .map_err(|_| ApiError::bad("API 账户凭据无效"))?;
    let credentials = ApiCredentials {
        api_key: f.api_key,
        ..(*current).clone()
    };
    s.upstream
        .api()
        .auth()
        .replace(&id, credentials)
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    s.upstream.evict(&id).await;
    s.supplier_cache.invalidate_all(&id).await;
    authorized(&id, true)
}

/// Edit the endpoint configuration and outbound proxy; the key stays.
pub async fn fingerprint(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(f): Json<Config>,
) -> ApiResult {
    let mut account = s.storage.require_account(&id).await?;
    if account.provider_id != codex2api_common::API {
        return Err(ApiError::bad("提供商不匹配"));
    }
    if let Some(proxy) = f.proxy_id.as_deref().filter(|v| !v.is_empty()) {
        s.storage.require_outbound_proxy(proxy).await?;
    }
    let (current, _) = s
        .upstream
        .api()
        .credentials(&id)
        .await
        .map_err(|_| ApiError::bad("API 账户凭据无效"))?;
    let credentials = ApiCredentials {
        api_key: current.api_key.clone(),
        base_url: f.base_url,
        wire: f.wire,
        auth_scheme: f.auth_scheme,
        retry_statuses: f.retry_statuses,
    };
    account.proxy_id = f.proxy_id.filter(|v| !v.is_empty());
    s.storage.save_account_fingerprint(&account).await?;
    let saved = s
        .upstream
        .api()
        .auth()
        .replace(&id, credentials.clone())
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    s.upstream.evict(&id).await;
    s.supplier_cache.invalidate_all(&id).await;
    Ok(Json(config_view(&credentials, saved.proxy_id.as_deref())))
}

pub fn account_routes() -> Router<AdminState> {
    Router::new()
        .route("/setup", get(setup))
        .route("/start", post(start))
}

pub(crate) fn request_identity() -> (&'static str, String) {
    ("", codex2api_common::version::api::USER_AGENT.to_owned())
}
