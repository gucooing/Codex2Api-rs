//! Authenticated Grok Build model discovery. Never treat offline defaults as availability.
use super::GrokUpstream;
use crate::{Result, UpstreamError};
use serde_json::{Value, json};

pub fn descriptors(value: &Value) -> Result<Vec<Value>> {
    let rows = value["data"].as_array().ok_or_else(|| {
        UpstreamError::InvalidRequest("Grok /models must contain a data array".into())
    })?;
    if rows.len() > 2048 {
        return Err(UpstreamError::InvalidRequest(
            "Grok model catalog is too large".into(),
        ));
    }
    let mut result = std::collections::BTreeMap::new();
    for row in rows {
        let meta = &row["_meta"];
        let field = |keys: &[&str]| {
            keys.iter()
                .find_map(|key| row.get(*key).or_else(|| meta.get(*key)))
        };
        let model = field(&["model", "modelId", "id"])
            .and_then(Value::as_str)
            .filter(|v| codex2api_core::valid_model(v))
            .ok_or_else(|| {
                UpstreamError::InvalidRequest("Grok returned an invalid model identifier".into())
            })?;
        let mut descriptor = json!({"id":model,"model":model,"object":"model"});
        for (key, aliases) in [
            ("name", vec!["name"]),
            ("description", vec!["description"]),
            ("model_family", vec!["model_family", "modelFamily"]),
            (
                "context_window",
                vec!["context_window", "contextWindow", "totalContextTokens"],
            ),
            ("context_windows", vec!["context_windows", "contextWindows"]),
            ("api_backend", vec!["api_backend", "apiBackend"]),
            (
                "supports_backend_search",
                vec!["supports_backend_search", "supportsBackendSearch"],
            ),
            (
                "supports_reasoning_effort",
                vec!["supports_reasoning_effort", "supportsReasoningEffort"],
            ),
            (
                "reasoning_efforts",
                vec!["reasoning_efforts", "reasoningEfforts"],
            ),
            (
                "reasoning_effort",
                vec!["reasoning_effort", "reasoningEffort"],
            ),
            (
                "max_completion_tokens",
                vec!["max_completion_tokens", "maxCompletionTokens"],
            ),
            (
                "auto_compact_threshold_percent",
                vec![
                    "auto_compact_threshold_percent",
                    "autoCompactThresholdPercent",
                ],
            ),
            (
                "compaction_at_tokens",
                vec!["compaction_at_tokens", "compactionAtTokens"],
            ),
            (
                "compactions_remaining",
                vec!["compactions_remaining", "compactionsRemaining"],
            ),
        ] {
            if let Some(value) = field(&aliases) {
                descriptor[key] = value.clone();
            }
        }
        let backend = descriptor["api_backend"]
            .as_str()
            .unwrap_or("chat_completions")
            .to_owned();
        if !matches!(
            backend.as_str(),
            "responses" | "chat_completions" | "messages"
        ) {
            return Err(UpstreamError::InvalidRequest(
                "Grok returned an unsupported model protocol".into(),
            ));
        }
        descriptor["api_backend"] = backend.into();
        // Base URLs, headers, credentials and supplier-only metadata never enter the client catalog.
        result.insert(model.to_owned(), descriptor);
    }
    Ok(result.into_values().collect())
}
impl GrokUpstream {
    pub async fn sync_models(&self, id: &str) -> Result<Value> {
        self.auth().refresh_grok(id, false, None).await?;
        let revision = self
            .auth()
            .storage()?
            .supplier_auth_revision(id)
            .await?
            .ok_or_else(|| UpstreamError::MissingAccessToken(id.into()))?;
        let raw = self.grok_json(id, "models").await?;
        let models = descriptors(&raw)?;
        if !self
            .auth()
            .storage()?
            .save_grok_catalog(id, revision, &raw, &models)
            .await?
        {
            return Err(UpstreamError::WorkspaceChanged);
        }
        Ok(self.auth().storage()?.grok_catalog(id).await?)
    }
}
