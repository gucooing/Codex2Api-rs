//! Antigravity transport: the desktop client's v1internal envelope on the account's
//! frozen identity and Google OAuth. Shared code only ever receives Responses events.
use crate::{Result, SupplierAuthRevision, UpstreamError};
use bytes::Bytes;
use codex2api_common::providers::antigravity as models;
use codex2api_common::version::antigravity as wire;
use codex2api_suppliers::antigravity::{
    AntigravityAuthError, AntigravityAuthService, AntigravityCredentials,
    transport::AntigravityHttpClients,
};
use http::HeaderValue;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

pub mod errors;
pub(crate) mod verdict;

#[derive(Clone)]
pub struct AntigravityUpstream {
    auth: AntigravityAuthService,
    transport: Arc<dyn crate::HttpTransport>,
}

struct Authorized {
    http: Arc<AntigravityHttpClients>,
    credentials: Arc<AntigravityCredentials>,
    revision: i64,
}

impl AntigravityUpstream {
    pub fn new(auth: AntigravityAuthService) -> Self {
        Self {
            auth,
            transport: Arc::new(crate::ReqwestTransport),
        }
    }
    pub fn with_transport(mut self, transport: Arc<dyn crate::HttpTransport>) -> Self {
        self.transport = transport;
        self
    }
    pub fn auth(&self) -> &AntigravityAuthService {
        &self.auth
    }

    async fn authorized(&self, id: &str) -> Result<Authorized> {
        let storage = self.auth.storage()?;
        if storage.supplier_account_snapshot(id).await?.provider_id != codex2api_common::ANTIGRAVITY
        {
            return Err(UpstreamError::Internal("Supplier provider mismatch".into()));
        }
        self.auth.refresh(id, false, None).await?;
        let snapshot = storage
            .supplier_auth_snapshot(id)
            .await?
            .ok_or_else(|| UpstreamError::MissingAccessToken(id.into()))?;
        let credentials =
            snapshot.decode(|| AntigravityCredentials::from_supplier_tokens(&snapshot.tokens))?;
        Ok(Authorized {
            http: self.auth.account_http(id).await?,
            credentials,
            revision: snapshot.auth_revision,
        })
    }

    /// One v1internal call on the account's service roots. A connection failure or an
    /// unroutable 404 moves to the next root; every other reply is the answer.
    async fn post(
        &self,
        authorized: &Authorized,
        path: &str,
        body: Bytes,
    ) -> Result<reqwest::Response> {
        let headers = codex2api_suppliers::antigravity::headers(
            &authorized.http.identity,
            Some(&authorized.credentials.access_token),
        )?;
        let roots: Vec<&str> = self
            .auth
            .config()
            .service_roots(authorized.credentials.paid())
            .collect();
        for (index, root) in roots.iter().enumerate() {
            let more = index + 1 < roots.len();
            let request = authorized
                .http
                .client
                .post(format!("{}{path}", root.trim_end_matches('/')))
                .headers(headers.clone())
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(body.clone());
            // Bound the wait for headers; generations stay with the stream watchdog.
            let response =
                tokio::time::timeout(Duration::from_secs(120), self.transport.send(request))
                    .await
                    .map_err(|_| UpstreamError::StreamIdleTimeout)?;
            match response {
                Ok(response) if response.status() == http::StatusCode::NOT_FOUND && more => {}
                Ok(response) => return Ok(response),
                Err(UpstreamError::Http(error)) if error.is_connect() && more => {}
                Err(error) => return Err(error),
            }
        }
        Err(UpstreamError::Internal(
            "Antigravity has no service root configured".into(),
        ))
    }

    /// The upstream ID serving a routed model on this account; see [`resolve_model`].
    async fn upstream_model(&self, id: &str, model: &str, effort: Option<&str>) -> Result<String> {
        let descriptor = self
            .auth
            .storage()?
            .supplier_model_descriptor(id, model)
            .await?;
        resolve_model(model, effort, descriptor.as_ref()).map_err(UpstreamError::InvalidRequest)
    }

