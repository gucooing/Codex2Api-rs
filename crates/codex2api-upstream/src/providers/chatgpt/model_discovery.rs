use crate::{Endpoint, Result, UpstreamClient, UpstreamError};
use serde_json::{Value, json};
impl UpstreamClient {
    pub async fn discover_supplier_models(&self) -> Result<(Vec<Value>, i64)> {
        let response = self
            .forward_endpoint(
                Endpoint::Models,
                bytes::Bytes::new(),
                http::HeaderMap::new(),
            )
            .await?;
        let revision = response
            .extensions()
            .get::<crate::SupplierAuthRevision>()
            .map(|r| r.0)
            .ok_or(UpstreamError::WorkspaceChanged)?;
        let status = response.status();
        let headers = response.headers().clone();
        if !status.is_success() {
            return Err(UpstreamError::status_with_headers(
                status,
                response.text().await?,
                headers,
            )
            .with_auth_revision(revision));
        }
        let value: Value = response.json().await?;
        let rows = value
            .get("models")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                UpstreamError::InvalidRequest("ChatGPT model list has no models array".into())
            })?;
        if rows.len() > 2048 {
            return Err(UpstreamError::InvalidRequest(
                "Model list is too large".into(),
            ));
        }
        let models = rows
            .iter()
            .map(|row| {
                let model = row["slug"]
                    .as_str()
                    .filter(|v| codex2api_core::valid_model(v))
                    .ok_or_else(|| {
                        UpstreamError::InvalidRequest("Invalid model identifier".into())
                    })?;
                Ok(json!({"model":model}))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((models, revision))
    }
}
