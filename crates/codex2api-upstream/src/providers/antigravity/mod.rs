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
use std::{collections::BTreeMap, sync::Arc, time::Duration};

pub(crate) mod api;
pub mod errors;
mod events;
mod request;
mod schema;
pub use errors::classify_failure;

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
            return Err(UpstreamError::InvalidRequest(
                "Supplier provider mismatch".into(),
            ));
        }
        let credentials = self.auth.refresh(id, false, None).await?;
        let revision = storage
            .supplier_auth_revision(id)
            .await?
            .ok_or_else(|| UpstreamError::MissingAccessToken(id.into()))?;
        Ok(Authorized {
            http: self.auth.account_http(id).await?,
            credentials,
            revision,
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
        Err(UpstreamError::InvalidRequest(
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
        let variants: Vec<&str> = descriptor
            .as_ref()
            .and_then(|descriptor| descriptor["upstream_models"].as_array())
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        resolve_model(model, effort, &variants).map_err(UpstreamError::InvalidRequest)
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
            let mut normalized = errors::normalize(status.as_u16(), &response.bytes().await?);
            // Quota cools the routed model, whichever variant or vendor name Google reports.
            normalized["error"]["model"] = model.as_str().into();
            let mut rebuilt = http::Response::builder()
                .status(status)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(reqwest::Body::from(normalized.to_string()))
                .expect("response");
            if let Some(retry) = normalized["headers"]["retry-after"]
                .as_str()
                .and_then(|v| HeaderValue::from_str(v).ok())
            {
                rebuilt
                    .headers_mut()
                    .insert(http::header::RETRY_AFTER, retry);
            }
            rebuilt.extensions_mut().insert(revision);
            return Ok(reqwest::Response::from(rebuilt));
        }
        let mut converted = events::responses_stream(response, model);
        converted.extensions_mut().insert(revision);
        Ok(timing.wrap(converted, crate::TokenProtocol::Responses))
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

    /// Replace the account's upstream models. Reasoning-effort variants collapse into
    /// their base model, whose descriptor keeps the upstream IDs a request can reach.
    /// Discovery changes supplier availability only, never the billing catalog.
    pub async fn discover_supplier_models(&self, id: &str) -> Result<Value> {
        let (authorized, raw) = self.fetch_models(id).await?;
        let mut variants = BTreeMap::<&str, Vec<&str>>::new();
        for name in raw["models"].as_object().expect("validated").keys() {
            if !codex2api_common::valid_model(name) {
                return Err(UpstreamError::InvalidRequest(
                    "Antigravity returned an invalid model identifier".into(),
                ));
            }
            variants
                .entry(models::effort_variant(name).0)
                .or_default()
                .push(name);
        }
        let descriptors: Vec<Value> = variants
            .into_iter()
            .map(|(model, upstream)| {
                json!({"id":model,"model":model,"object":"model","upstream_models":upstream})
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

    /// A fresh official quota read, applied to the account's models. A model cools
    /// down until its earliest reset once every variant is reported empty, and
    /// recovers as soon as any variant has quota left.
    pub async fn quota(&self, id: &str) -> Result<Value> {
        let (authorized, raw) = self.fetch_models(id).await?;
        let storage = self.auth.storage()?;
        let now = chrono::Utc::now().timestamp();
        let mut quotas = BTreeMap::<&str, (bool, Option<i64>)>::new();
        for (name, info) in raw["models"].as_object().expect("validated") {
            let Some(remaining) = info["quotaInfo"]["remainingFraction"].as_f64() else {
                continue;
            };
            let reset = info["quotaInfo"]["resetTime"]
                .as_str()
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                .map(|t| t.timestamp())
                .filter(|t| *t > now);
            let (available, earliest) = quotas.entry(models::effort_variant(name).0).or_default();
            if remaining > 0.0 {
                *available = true;
            } else if let Some(reset) = reset {
                *earliest = Some(earliest.map_or(reset, |current| current.min(reset)));
            }
        }
        for (model, (available, reset)) in quotas {
            if available {
                storage.recover_supplier_model_quota(id, model).await?;
            } else if let Some(until) = reset {
                storage
                    .exhaust_supplier_model_quota(id, authorized.revision, model, until, "quota")
                    .await?;
            }
        }
        Ok(raw)
    }
}

/// The upstream ID for a routed `model` among the upstream IDs discovery grouped
/// under it. A model with reasoning-effort variants accepts exactly the variant its
/// effort's suffix names, or its own ID when the request names no effort and the
/// account lists it; every other combination is refused before it reaches
/// Antigravity. A model without effort variants is sent as named.
fn resolve_model(
    model: &str,
    effort: Option<&str>,
    variants: &[&str],
) -> std::result::Result<String, String> {
    let efforts: Vec<&str> = models::EFFORTS
        .into_iter()
        .filter(|effort| {
            variants
                .iter()
                .any(|variant| models::effort_variant(variant) == (model, Some(*effort)))
        })
        .collect();
    if efforts.is_empty() {
        return Ok(model.to_owned());
    }
    let requested = match effort {
        Some(effort) => models::effort_model(model, effort),
        None => Some(model.to_owned()),
    };
    if let Some(upstream) = requested.filter(|upstream| variants.contains(&upstream.as_str())) {
        return Ok(upstream);
    }
    let supported = efforts.join(", ");
    Err(match effort {
        Some(effort) => format!(
            "Model {model} does not support reasoning effort '{effort}'; supported values: {supported}"
        ),
        None => format!("Model {model} requires a reasoning effort; supported values: {supported}"),
    })
}

#[cfg(test)]
mod tests {
    use super::resolve_model;

    #[test]
    fn effort_variants_forward_only_the_exact_requested_effort() {
        let opus = [
            "claude-opus-5-5-high",
            "claude-opus-5-5-low",
            "claude-opus-5-5-medium",
        ];
        assert_eq!(
            resolve_model("claude-opus-5-5", Some("high"), &opus).unwrap(),
            "claude-opus-5-5-high"
        );
        let refused = resolve_model("claude-opus-5-5", Some("xhigh"), &opus).unwrap_err();
        assert!(refused.contains("'xhigh'") && refused.contains("low, medium, high"));
        for effort in [None, Some("none"), Some("minimal")] {
            assert!(resolve_model("claude-opus-5-5", effort, &opus).is_err());
        }
        let flash = ["gemini-3.5-flash-extra-low", "gemini-3.5-flash-low"];
        assert_eq!(
            resolve_model("gemini-3.5-flash", Some("minimal"), &flash).unwrap(),
            "gemini-3.5-flash-extra-low"
        );
        // A base ID Antigravity lists serves requests without an effort.
        let tiered = ["gemini-3.6-flash", "gemini-3.6-flash-high"];
        assert_eq!(
            resolve_model("gemini-3.6-flash", None, &tiered).unwrap(),
            "gemini-3.6-flash"
        );
        assert!(resolve_model("gemini-3.6-flash", Some("low"), &tiered).is_err());
        // Models without effort variants keep the effort as a thinking budget.
        for variants in [&["gemini-2.5-pro"][..], &[][..]] {
            assert_eq!(
                resolve_model("gemini-2.5-pro", Some("high"), variants).unwrap(),
                "gemini-2.5-pro"
            );
        }
    }
}