    /// A streamed generation; the reply is a Responses event stream or a normalized error.
    pub async fn generate(&self, id: &str, body: Value) -> Result<reqwest::Response> {
        let model = body["model"]
            .as_str()
            .ok_or_else(|| UpstreamError::InvalidRequest("model is required".into()))?
            .to_owned();
        let upstream = self
            .upstream_model(id, &model, body["reasoning_effort"].as_str())
            .await?;
        let mut authorized = self.authorized(id).await?;
        if authorized.credentials.project_id.is_none() {
            self.auth.refresh_profile(id).await?;
            authorized = self.authorized(id).await?;
        }
        let project =
            authorized
                .credentials
                .project_id
                .clone()
                .ok_or(UpstreamError::AntigravityAuth(
                    AntigravityAuthError::MissingProject,
                ))?;
        let envelope = json!({"project":project,"requestId":format!("{}{}", wire::REQUEST_ID_PREFIX, uuid::Uuid::new_v4()),
            "userAgent":wire::ENVELOPE_USER_AGENT,"requestType":wire::REQUEST_TYPE_AGENT,"model":upstream,"request":body["request"]});
        let timing = crate::ResponseTiming::default();
        let response = self
            .post(
                &authorized,
                &format!("{}?alt=sse", wire::PATH_STREAM_GENERATE),
                Bytes::from(serde_json::to_vec(&envelope)?),
            )
            .await?;
        let revision = SupplierAuthRevision(authorized.revision);
        if !response.status().is_success() {
            let status = response.status();
            let original_headers = response.headers().clone();
            let mut normalized = errors::normalize(status.as_u16(), &response.bytes().await?);
            // Quota cools the routed model, whichever variant or vendor name Google reports.
            normalized["error"]["model"] = model.as_str().into();
            let mut rebuilt = http::Response::builder()
                .status(status)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(reqwest::Body::from(normalized.to_string()))
                .expect("response");
            for name in [
                "retry-after",
                "retry-after-ms",
                "x-should-retry",
                "x-request-id",
                "x-oai-request-id",
            ] {
                if let Some(value) = original_headers.get(name) {
                    rebuilt.headers_mut().insert(name, value.clone());
                }
            }
            if let Some(retry) = normalized["headers"]["retry-after"]
                .as_str()
                .and_then(|v| HeaderValue::from_str(v).ok())
            {
                rebuilt
                    .headers_mut()
                    .entry(http::header::RETRY_AFTER)
                    .or_insert(retry);
            }
            rebuilt.extensions_mut().insert(revision);
            return Ok(reqwest::Response::from(rebuilt));
        }
        let mut response = timing.wrap(response, crate::TokenProtocol::Gemini);
        response.extensions_mut().insert(revision);
        Ok(response)
    }

    /// The account's official model catalog with per-model quota, as returned.
    async fn fetch_models(&self, id: &str) -> Result<(Authorized, Value)> {
        let authorized = self.authorized(id).await?;
        let project =
            authorized
                .credentials
                .project_id
                .clone()
                .ok_or(UpstreamError::AntigravityAuth(
                    AntigravityAuthError::MissingProject,
                ))?;
        let response = self
            .post(
                &authorized,
                wire::PATH_FETCH_MODELS,
                Bytes::from(json!({"project":project}).to_string()),
            )
            .await?;
        let status = response.status();
        if !status.is_success() {
            let normalized = errors::normalize(status.as_u16(), &response.bytes().await?);
            return Err(UpstreamError::status(status, normalized.to_string())
                .with_auth_revision(authorized.revision));
        }
        let raw: Value = response.json().await?;
        if raw["models"].as_object().is_none_or(|m| m.len() > 2048) {
            return Err(UpstreamError::InvalidRequest(
                "Antigravity returned an invalid model catalog".into(),
            ));
        }
        Ok((authorized, raw))
    }

    /// Replace the account's upstream models with Antigravity's models: upstream IDs
    /// are grouped by display name, and each model's descriptor records the ID that
    /// serves each reasoning effort and the ID for requests without one. Discovery
    /// changes supplier availability only, never the billing catalog.
    pub async fn discover_supplier_models(&self, id: &str) -> Result<Value> {
        let (authorized, raw) = self.fetch_models(id).await?;
        let catalog = raw["models"].as_object().expect("validated");
        if !catalog
            .keys()
            .all(|model| codex2api_common::valid_model(model))
        {
            return Err(UpstreamError::InvalidRequest(
                "Antigravity returned an invalid model identifier".into(),
            ));
        }
        let descriptors: Vec<Value> = models::model_groups(catalog)
            .into_iter()
            .map(|group| {
                let mut descriptor = json!({"id":group.model,"model":group.model,"object":"model",
                    "name":group.name,"efforts":group.efforts});
                if let Some(upstream) = group.upstream_model {
                    descriptor["upstream_model"] = upstream.into();
                }
                if let Some(pool) = group.quota_pool {
                    descriptor["quota_pool"] = pool.into();
                }
                descriptor
            })
            .collect();
        let storage = self.auth.storage()?;
        if !storage
            .sync_supplier_models(id, authorized.revision, &descriptors)
            .await?
        {
            return Err(UpstreamError::WorkspaceChanged);
        }
        Ok(json!({"items":storage.supplier_models(id).await?}))
    }

