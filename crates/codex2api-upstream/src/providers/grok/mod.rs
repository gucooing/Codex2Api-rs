//! Grok Build transport. No Codex request normalization or supplier routing headers.
use crate::{Result, SupplierAuthRevision, UpstreamError};
use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method};
use serde_json::{Value, json};

#[derive(Clone)]
pub struct GrokUpstream {
    auth: codex2api_auth::grok::GrokAuthService,
}

pub mod errors;
pub mod models;

/// Normalize a real Grok billing period for the shared administrator quota view.
/// Keep the original response, and leave absent measurements unknown.
pub fn quota_snapshot(mut value: Value) -> Result<Value> {
    let config = value
        .get("config")
        .filter(|v| v.is_object())
        .ok_or_else(|| {
            UpstreamError::InvalidRequest("Grok billing response has no config".into())
        })?;
    let date = |v: &Value| {
        v.as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|v| v.timestamp())
    };
    let start =
        date(&config["currentPeriod"]["start"]).or_else(|| date(&config["billingPeriodStart"]));
    let end = date(&config["currentPeriod"]["end"]).or_else(|| date(&config["billingPeriodEnd"]));
    let percent = config["creditUsagePercent"]
        .as_f64()
        .or_else(|| {
            let limit = config["monthlyLimit"]["val"].as_f64()?;
            let used = config["used"]["val"].as_f64()?;
            (limit > 0.0).then_some(used / limit * 100.0)
        })
        .filter(|v| v.is_finite() && *v >= 0.0);
    if percent.is_some() || start.is_some() || end.is_some() {
        value["rate_limit"] = json!({"primary_window":{"used_percent":percent,"reset_at":end,
            "limit_window_seconds":start.zip(end).and_then(|(s,e)| (e>s).then_some(e-s))}});
    }
    Ok(value)
}

impl GrokUpstream {
    pub fn new(auth: codex2api_auth::grok::GrokAuthService) -> Self {
        Self { auth }
    }
    pub fn auth(&self) -> &codex2api_auth::grok::GrokAuthService {
        &self.auth
    }
    pub async fn grok_request(
        &self,
        id: &str,
        method: Method,
        path: &str,
        body: Option<Value>,
        inbound: &HeaderMap,
    ) -> Result<reqwest::Response> {
        if !matches!(
            path,
            "responses"
                | "chat/completions"
                | "messages"
                | "models"
                | "user?include=subscription"
                | "settings"
                | "billing?format=credits"
        ) {
            return Err(UpstreamError::InvalidRequest(
                "Unsupported Grok upstream operation".into(),
            ));
        }
        let account = self.auth().storage()?.require_account(id).await?;
        if account.provider_id != codex2api_core::GROK {
            return Err(UpstreamError::InvalidRequest(
                "Supplier provider mismatch".into(),
            ));
        }
        self.auth().refresh_grok(id, false, None).await?;
        let http = self.auth().account_http(id).await?;
        let snapshot = self
            .auth()
            .storage()?
            .supplier_auth_snapshot(id)
            .await?
            .ok_or_else(|| UpstreamError::MissingAccessToken(id.into()))?;
        let access = snapshot
            .tokens
            .access_token
            .as_deref()
            .ok_or_else(|| UpstreamError::MissingAccessToken(id.into()))?;
        let mut headers = codex2api_auth::grok::headers(&http.identity, Some(access))?;
        if let Some(user) = &account.chatgpt_user_id {
            let user = HeaderValue::from_str(user)
                .map_err(|_| UpstreamError::InvalidRequest("Invalid Grok identity".into()))?;
            headers.insert("x-userid", user.clone());
            headers.insert("x-grok-user-id", user);
        }
        // Only request-scoped identifiers pass through. Never forward incoming credentials,
        // x-email, deployment identities, cookies, or supplier/account selectors.
        for name in [
            "x-grok-conv-id",
            "x-grok-req-id",
            "x-grok-session-id",
            "x-grok-agent-id",
            "x-grok-turn-idx",
            "x-grok-transient-retry",
            "x-grok-conv-group-id",
        ] {
            if let Some(value) = inbound.get(name).filter(|v| v.as_bytes().len() <= 256) {
                headers.insert(name, value.clone());
            }
        }
        if matches!(path, "responses" | "chat/completions" | "messages") {
            headers.entry("x-grok-req-id").or_insert_with(|| {
                HeaderValue::from_str(&uuid::Uuid::new_v4().to_string()).expect("UUID")
            });
            if let Some(model) = body.as_ref().and_then(|v| v["model"].as_str()) {
                headers.insert(
                    "x-grok-model-override",
                    HeaderValue::from_str(model)
                        .map_err(|_| UpstreamError::InvalidRequest("Invalid model".into()))?,
                );
            }
            if body.as_ref().is_some_and(|v| v["stream"] == true) {
                headers.insert("accept", HeaderValue::from_static("text/event-stream"));
            }
        }
        let url = format!(
            "{}/{}",
            self.auth().grok_config().base_url.trim_end_matches('/'),
            path
        );
        let mut request = http.routed_api.request(method, url).headers(headers);
        if let Some(body) = body {
            request = request.json(&body);
        }
        // Bound the wait for response headers while leaving long generations to the stream watchdog.
        let mut response =
            tokio::time::timeout(std::time::Duration::from_secs(120), request.send())
                .await
                .map_err(|_| UpstreamError::StreamIdleTimeout)??;
        response
            .extensions_mut()
            .insert(SupplierAuthRevision(snapshot.auth_revision));
        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let raw: Value = response.json().await.unwrap_or_default();
            let mut rebuilt = http::Response::builder()
                .status(status)
                .body(reqwest::Body::from(
                    json!({"error":errors::normalize(&raw)}).to_string(),
                ))
                .expect("response");
            *rebuilt.headers_mut() = headers;
            rebuilt.headers_mut().remove(http::header::CONTENT_LENGTH);
            rebuilt.headers_mut().remove(http::header::CONTENT_ENCODING);
            rebuilt.headers_mut().insert(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            rebuilt
                .extensions_mut()
                .insert(SupplierAuthRevision(snapshot.auth_revision));
            return Ok(reqwest::Response::from(rebuilt));
        }
        Ok(response)
    }

