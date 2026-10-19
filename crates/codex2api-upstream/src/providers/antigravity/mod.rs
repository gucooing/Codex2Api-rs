//! Antigravity transport: the desktop client's v1internal envelope on the account's
//! frozen identity and Google OAuth. Shared code only ever receives Responses events.
use crate::{Result, SupplierAuthRevision, UpstreamError};
use bytes::Bytes;
use codex2api_common::version::antigravity as wire;
use codex2api_suppliers::antigravity::{
    AntigravityAuthError, AntigravityAuthService, AntigravityCredentials,
    transport::AntigravityHttpClients,
};
use http::HeaderValue;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

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

    /// A streamed generation; the reply is a Responses event stream or a normalized error.
    pub async fn generate(&self, id: &str, body: Value) -> Result<reqwest::Response> {
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
        let model = body["model"]
            .as_str()
            .ok_or_else(|| UpstreamError::InvalidRequest("model is required".into()))?
            .to_owned();
        let envelope = json!({"project":project,"requestId":format!("{}{}", wire::REQUEST_ID_PREFIX, uuid::Uuid::new_v4()),
            "userAgent":wire::ENVELOPE_USER_AGENT,"requestType":wire::REQUEST_TYPE_AGENT,"model":model,"request":body["request"]});
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
            // Quota is per model; an error that names none still concerns this generation's model.
            if normalized["error"].get("model").is_none() {
                normalized["error"]["model"] = model.as_str().into();
            }
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

    /// Replace the account's upstream models and register newly seen models in the
    /// billing catalog; a family with a known official price starts enabled.
    pub async fn discover_supplier_models(&self, id: &str) -> Result<Value> {
        let (authorized, raw) = self.fetch_models(id).await?;
        let mut descriptors = Vec::new();
        let mut supported = Vec::new();
        for (name, info) in raw["models"].as_object().expect("validated") {
            if !codex2api_common::valid_model(name) {
                return Err(UpstreamError::InvalidRequest(
                    "Antigravity returned an invalid model identifier".into(),
                ));
            }
            let mut descriptor = json!({"id":name,"model":name,"object":"model"});
            for (key, source) in [
                ("name", "displayName"),
                ("supports_thinking", "supportsThinking"),
                ("supports_images", "supportsImages"),
                ("max_output_tokens", "maxOutputTokens"),
                ("recommended", "recommended"),
            ] {
                if let Some(value) = info.get(source) {
                    descriptor[key] = value.clone();
                }
            }
            descriptors.push(descriptor);
            supported.push(codex2api_common::SupportedModel {
                provider_id: codex2api_common::ANTIGRAVITY.into(),
                model: name.clone(),
                kind: "text".into(),
                preset_key: info["displayName"]
                    .as_str()
                    .and_then(codex2api_common::providers::antigravity::family)
                    .or_else(|| codex2api_common::providers::antigravity::family(name)),
            });
        }
        let storage = self.auth.storage()?;
        if !storage
            .sync_supplier_models(id, authorized.revision, &descriptors)
            .await?
        {
            return Err(UpstreamError::WorkspaceChanged);
        }
        storage.sync_supported_models(&supported).await?;
        Ok(json!({"items":storage.supplier_models(id).await?}))
    }

    /// A fresh official quota read. Models reported empty cool down until their reset;
    /// models reported available recover immediately.
    pub async fn quota(&self, id: &str) -> Result<Value> {
        let (authorized, raw) = self.fetch_models(id).await?;
        let storage = self.auth.storage()?;
        let now = chrono::Utc::now().timestamp();
        for (name, info) in raw["models"].as_object().expect("validated") {
            let Some(remaining) = info["quotaInfo"]["remainingFraction"].as_f64() else {
                continue;
            };
            let reset = info["quotaInfo"]["resetTime"]
                .as_str()
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                .map(|t| t.timestamp())
                .filter(|t| *t > now);
            match reset {
                Some(until) if remaining <= 0.0 => {
                    storage
                        .exhaust_supplier_model_quota(id, authorized.revision, name, until, "quota")
                        .await?
                }
                _ if remaining > 0.0 => storage.recover_supplier_model_quota(id, name).await?,
                _ => {}
            }
        }
        Ok(raw)
    }
}