    /// A fresh official quota read, applied to the account's models: every model of a
    /// spent quota cools down until its reset, and every model of a quota with some
    /// left recovers.
    pub async fn quota(&self, id: &str) -> Result<Value> {
        let storage = self.auth.storage()?;
        let observation = storage.supplier_health(id).await?.observation_revision;
        let (authorized, raw) = self.fetch_models(id).await?;
        let now = chrono::Utc::now().timestamp();
        let quotas = model_quotas(&raw);
        // Apply recoveries before this response's new exhaustions advance the
        // observation revision. Concurrent failures invalidate every recovery.
        for quota in &quotas {
            if quota
                .remaining_fraction
                .is_some_and(|remaining| remaining > 0.0)
            {
                for model in &quota.models {
                    storage
                        .unblock_supplier_model(id, authorized.revision, model, observation)
                        .await?;
                }
            }
        }
        for quota in quotas {
            for model in &quota.models {
                match (quota.remaining_fraction, quota.reset_at) {
                    (Some(remaining), Some(until)) if remaining <= 0.0 && until > now => {
                        storage
                            .block_supplier(
                                id,
                                authorized.revision,
                                &codex2api_storage::BlockRequest {
                                    model: Some(model.clone()),
                                    kind: codex2api_storage::BlockKind::Quota,
                                    until_at: Some(until),
                                    code: Some("quota".into()),
                                    message: None,
                                },
                            )
                            .await?;
                    }
                    _ => {}
                }
            }
        }
        Ok(raw)
    }
}

/// One quota of an official catalog read: a model with its own quota, or the models
/// sharing a quota pool.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ModelQuota {
    /// The quota pool, or the model that has its own quota.
    pub key: String,
    /// Display names of the models drawing from the quota.
    pub name: String,
    /// Client-facing names of those models.
    pub models: Vec<String>,
    pub upstream_models: Vec<String>,
    /// Lowest remaining fraction any of the upstream IDs reports.
    pub remaining_fraction: Option<f64>,
    /// Earliest reset among them, as a Unix timestamp.
    pub reset_at: Option<i64>,
}

/// Antigravity reports quota under every upstream ID, while the quota belongs to the
/// model or to its pool: each quota takes the lowest fraction and earliest reset any of
/// its upstream IDs report.
pub fn model_quotas(raw: &Value) -> Vec<ModelQuota> {
    let Some(catalog) = raw["models"].as_object() else {
        return Vec::new();
    };
    let mut quotas = std::collections::BTreeMap::<String, (Vec<String>, ModelQuota)>::new();
    for group in models::model_groups(catalog) {
        let key = group
            .quota_pool
            .map_or_else(|| group.model.clone(), str::to_owned);
        let (names, quota) = quotas.entry(key.clone()).or_insert_with(|| {
            let quota = ModelQuota {
                key,
                name: String::new(),
                models: Vec::new(),
                upstream_models: Vec::new(),
                remaining_fraction: None,
                reset_at: None,
            };
            (Vec::new(), quota)
        });
        for info in group
            .upstream_models
            .iter()
            .filter_map(|id| catalog.get(id))
        {
            if let Some(fraction) = models::remaining_fraction(info) {
                quota.remaining_fraction = Some(
                    quota
                        .remaining_fraction
                        .map_or(fraction, |current| current.min(fraction)),
                );
            }
            if let Some(at) = info["quotaInfo"]["resetTime"]
                .as_str()
                .and_then(|time| chrono::DateTime::parse_from_rfc3339(time).ok())
                .map(|time| time.timestamp())
            {
                quota.reset_at = Some(quota.reset_at.map_or(at, |current| current.min(at)));
            }
        }
        names.push(group.name);
        quota.models.push(group.model);
        quota.upstream_models.extend(group.upstream_models);
    }
    quotas
        .into_values()
        .map(|(names, mut quota)| {
            quota.name = names.join(" · ");
            quota
        })
        .collect()
}

/// The upstream ID for a routed `model` from the descriptor discovery stored. A model
/// with reasoning-effort variants accepts exactly the efforts Antigravity lists, or no
/// effort when it lists an ID without one; every other combination is refused before
/// it reaches Antigravity. A model without effort variants sends its upstream ID, or
/// its own name when it has no descriptor, and keeps the effort as a thinking budget.
fn resolve_model(
    model: &str,
    effort: Option<&str>,
    descriptor: Option<&Value>,
) -> std::result::Result<String, String> {
    let descriptor = descriptor.unwrap_or(&Value::Null);
    let upstream = descriptor["upstream_model"].as_str();
    let Some(efforts) = descriptor["efforts"]
        .as_object()
        .filter(|efforts| !efforts.is_empty())
    else {
        return Ok(upstream.unwrap_or(model).to_owned());
    };
    let selected = match effort {
        Some(effort) => efforts.get(effort).and_then(Value::as_str),
        None => upstream,
    };
    if let Some(selected) = selected {
        return Ok(selected.to_owned());
    }
    let supported = models::EFFORTS
        .into_iter()
        .filter(|effort| efforts.contains_key(*effort))
        .collect::<Vec<_>>()
        .join(", ");
    Err(match effort {
        Some(effort) => format!(
            "Model {model} does not support reasoning effort '{effort}'; supported values: {supported}"
        ),
        None => format!("Model {model} requires a reasoning effort; supported values: {supported}"),
    })
}