    pub async fn grok_json(&self, id: &str, path: &str) -> Result<Value> {
        let mut response = self
            .grok_request(id, Method::GET, path, None, &HeaderMap::new())
            .await?;
        if response.status() == http::StatusCode::UNAUTHORIZED {
            let revision = response.extensions().get::<SupplierAuthRevision>().copied();
            let snapshot = self
                .auth()
                .storage()?
                .supplier_auth_snapshot(id)
                .await?
                .ok_or_else(|| UpstreamError::MissingAccessToken(id.into()))?;
            if revision.is_some_and(|r| r.0 == snapshot.auth_revision) {
                self.auth()
                    .refresh_grok(
                        id,
                        true,
                        Some(snapshot.tokens.access_token.as_deref().unwrap_or("")),
                    )
                    .await?;
            }
            response = self
                .grok_request(id, Method::GET, path, None, &HeaderMap::new())
                .await?;
        }
        let status = response.status();
        if !status.is_success() {
            let revision = response.extensions().get::<SupplierAuthRevision>().copied();
            let headers = response.headers().clone();
            let error = UpstreamError::status_with_headers(status, response.text().await?, headers);
            if let Some(revision) = revision
                && matches!(
                    error
                        .supplier_failure_for(codex2api_core::GROK, chrono::Utc::now().timestamp()),
                    Some(crate::SupplierFailure::Authentication)
                )
            {
                self.auth()
                    .storage()?
                    .reject_supplier_auth(id, revision.0)
                    .await?;
            }
            return Err(error);
        }
        Ok(response.json().await?)
    }
}

pub fn prepare(body: &[u8], inbound: &HeaderMap) -> Result<Value> {
    let mut value = crate::decode_body(body, inbound)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| UpstreamError::InvalidRequest("Expected a JSON object".into()))?;
    if object
        .get("previous_response_id")
        .is_some_and(|v| !v.is_null())
    {
        return Err(UpstreamError::InvalidRequest("Grok requires full conversation input; supplier response references cannot cross virtual accounts".into()));
    }
    // Native Grok sends full input. Stored supplier conversations cannot be exposed to consumers.
    object.insert("store".into(), Value::Bool(false));
    for field in ["user", "user_id", "account_id", "deployment_id"] {
        object.remove(field);
    }
    Ok(value)
}

pub fn encoded(value: &Value) -> Result<Bytes> {
    Ok(Bytes::from(serde_json::to_vec(value)?))
}

/// The pinned Grok reader treats WKE as a machine-readable error tag. Ordinary 429s
/// and rate_limit_exceeded remain client backoff, never supplier exhaustion.
pub fn classify_failure(
    status: Option<u16>,
    value: &Value,
    headers: &HeaderMap,
    now: i64,
) -> Option<crate::SupplierFailure> {
    let error = value
        .pointer("/response/error")
        .or_else(|| value.get("error"))
        .unwrap_or(value);
    let status = status.or_else(|| {
        value["status"]
            .as_u64()
            .or_else(|| value["code"].as_u64())
            .and_then(|n| u16::try_from(n).ok())
    });
    if status == Some(401) {
        return Some(crate::SupplierFailure::Authentication);
    }
    let message = error
        .as_str()
        .or_else(|| error["message"].as_str())
        .unwrap_or("");
    let wke = message
        .split_once("[WKE=")
        .and_then(|(_, tail)| tail.split_once(']').map(|(tag, _)| tag));
    let code = wke
        .or_else(|| error["code"].as_str())
        .or_else(|| error["type"].as_str())?;
    if !matches!(
        code,
        "personal-team-blocked:spending-limit"
            | "insufficient_quota"
            | "insufficient_credits"
            | "billing_error"
    ) {
        return None;
    }
    let retry = headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|n| *n >= 0)
        .map(|n| now.saturating_add(n));
    let reset = error["reset_at"]
        .as_i64()
        .or_else(|| error["resets_at"].as_i64())
        .filter(|t| *t > now);
    Some(crate::SupplierFailure::QuotaExhausted {
        until: reset
            .or(retry)
            .unwrap_or(now.saturating_add(60))
            .max(now.saturating_add(1)),
        code: code.into(),
    })
}
