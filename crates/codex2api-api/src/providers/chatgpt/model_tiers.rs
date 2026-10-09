//! Service-owned tier availability comes from administrator model pricing.
use codex2api_storage::Storage;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(super) async fn ultrafast_models(storage: &Storage) -> crate::Result<BTreeSet<String>> {
    Ok(storage
        .model_prices(codex2api_core::CHATGPT)
        .await?
        .into_iter()
        .filter(|price| price.tier == "ultrafast" && price.min_input_tokens == 0)
        .map(|price| price.model)
        .collect())
}

pub(super) fn codex_descriptor(model: &str, ultrafast: &BTreeSet<String>) -> Value {
    let mut descriptor = codex2api_upstream::configured_model_descriptor(model);
    if ultrafast.contains(model) {
        descriptor["service_tiers"].as_array_mut().unwrap().push(json!({
            "id":"ultrafast", "name":"Ultrafast", "description":"Faster responses, increased usage"
        }));
    }
    descriptor
}

/// Desktop obtains selectable tiers from version presets, then intersects
/// per-model options with the tiers published by those presets.
pub(super) fn desktop_options(catalog: &mut Value, ultrafast: &BTreeSet<String>) {
    let update = |entry: &mut Value, slug_key: &str| {
        let enabled = entry[slug_key]
            .as_str()
            .is_some_and(|slug| ultrafast.contains(slug));
        let mut options = entry["service_tier_options"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        options.retain(|option| option["service_tier"] != "ultrafast");
        if enabled {
            options.push(json!({"service_tier":"ultrafast","title":"Ultrafast","subtitle":"Faster responses, increased usage"}));
        }
        entry["service_tier_options"] = options.into();
    };
    for model in catalog["models"].as_array_mut().into_iter().flatten() {
        update(model, "slug");
    }
    for version in catalog["versions"].as_array_mut().into_iter().flatten() {
        for preset in version["intelligence_presets"]
            .as_array_mut()
            .into_iter()
            .flatten()
        {
            update(preset, "model_slug");
        }
    }
}
