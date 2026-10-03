//! ChatGPT protocol descriptors copied from the pinned official Codex snapshot.
//! Source: codex-rs/models-manager/models.json at CODEX_REF_COMMIT.
//! This data describes client capabilities; it does not grant model access.
use serde_json::Value;
use std::sync::OnceLock;

fn catalog() -> &'static Value {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("chatgpt-models.json"))
            .expect("pinned Codex model descriptors")
    })
}

pub fn codex_model_descriptor(model: &str) -> Option<Value> {
    catalog()["models"]
        .as_array()?
        .iter()
        .find(|item| item["slug"] == model)
        .cloned()
}

/// Public models actually described by the pinned provider adapter.
/// Internal/hidden aliases do not become public service entitlements.
pub fn supported_models() -> Vec<codex2api_core::SupportedModel> {
    catalog()["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|model| model["visibility"] == "list" && model["supported_in_api"] == true)
        .filter_map(|model| model["slug"].as_str())
        .map(|model| codex2api_core::SupportedModel {
            provider_id: codex2api_core::CHATGPT.into(),
            model: model.into(),
            kind: "text".into(),
        })
        .collect()
}
