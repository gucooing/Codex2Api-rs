//! ChatGPT protocol descriptors copied from the pinned official Codex snapshot.
//! Source: codex-rs/models-manager/models.json at CODEX_REF_COMMIT.
//! This data describes client capabilities; it does not grant model access.
use serde_json::Value;
use std::sync::OnceLock;

pub fn codex_model_descriptor(model: &str) -> Option<Value> {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    let catalog = CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("chatgpt-models.json"))
            .expect("pinned Codex model descriptors")
    });
    catalog["models"]
        .as_array()?
        .iter()
        .find(|item| item["slug"] == model)
        .cloned()
}
