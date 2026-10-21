//! API-key supplier administration: an account is created from one key with its name,
//! endpoint configuration and model list, and its key is replaced through the same
//! reauthorization entry as other channels. There is no OAuth, no device fingerprint
//! and no official quota read; the channel's "fingerprint" is its configuration.
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

/// The account configuration an administrator edits; the key has a separate endpoint.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    name: String,
    base_url: String,
    wire: String,
    auth_scheme: String,
    retry_statuses: Vec<u16>,
    retry_count: u32,
    proxy_id: Option<String>,
}
impl Config {
    fn credentials(self, api_key: String) -> (String, ApiCredentials, Option<String>) {
        let credentials = ApiCredentials {
            api_key,
            base_url: self.base_url,
            wire: self.wire,
            auth_scheme: self.auth_scheme,
            retry_statuses: self.retry_statuses,
            retry_count: self.retry_count,
        };
        (
            self.name,
            credentials,
            self.proxy_id.filter(|v| !v.is_empty()),
        )
    }
}

fn config_view(name: Option<&str>, credentials: &ApiCredentials, proxy_id: Option<&str>) -> Value {
    json!({"name":name,"base_url":credentials.base_url,"wire":credentials.wire,
        "auth_scheme":credentials.auth_scheme,"retry_statuses":credentials.retry_statuses,
        "retry_count":credentials.retry_count,"proxy_id":proxy_id})
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
    Ok(config_view(
        account.display_name.as_deref(),
        &credentials,
        account.proxy_id.as_deref(),
    ))
}

/// The form defaults of a new account.
pub async fn setup() -> ApiResult {
    let wire = channel::WIRE_CHAT_COMPLETIONS;
    Ok(Json(
        json!({"fingerprint":{"name":"","base_url":channel::default_base_url(wire),"wire":wire,
        "auth_scheme":channel::default_auth_scheme(wire),"retry_statuses":[],
        "retry_count":channel::DEFAULT_RETRY_COUNT,"proxy_id":null}}),
    ))
}

/// The URL of an outbound proxy the administrator selected, after checking it exists.
async fn proxy_url(s: &AdminState, proxy: Option<&str>) -> Result<Option<String>, ApiError> {
    Ok(match proxy {
        Some(id) => Some(s.storage.require_outbound_proxy(id).await?.url),
        None => None,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preview {
    fingerprint: Config,
    api_key: String,
}
/// The models the endpoint lists for the key being configured; nothing is stored.
pub async fn models(State(s): State<AdminState>, Json(f): Json<Preview>) -> ApiResult {
    let (_, mut credentials, proxy) = f.fingerprint.credentials(f.api_key);
    let proxy = proxy_url(&s, proxy.as_deref()).await?;
    credentials
        .validate()
        .map_err(|e| ApiError::bad(e.to_string()))?;
    let items = s
        .upstream
        .api()
        .list_models(&credentials, proxy.as_deref())
        .await
        .map_err(|error| {
            let failure = error.failure();
            let message = match (failure.status, failure.message) {
                (Some(status), Some(message)) => format!("HTTP {status}，{message}"),
                (Some(status), None) => format!("HTTP {status}"),
                (None, message) => message.unwrap_or_else(|| error.to_string()),
            };
            // A vendor may quote the key it rejected; the key is never echoed.
            ApiError::upstream(format!(
                "获取上游模型失败：{}",
                message.replace(&credentials.api_key, "…")
            ))
        })?;
    Ok(Json(json!({"items":items})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Start {
    fingerprint: Config,
    api_key: String,
    models: Vec<String>,
}
/// One key becomes one account with its name and model list; the same key at the
/// same Base URL is the same account, which is updated instead.
pub async fn start(State(s): State<AdminState>, Json(f): Json<Start>) -> ApiResult {
    let (name, credentials, proxy) = f.fingerprint.credentials(f.api_key);
    proxy_url(&s, proxy.as_deref()).await?;
    let (account, reused) = s
        .upstream
        .api()
        .auth()
        .create(&name, credentials, proxy.as_deref(), &f.models)
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    s.upstream.evict(&account.id).await;
    s.supplier_cache.invalidate_all(&account.id).await;
    Ok(Json(
        json!({"status":"complete","supplier_id":account.id,"reused_existing":reused}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relogin {
    api_key: String,
}
/// Read the saved key in the administrator's account editor. The admin router
/// requires an admin web session and marks every response as no-store.
pub async fn key(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    let account = s.storage.require_account(&id).await?;
    if account.provider_id != codex2api_common::API {
        return Err(ApiError::bad("提供商不匹配"));
    }
    let (credentials, _) = s
        .upstream
        .api()
        .credentials(&id)
        .await
        .map_err(|_| ApiError::bad("API 账户凭据无效"))?;
    Ok(Json(json!({"api_key": credentials.api_key})))
}

/// Replace the key; the name, endpoint configuration and account identity stay.
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
        .replace(&id, None, credentials)
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    s.upstream.evict(&id).await;
    s.supplier_cache.invalidate_all(&id).await;
    Ok(Json(
        json!({"status":"complete","supplier_id":id,"reused_existing":true}),
    ))
}

/// Edit the name, endpoint configuration and outbound proxy; the key stays.
pub async fn fingerprint(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(f): Json<Config>,
) -> ApiResult {
    let mut account = s.storage.require_account(&id).await?;
    if account.provider_id != codex2api_common::API {
        return Err(ApiError::bad("提供商不匹配"));
    }
    let (current, _) = s
        .upstream
        .api()
        .credentials(&id)
        .await
        .map_err(|_| ApiError::bad("API 账户凭据无效"))?;
    let (name, credentials, proxy) = f.credentials(current.api_key.clone());
    codex2api_suppliers::api::account_name(&name).map_err(|e| ApiError::bad(e.to_string()))?;
    credentials
        .clone()
        .validate()
        .map_err(|e| ApiError::bad(e.to_string()))?;
    proxy_url(&s, proxy.as_deref()).await?;
    account.proxy_id = proxy;
    s.storage.save_account_fingerprint(&account).await?;
    let saved = s
        .upstream
        .api()
        .auth()
        .replace(&id, Some(&name), credentials)
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    s.upstream.evict(&id).await;
    s.supplier_cache.invalidate_all(&id).await;
    fingerprint_view(&s, &saved).await.map(Json)
}

pub fn account_routes() -> Router<AdminState> {
    Router::new()
        .route("/setup", get(setup))
        .route("/models", post(models))
        .route("/start", post(start))
        .route("/{id}/key", get(key))
}

pub(crate) fn request_identity() -> (&'static str, String) {
    ("", codex2api_common::version::api::USER_AGENT.to_owned())
}
