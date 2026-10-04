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

/// Administration, not the pinned metadata file, decides which models are offered.
/// Unknown slugs use the official ModelInfo wire fields without inventing a context limit.
pub fn configured_model_descriptor(model: &str) -> Value {
    let mut descriptor=codex_model_descriptor(model).unwrap_or_else(||serde_json::json!({
        "slug":model,"display_name":model,"description":null,"default_reasoning_level":null,
        "supported_reasoning_levels":[],"shell_type":"unified_exec","visibility":"list",
        "supported_in_api":true,"priority":99,"additional_speed_tiers":[],"service_tiers":[],
        "availability_nux":null,"upgrade":null,"support_verbosity":false,"default_verbosity":null,
        "apply_patch_tool_type":null,"truncation_policy":{"mode":"bytes","limit":10000},
        "experimental_supported_tools":[],"input_modalities":["text"],"supports_search_tool":false
    }));
    descriptor["visibility"] = "list".into();
    descriptor["supported_in_api"] = true.into();
    descriptor
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
