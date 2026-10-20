//! The API-key channel's transport: one request per attempt on the account's isolated
//! client, the key in the configured header, the fixed User-Agent, and the account's
//! wire deciding the endpoint under its Base URL. Model discovery reads the vendor's
//! `/models` listing with the same key.
use crate::{
    Result, SupplierAuthRevision, UpstreamError,
    supply::{Wire, WireRequest},
};
use codex2api_common::version::api as version;
use codex2api_suppliers::api::{ApiAuthService, ApiCredentials};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

pub(crate) mod verdict;

#[derive(Clone)]
pub struct ApiUpstream {
    auth: ApiAuthService,
    transport: Arc<dyn crate::HttpTransport>,
}

impl ApiUpstream {
    pub fn new(auth: ApiAuthService) -> Self {
        Self {
            auth,
            transport: Arc::new(crate::ReqwestTransport),
        }
    }
    pub fn with_transport(mut self, transport: Arc<dyn crate::HttpTransport>) -> Self {
        self.transport = transport;
        self
    }
    pub fn auth(&self) -> &ApiAuthService {
        &self.auth
    }

    /// The account's credentials, decoded once per credential snapshot.
    pub async fn credentials(&self, id: &str) -> Result<(Arc<ApiCredentials>, i64)> {
        Ok(self.auth.credentials(id).await?)
    }

    /// The wire the account's endpoint speaks.
    pub async fn wire(&self, id: &str) -> Result<Wire> {
        let (credentials, _) = self.credentials(id).await?;
        Wire::parse(&credentials.wire)
            .ok_or_else(|| UpstreamError::Internal("API supplier wire is invalid".into()))
    }

    async fn post(
        &self,
        credentials: &ApiCredentials,
        http: &codex2api_suppliers::api::ApiHttpClient,
        path: &str,
        body: Option<&Value>,
        stream: bool,
    ) -> Result<reqwest::Response> {
        let headers = codex2api_suppliers::api::headers(credentials, stream)?;
        let url = credentials.endpoint(path);
        let request = match body {
            Some(body) => http
                .client
                .post(url)
                .headers(headers)
                .body(serde_json::to_vec(body)?),
            None => http.client.get(url).headers(headers),
        };
        // Bound the wait for headers; generations stay with the stream watchdog.
        tokio::time::timeout(Duration::from_secs(120), self.transport.send(request))
            .await
            .map_err(|_| UpstreamError::StreamIdleTimeout)?
    }

    /// One generation in the account's wire; the reply is the vendor's own stream.
    pub async fn send(&self, id: &str, request: WireRequest) -> Result<reqwest::Response> {
        let (credentials, revision) = self.credentials(id).await?;
        if Wire::parse(&credentials.wire) != Some(request.wire) {
            return Err(UpstreamError::Internal(
                "API supplier wire does not match the converted request".into(),
            ));
        }
        let http = self.auth.account_http(id).await?;
        let protocol = match request.wire {
            Wire::ChatCompletions => crate::TokenProtocol::ChatCompletions,
            Wire::Messages => crate::TokenProtocol::Messages,
            _ => crate::TokenProtocol::Responses,
        };
        let timing = crate::ResponseTiming::default();
        let response = self
            .post(
                &credentials,
                &http,
                credentials.wire_path(),
                Some(&request.body),
                request.stream,
            )
            .await?;
        let mut response = timing.wrap(response, protocol);
        response
            .extensions_mut()
            .insert(SupplierAuthRevision(revision));
        Ok(response)
    }

    /// The vendor's model listing for a key; every vendor's listing is a `data` array
    /// of objects with an `id`.
    async fn read_models(
        &self,
        credentials: &ApiCredentials,
        http: &codex2api_suppliers::api::ApiHttpClient,
        revision: Option<i64>,
    ) -> Result<Vec<Value>> {
        let response = self
            .post(credentials, http, version::MODELS_PATH, None, false)
            .await?;
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.bytes().await?;
        if !status.is_success() {
            let error = UpstreamError::status_with_headers(
                status,
                String::from_utf8_lossy(&bytes).into_owned(),
                headers,
            );
            return Err(match revision {
                Some(revision) => error.with_auth_revision(revision),
                None => error,
            });
        }
        descriptors(&serde_json::from_slice(&bytes)?)
    }

    /// The model identifiers an endpoint lists for a key before any account exists, read
    /// on a client used only for this listing. Nothing is stored.
    pub async fn list_models(
        &self,
        credentials: &ApiCredentials,
        proxy: Option<&str>,
    ) -> Result<Vec<String>> {
        let http = codex2api_suppliers::api::ApiHttpClient::with_proxy(proxy)?;
        Ok(self
            .read_models(credentials, &http, None)
            .await?
            .into_iter()
            .filter_map(|model| model["model"].as_str().map(str::to_owned))
            .collect())
    }

    /// Replace the account's upstream models with the vendor's current listing.
    pub async fn discover_supplier_models(&self, id: &str) -> Result<Value> {
        let (credentials, revision) = self.credentials(id).await?;
        let http = self.auth.account_http(id).await?;
        let models = self
            .read_models(&credentials, &http, Some(revision))
            .await?;
        let storage = self.auth.storage()?;
        if !storage.sync_supplier_models(id, revision, &models).await? {
            return Err(UpstreamError::WorkspaceChanged);
        }
        Ok(json!({"items":storage.supplier_models(id).await?}))
    }
}

/// Catalog rows as account-local descriptors; Base URLs and keys never enter them.
pub fn descriptors(value: &Value) -> Result<Vec<Value>> {
    let rows = value["data"]
        .as_array()
        .or_else(|| value["models"].as_array())
        .ok_or_else(|| {
            UpstreamError::InvalidRequest("The model listing must contain a data array".into())
        })?;
    if rows.len() > 2048 {
        return Err(UpstreamError::InvalidRequest(
            "The model listing is too large".into(),
        ));
    }
    let mut result = std::collections::BTreeMap::new();
    for row in rows {
        let Some(model) = row["id"]
            .as_str()
            .or_else(|| {
                row["name"]
                    .as_str()
                    .map(|name| name.strip_prefix("models/").unwrap_or(name))
            })
            .filter(|v| codex2api_common::valid_model(v))
        else {
            continue;
        };
        let mut descriptor = json!({"id":model,"model":model,"object":"model"});
        for key in ["display_name", "owned_by", "created", "created_at"] {
            if let Some(value) = row.get(key).filter(|v| !v.is_null()) {
                descriptor[key] = value.clone();
            }
        }
        result.insert(model.to_owned(), descriptor);
    }
    Ok(result.into_values().collect())
}