#[cfg(test)]
mod tests {
    use super::{model_quotas, resolve_model};
    use serde_json::json;

    #[test]
    fn effort_variants_forward_only_the_exact_requested_effort() {
        let flash = json!({"efforts":{"high":"gemini-3-flash-agent",
            "low":"gemini-3.5-flash-extra-low","medium":"gemini-3.5-flash-low"}});
        assert_eq!(
            resolve_model("gemini-3.5-flash", Some("high"), Some(&flash)).unwrap(),
            "gemini-3-flash-agent"
        );
        assert_eq!(
            resolve_model("gemini-3.5-flash", Some("medium"), Some(&flash)).unwrap(),
            "gemini-3.5-flash-low"
        );
        let refused = resolve_model("gemini-3.5-flash", Some("xhigh"), Some(&flash)).unwrap_err();
        assert!(
            refused.contains("'xhigh'") && refused.contains("low, medium, high"),
            "{refused}"
        );
        for effort in [None, Some("none"), Some("minimal")] {
            assert!(resolve_model("gemini-3.5-flash", effort, Some(&flash)).is_err());
        }
        // An ID without an effort label serves requests that name no effort.
        let tiered =
            json!({"efforts":{"high":"gemini-3.6-flash-high"},"upstream_model":"gemini-3.6-flash"});
        assert_eq!(
            resolve_model("gemini-3.6-flash", None, Some(&tiered)).unwrap(),
            "gemini-3.6-flash"
        );
        assert!(resolve_model("gemini-3.6-flash", Some("low"), Some(&tiered)).is_err());
        // Without effort variants the chosen alias is sent and the effort stays a budget.
        let lite = json!({"efforts":{},"upstream_model":"gemini-3.5-flash-lite"});
        assert_eq!(
            resolve_model("gemini-3.5-flash-lite", Some("high"), Some(&lite)).unwrap(),
            "gemini-3.5-flash-lite"
        );
        assert_eq!(
            resolve_model("custom-model", Some("high"), None).unwrap(),
            "custom-model"
        );
    }

    #[test]
    fn quota_belongs_to_the_model_or_its_pool_not_its_upstream_ids() {
        let raw = json!({"models":{
            "gemini-3-flash-agent":{"displayName":"Gemini 3.5 Flash (High)",
                "quotaInfo":{"resetTime":"2026-10-20T05:00:00Z"}},
            "gemini-3.5-flash-low":{"displayName":"Gemini 3.5 Flash (Medium)",
                "quotaInfo":{"remainingFraction":0.4,"resetTime":"2026-10-20T06:00:00Z"}},
            "claude-opus-5-5-high":{"displayName":"Claude Opus 5.5 (High)",
                "modelProvider":"MODEL_PROVIDER_ANTHROPIC","quotaInfo":{"remainingFraction":0.5}},
            "gpt-oss-120b-medium":{"displayName":"GPT-OSS 120B (Medium)",
                "modelProvider":"MODEL_PROVIDER_OPENAI","quotaInfo":{"remainingFraction":0.75}}
        }});
        let quotas = model_quotas(&raw);
        assert_eq!(quotas.len(), 2);
        let flash = quotas
            .iter()
            .find(|quota| quota.key == "gemini-3.5-flash")
            .unwrap();
        assert_eq!(flash.models, ["gemini-3.5-flash"]);
        assert_eq!(flash.remaining_fraction, Some(0.0));
        assert_eq!(
            flash.reset_at,
            Some(
                chrono::DateTime::parse_from_rfc3339("2026-10-20T05:00:00Z")
                    .unwrap()
                    .timestamp()
            )
        );
        assert_eq!(flash.upstream_models.len(), 2);
        let pool = quotas
            .iter()
            .find(|quota| quota.key == codex2api_common::providers::antigravity::THIRD_PARTY_POOL)
            .unwrap();
        assert_eq!(pool.models, ["claude-opus-5.5", "gpt-oss-120b"]);
        assert_eq!(pool.name, "Claude Opus 5.5 · GPT-OSS 120B");
        assert_eq!((pool.remaining_fraction, pool.reset_at), (Some(0.5), None));
    }
}
