//! Public Desktop support protocols; private evaluations require an authenticated snapshot.
use axum::{
    body::{Body, Bytes},
    extract::{Extension, OriginalUri, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use codex2api_storage::VirtualAccess;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::Read;

const MAX_BODY: usize = 4 * 1024 * 1024;
#[derive(Clone, Copy)]
pub(crate) enum Intake {
    Telemetry,
    Events,
    Exception,
}

fn json_response(value: Value) -> Response {
    let mut response = crate::providers::chatgpt::identity::json_response(value);
    response
        .headers_mut()
        .insert("access-control-allow-origin", "*".parse().unwrap());
    response
}

pub(crate) async fn preflight() -> Response {
    (StatusCode::NO_CONTENT,[("access-control-allow-origin","*"),("access-control-allow-methods","GET, POST, OPTIONS"),("access-control-allow-headers","content-type, authorization, statsig-api-key, statsig-sdk-type, statsig-sdk-version, x-request-id")]).into_response()
}

fn decoded(body: &[u8], headers: &HeaderMap, query: Option<&str>) -> crate::Result<Vec<u8>> {
    if body.len() > MAX_BODY {
        return Err(crate::ApiError::bad_request("SDK request is too large."));
    }
    let params: std::collections::HashMap<_, _> =
        url::form_urlencoded::parse(query.unwrap_or("").as_bytes())
            .into_owned()
            .collect();
    let mut bytes = body.to_vec();
    if params.get("gz").is_some_and(|v| v == "1")
        || headers.get("content-encoding").is_some_and(|v| v == "gzip")
    {
        let mut output = Vec::new();
        flate2::read::GzDecoder::new(bytes.as_slice())
            .take((MAX_BODY + 1) as u64)
            .read_to_end(&mut output)
            .map_err(|_| crate::ApiError::bad_request("Invalid compressed SDK request."))?;
        bytes = output;
    }
    if bytes.len() > MAX_BODY {
        return Err(crate::ApiError::bad_request("SDK request is too large."));
    }
    if params.get("se").is_some_and(|v| v == "1") {
        bytes.reverse();
        bytes = STANDARD
            .decode(bytes)
            .map_err(|_| crate::ApiError::bad_request("Invalid SDK request encoding."))?;
    }
    Ok(bytes)
}

async fn optional_access(
    state: &crate::ApiState,
    headers: &HeaderMap,
) -> crate::Result<Option<VirtualAccess>> {
    if !headers.contains_key("authorization") {
        return Ok(None);
    }
    let token = crate::providers::chatgpt::access::extract_bearer(headers)?;
    let access = state
        .storage
        .virtual_access(&codex2api_storage::hash_token(token))
        .await?
        .filter(|access| access.provider_id == codex2api_core::CHATGPT)
        .ok_or_else(crate::ApiError::invalid_token)?;
    if headers
        .get("chatgpt-account-id")
        .is_some_and(|v| v.to_str().ok() != Some(access.virtual_account_id.as_str()))
    {
        return Err(crate::ApiError::invalid_token());
    }
    Ok(Some(access))
}

pub(crate) async fn intake(
    State(state): State<crate::ApiState>,
    Extension(kind): Extension<Intake>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> crate::Result<Response> {
    let access = optional_access(&state, &headers).await?;
    let bytes = decoded(&body, &headers, uri.query())?;
    let records: Vec<Value> = match kind {
        Intake::Telemetry => std::str::from_utf8(&bytes)
            .map_err(|_| crate::ApiError::bad_request("Invalid telemetry text."))?
            .lines()
            .filter(|s| !s.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()
            .map_err(|_| crate::ApiError::bad_request("Invalid telemetry JSON lines."))?,
        Intake::Events => serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|v| v["events"].as_array().cloned())
            .ok_or_else(|| crate::ApiError::bad_request("Expected SDK events."))?,
        Intake::Exception => vec![
            serde_json::from_slice(&bytes)
                .map_err(|_| crate::ApiError::bad_request("Invalid SDK exception."))?,
        ],
    };
    if records.len() > 1000 || records.iter().any(|v| !v.is_object()) {
        return Err(crate::ApiError::bad_request("Invalid SDK batch."));
    }
    let source = match kind {
        Intake::Telemetry => "telemetry",
        Intake::Events => "statsig_events",
        Intake::Exception => "sdk_exception",
    };
    // These senders also run before login. Body user IDs are untrusted claims,
    // not evidence that a diagnostic belongs to a virtual account.
    let token = |v: &Value| {
        v.as_str()
            .filter(|s| {
                s.len() <= 160
                    && s.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "_:-./ ".contains(c))
            })
            .map(str::to_owned)
    };
    let summaries:Vec<_>=records.iter().map(|v|json!({
        "event":token(&v["eventName"]),"level":token(&v["status"]),"logger":token(&v["logger"]["name"]),
        "tag":token(&v["tag"]),"exception":token(&v["exception"]),"reason":token(&v["reason"]),"sdk_version":token(&v["sdkVersion"])
    })).collect();
    if state
        .storage
        .desktop_support_settings()
        .await?
        .collect_diagnostics
    {
        let owner = access.as_ref().map(|a| a.virtual_account_id.as_str());
        let mut digest = Sha256::new();
        digest.update(source);
        digest.update(owner.unwrap_or("anonymous"));
        digest.update(&bytes);
        state
            .storage
            .record_desktop_diagnostic(
                &format!("{:x}", digest.finalize()),
                source,
                owner,
                records.len(),
                &json!(summaries),
            )
            .await?;
    }
    // The actual intake and exception readers ignore the success body. Statsig's
    // event logger reads success, so return it only after the persistence above.
    Ok(match kind {
        Intake::Events => json_response(json!({"success":true})),
        _ => preflight().await,
    })
}

#[derive(Serialize, Deserialize)]
struct Snapshot {
    owner: String,
    device: String,
    stable: Option<String>,
    beta: bool,
    expires: i64,
    digest: String,
}
async fn checksum(
    state: &crate::ApiState,
    payload: &Value,
    access: &VirtualAccess,
    stable: Option<String>,
    beta: bool,
) -> crate::Result<String> {
    let ticket = Snapshot {
        owner: access.virtual_account_id.clone(),
        device: access.device_id.clone(),
        stable,
        beta,
        expires: chrono::Utc::now().timestamp() + 86400,
        digest: format!("{:x}", Sha256::digest(payload.to_string())),
    };
    let encoded = URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&ticket).map_err(|e| crate::ApiError::internal(e.to_string()))?);
    let key = state.storage.oauth_signing_key().await?;
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).unwrap();
    mac.update(b"desktop-config:");
    mac.update(encoded.as_bytes());
    Ok(format!(
        "{encoded}.{}",
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    ))
}

pub(crate) async fn bootstrap(
    state: &crate::ApiState,
    access: &VirtualAccess,
    beta: bool,
    stable: Option<String>,
) -> crate::Result<Value> {
    let account = state
        .storage
        .virtual_account(&access.virtual_account_id)
        .await?
        .filter(|a| a.enabled)
        .ok_or_else(crate::ApiError::invalid_token)?;
    let mut payload = state
        .storage
        .virtual_config(&account.id, "feature_bootstrap")
        .await?
        .value;
    let hash = codex2api_storage::statsig_hash;
    let controls = state
        .storage
        .virtual_config(&account.id, "computer_use_policy")
        .await?
        .value;
    // Actual Desktop AWn/jWn and uHn/fHn gates. Missing evaluations mean
    // "disabled by organization or region" even when native requirements allow use.
    for (gate, key) in [
        ("410065390", "browser_enabled"),
        ("1506311413", "computer_enabled"),
    ] {
        payload["feature_gates"][hash(gate)] =
            json!({"name":hash(gate),"value":controls[key],"rule_id":"local"});
    }
    let model_policy = state
        .storage
        .virtual_config(&account.id, "desktop_model_policy")
        .await?
        .value;
    // agent-settings br() controls the section; PLa/O9n retain only model-supported levels.
    for (gate, key) in [
        ("3693343337", "reasoning_settings_enabled"),
        ("1186680773", "ultra_effort_available"),
    ] {
        payload["feature_gates"][hash(gate)] =
            json!({"name":hash(gate),"value":model_policy[key],"rule_id":"local"});
    }
    // Installed language resources and implemented profile routes are protocol
    // capabilities. A server-side preference must not override the user's locale.
    payload["layer_configs"][hash("72216192")] = json!({"name":hash("72216192"),"value":{"enable_i18n":true,"locale_source":"FIRST_AVAILABLE"},"rule_id":"local"});
    payload["layer_configs"][hash("3503973010")] = json!({"name":hash("3503973010"),"value":{"show_dropdown_entry_point":true},"rule_id":"local"});
    payload["feature_gates"][hash("2478676115")] =
        json!({"name":hash("2478676115"),"value":true,"rule_id":"local"});
    let mut ids = json!({"account_id":account.id});
    if let Some(stable) = &stable {
        ids["stableID"] = stable.clone().into();
    }
    payload["user"] = json!({"userID":format!("user-{}",account.id),"email":account.email,"customIDs":ids,"custom":{"auth_method":"chatgpt","plan_type":account.effective_plan(),"desktop_app_beta_enabled":beta}});
    payload["evaluated_keys"] = json!({"userID":format!("user-{}",account.id),"customIDs":ids});
    payload["hash_used"] = "djb2".into();
    payload["has_updates"] = true.into();
    payload["time"] = chrono::Utc::now().timestamp_millis().into();
    let mut gates: std::collections::BTreeSet<String> = payload["feature_gates"]
        .as_object()
        .into_iter()
        .flat_map(|m| m.keys().cloned())
        .collect();
    gates.extend(
        codex2api_storage::client_fields("feature_bootstrap")
            .into_iter()
            .map(|f| f.path[1].clone()),
    );
    payload["live_entity_names"] = json!({"feature_gates":gates,"dynamic_configs":payload["dynamic_configs"].as_object().into_iter().flat_map(|m|m.keys()).collect::<Vec<_>>(),"experiments":[],"layer_configs":payload["layer_configs"].as_object().into_iter().flat_map(|m|m.keys()).collect::<Vec<_>>()});
    let snapshot = checksum(state, &payload, access, stable, beta).await?;
    payload["full_checksum"] = snapshot.clone().into();
    // The installed SDK seeds live cursors with derived_fields, but deliberately
    // drops full_checksum from that first cursor. Carry the same authenticated
    // snapshot through both protocol-defined echo fields.
    if !payload["derived_fields"].is_object() {
        payload["derived_fields"] = json!({});
    }
    payload["derived_fields"]["codex2api_snapshot"] = snapshot.into();
    Ok(payload)
}

pub(crate) async fn initialize(
    State(state): State<crate::ApiState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> crate::Result<Response> {
    let bytes = decoded(&body, &headers, uri.query())?;
    let request: Value = serde_json::from_slice(&bytes)
        .map_err(|_| crate::ApiError::bad_request("Invalid SDK initialization."))?;
    let (access, stable, mut beta) = if let Some(access) = optional_access(&state, &headers).await?
    {
        (
            access,
            request["user"]["customIDs"]["stableID"]
                .as_str()
                .map(str::to_owned),
            request["user"]["custom"]["desktop_app_beta_enabled"]
                .as_bool()
                .unwrap_or(false),
        )
    } else {
        let ticket = request["full_checksum"]
            .as_str()
            .or(request["previousDerivedFields"]["codex2api_snapshot"].as_str())
            .and_then(|s| s.split_once('.'))
            .ok_or_else(crate::ApiError::invalid_token)?;
        let signature = URL_SAFE_NO_PAD
            .decode(ticket.1)
            .map_err(|_| crate::ApiError::invalid_token())?;
        let secret = state.storage.oauth_signing_key().await?;
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(b"desktop-config:");
        mac.update(ticket.0.as_bytes());
        mac.verify_slice(&signature)
            .map_err(|_| crate::ApiError::invalid_token())?;
        let saved: Snapshot = serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(ticket.0)
                .map_err(|_| crate::ApiError::invalid_token())?,
        )
        .map_err(|_| crate::ApiError::invalid_token())?;
        if saved.expires < chrono::Utc::now().timestamp()
            || !state
                .storage
                .virtual_devices(&saved.owner)
                .await?
                .iter()
                .any(|d| d.id == saved.device)
        {
            return Err(crate::ApiError::invalid_token());
        }
        (
            VirtualAccess {
                scopes: "openid profile email".into(),
                provider_id: "chatgpt".into(),
                virtual_account_id: saved.owner,
                device_id: saved.device,
                account_id: None,
                name: String::new(),
                token_hash: String::new(),
            },
            saved.stable,
            saved.beta,
        )
    };
    if stable
        .as_ref()
        .is_some_and(|s| s.len() > 256 || s.chars().any(char::is_control))
    {
        return Err(crate::ApiError::bad_request("Invalid SDK stable identity."));
    }
    if headers
        .get("chatgpt-account-id")
        .is_some_and(|v| v.to_str().ok() != Some(access.virtual_account_id.as_str()))
    {
        return Err(crate::ApiError::invalid_token());
    }
    if let Some(value) = request["user"]["custom"].get("desktop_app_beta_enabled") {
        beta = value
            .as_bool()
            .ok_or_else(|| crate::ApiError::bad_request("Invalid desktop beta state."))?;
    }
    if request["user"]["userID"] != format!("user-{}", access.virtual_account_id)
        || request["user"]["customIDs"]["account_id"] != access.virtual_account_id
        || request["user"]["customIDs"]["stableID"].as_str() != stable.as_deref()
    {
        return Err(crate::ApiError::invalid_token());
    }
    if request.get("hash").is_some_and(|v| v != "djb2") {
        return Err(crate::ApiError::bad_request(
            "Unsupported SDK hash algorithm.",
        ));
    }
    let mut payload = bootstrap(&state, &access, beta, stable).await?;
    if request["responseMode"] == "live_overlay" {
        payload["response_mode"] = "live_overlay".into();
    }
    state
        .storage
        .record_virtual_request(
            &access.virtual_account_id,
            &access.device_id,
            "POST",
            "/api/oauth/chatgpt/v1/initialize",
            200,
            0,
        )
        .await?;
    Ok(json_response(payload))
}

pub(crate) fn public_resource_url(path: &str) -> Option<String> {
    if path == "/mcp-app.html" {
        return Some(format!("https://web-sandbox.oaiusercontent.com{path}"));
    }
    if path == "/codex-app-prod/windows-store-update.json" {
        return Some(format!("https://persistent.oaistatic.com{path}"));
    }
    let name = path.strip_prefix("/assets/")?;
    let stem = name
        .strip_suffix(".js")
        .or_else(|| name.strip_suffix(".css"))?;
    if stem
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
        && stem
            .match_indices('-')
            .any(|(i, _)| i > 0 && stem.len() - i > 8)
    {
        Some(format!("https://web-sandbox.oaiusercontent.com{path}"))
    } else {
        None
    }
}

pub(crate) async fn public_resource(
    State(state): State<crate::ApiState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> crate::Result<Response> {
    let path = uri
        .path()
        .strip_prefix(super::oauth::PREFIX)
        .unwrap_or(uri.path());
    let target = public_resource_url(path).ok_or_else(super::virtual_data::not_found)?;
    let settings = state.storage.desktop_support_settings().await?;
    if let Some(cached) = state.storage.desktop_resource(path).await?
        && chrono::Utc::now().timestamp_millis() - cached.fetched_at_ms
            < i64::from(settings.resource_cache_minutes) * 60_000
    {
        return resource_response(cached.content, cached.headers);
    }
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(20));
    if let Some(id) = settings.proxy_id {
        let proxy = state.storage.require_outbound_proxy(&id).await?;
        builder = builder.proxy(
            reqwest::Proxy::all(&proxy.url)
                .map_err(|e| crate::ApiError::internal(e.to_string()))?,
        );
    }
    let client = builder
        .build()
        .map_err(|e| crate::ApiError::internal(e.to_string()))?;
    let mut request = client.get(&target);
    for key in ["user-agent", "accept"] {
        if let Some(value) = headers.get(key) {
            request = request.header(key, value);
        }
    }
    let upstream = request.send().await.map_err(|_| {
        crate::ApiError::openai(
            StatusCode::BAD_GATEWAY,
            "upstream_error",
            "Could not load the Desktop public resource.",
            Some("desktop_resource_unavailable"),
        )
    })?;
    if !upstream.status().is_success() {
        return Err(crate::ApiError::openai(
            upstream.status(),
            "upstream_error",
            "The official public resource service rejected the request.",
            Some("desktop_resource_unavailable"),
        ));
    }
    let mut selected = json!({});
    for key in [
        "content-type",
        "content-security-policy",
        "permissions-policy",
        "cache-control",
        "etag",
        "last-modified",
        "access-control-allow-origin",
        "origin-agent-cluster",
        "x-content-type-options",
    ] {
        if let Some(value) = upstream.headers().get(key).and_then(|v| v.to_str().ok()) {
            selected[key] = value.into();
        }
    }
    use futures::StreamExt;
    let mut stream = upstream.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| crate::ApiError::internal("Resource transfer failed."))?;
        if bytes.len() + chunk.len() > 16 * 1024 * 1024 {
            return Err(crate::ApiError::internal(
                "Public resource exceeds size limit.",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    state
        .storage
        .save_desktop_resource(path, &bytes, &selected)
        .await?;
    resource_response(bytes, selected)
}
fn resource_response(bytes: Vec<u8>, headers: Value) -> crate::Result<Response> {
    let mut response = Response::new(Body::from(bytes));
    for (key, value) in headers.as_object().into_iter().flatten() {
        if let (Ok(key), Some(value)) = (key.parse::<axum::http::HeaderName>(), value.as_str())
            && let Ok(value) = value.parse()
        {
            response.headers_mut().insert(key, value);
        }
    }
    Ok(response)
}
