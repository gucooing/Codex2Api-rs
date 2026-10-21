//! Antigravity (Google Cloud Code Assist) protocol constants and the client's model
//! naming. Prices live only in the administrator's billing catalog.
pub mod version;

use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Reasoning-effort labels Antigravity appends to a display name, as in
/// "Gemini 3.5 Flash (High)"; each one is the public effort of the same name. Upstream
/// IDs carry no reliable effort: "Gemini 3.5 Flash (High)" is `gemini-3-flash-agent`.
pub const EFFORTS: [&str; 3] = ["low", "medium", "high"];

/// Quota pool of Antigravity's third-party models: every Anthropic and OpenAI model
/// draws from one shared quota, while each Google model keeps its own.
pub const THIRD_PARTY_POOL: &str = "anthropic-openai";
const THIRD_PARTY_PROVIDERS: [&str; 2] = ["MODEL_PROVIDER_ANTHROPIC", "MODEL_PROVIDER_OPENAI"];

/// One model as Antigravity presents it: every upstream ID whose display name reads the
/// same once a trailing effort label is removed, alias IDs included.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelGroup {
    /// Client-facing model name derived from the display name.
    pub model: String,
    /// Display name without its effort label.
    pub name: String,
    /// Upstream ID serving each public reasoning effort.
    pub efforts: BTreeMap<String, String>,
    /// Upstream ID serving requests that name no effort.
    pub upstream_model: Option<String>,
    /// Every upstream ID of the model, in order.
    pub upstream_models: Vec<String>,
    /// The quota pool the model shares with other models, if any.
    pub quota_pool: Option<&'static str>,
}

/// A display name without its trailing effort label, and the effort the label names.
pub fn display_effort(display: &str) -> (&str, Option<&'static str>) {
    let display = display.trim();
    if let Some(inner) = display.strip_suffix(')')
        && let Some(open) = inner.rfind('(')
        && let Some(effort) = EFFORTS
            .into_iter()
            .find(|effort| effort.eq_ignore_ascii_case(inner[open + 1..].trim()))
    {
        let name = inner[..open].trim_end();
        if !name.is_empty() {
            return (name, Some(effort));
        }
    }
    (display, None)
}

/// The client-facing name of a display name: lowercase, runs of spaces and hyphens as
/// one hyphen, other punctuation dropped ("GPT-OSS 120B" is `gpt-oss-120b`).
pub fn model_name(display: &str) -> String {
    let mut name = String::with_capacity(display.len());
    for c in display.chars() {
        if c.is_ascii_whitespace() || c == '-' {
            if !name.is_empty() && !name.ends_with('-') {
                name.push('-');
            }
        } else if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
            name.push(c.to_ascii_lowercase());
        }
    }
    if name.ends_with('-') {
        name.pop();
    }
    name
}

/// Whether `id` should replace `current` for one label: the ID equal to the preferred
/// name wins, otherwise the first in order.
fn preferred(current: Option<&String>, id: &str, wanted: &str) -> bool {
    current.is_none_or(|current| current != wanted && (id == wanted || id < current.as_str()))
}

/// Antigravity's model catalog (the `fetchAvailableModels` `models` map) grouped into
/// client-facing models, in model name order. An ID without a display name is shown
/// under its own name.
pub fn model_groups(models: &Map<String, Value>) -> Vec<ModelGroup> {
    let mut groups = BTreeMap::<String, ModelGroup>::new();
    for (id, info) in models {
        let display = info["displayName"]
            .as_str()
            .filter(|display| !display.trim().is_empty())
            .unwrap_or(id);
        let (name, effort) = display_effort(display);
        let model = Some(model_name(name))
            .filter(|model| crate::valid_model(model))
            .unwrap_or_else(|| id.clone());
        let group = groups.entry(model.clone()).or_insert_with(|| ModelGroup {
            model,
            name: name.to_owned(),
            ..ModelGroup::default()
        });
        group.upstream_models.push(id.clone());
        if THIRD_PARTY_PROVIDERS.contains(&info["modelProvider"].as_str().unwrap_or_default()) {
            group.quota_pool = Some(THIRD_PARTY_POOL);
        }
        match effort {
            Some(effort) => {
                let wanted = format!("{}-{effort}", group.model);
                if preferred(group.efforts.get(effort), id, &wanted) {
                    group.efforts.insert(effort.to_owned(), id.clone());
                }
            }
            None => {
                if preferred(group.upstream_model.as_ref(), id, &group.model) {
                    group.upstream_model = Some(id.clone());
                }
            }
        }
    }
    groups
        .into_values()
        .map(|mut group| {
            group.upstream_models.sort();
            group
        })
        .collect()
}

/// The remaining quota fraction one catalog entry reports. Proto JSON omits a zero
/// fraction, so a reset time or `isExhausted` without one means the quota is spent; an
/// entry with neither reports no quota.
pub fn remaining_fraction(info: &Value) -> Option<f64> {
    let quota = info.get("quotaInfo").filter(|quota| quota.is_object())?;
    match quota["remainingFraction"].as_f64() {
        Some(fraction) => Some(fraction.clamp(0.0, 1.0)),
        None if quota["resetTime"].is_string() || quota["isExhausted"] == true => Some(0.0),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn display_names_group_effort_variants_and_aliases_into_models() {
        let catalog = json!({
            "gemini-3-flash-agent": {"displayName": "Gemini 3.5 Flash (High)"},
            "gemini-3.5-flash-extra-low": {"displayName": "Gemini 3.5 Flash (Low)"},
            "gemini-3.5-flash-low": {"displayName": "Gemini 3.5 Flash (Medium)"},
            "gemini-2.5-flash": {"displayName": "Gemini 3.5 Flash Lite"},
            "gemini-2.5-flash-thinking": {"displayName": "Gemini 3.5 Flash Lite"},
            "gemini-3.5-flash-lite": {"displayName": "Gemini 3.5 Flash Lite"},
            "gemini-3.8-flash-high": {"displayName": "Gemini 3.8 Flash (High)"},
            "gemini-4-argon-high": {"displayName": "Gemini 4 Argon (High)"},
            "gemini-4-argon-low": {"displayName": "Gemini 4 Argon (Low)"},
            "gemini-4-argon-medium": {"displayName": "Gemini 4 Argon (Medium)"},
            "gemini-3.8-flash-tiered": {"displayName": "gemini-3.8-flash-tiered"},
            "gpt-oss-120b-medium": {"displayName": "GPT-OSS 120B (Medium)",
                "modelProvider": "MODEL_PROVIDER_OPENAI"},
            "claude-opus-5-5-low": {"displayName": "Claude Opus 5.5 (Low)",
                "modelProvider": "MODEL_PROVIDER_ANTHROPIC"},
            "tab_flash_lite_preview": {}
        });
        let groups = model_groups(catalog.as_object().unwrap());
        let group = |model: &str| groups.iter().find(|group| group.model == model).unwrap();
        let flash = group("gemini-3.5-flash");
        assert_eq!(flash.name, "Gemini 3.5 Flash");
        assert_eq!(flash.efforts["high"], "gemini-3-flash-agent");
        assert_eq!(flash.efforts["low"], "gemini-3.5-flash-extra-low");
        assert_eq!(flash.efforts["medium"], "gemini-3.5-flash-low");
        assert_eq!(flash.upstream_model, None);
        let lite = group("gemini-3.5-flash-lite");
        assert!(lite.efforts.is_empty());
        assert_eq!(
            lite.upstream_model.as_deref(),
            Some("gemini-3.5-flash-lite")
        );
        assert_eq!(lite.upstream_models.len(), 3);
        assert_eq!(
            group("gemini-3.8-flash-tiered").upstream_model.as_deref(),
            Some("gemini-3.8-flash-tiered")
        );
        assert_eq!(group("gemini-3.8-flash").efforts.len(), 1);
        let argon = group("gemini-4-argon");
        assert_eq!(argon.efforts["high"], "gemini-4-argon-high");
        assert_eq!(argon.efforts["low"], "gemini-4-argon-low");
        assert_eq!(argon.efforts["medium"], "gemini-4-argon-medium");
        assert_eq!(argon.upstream_model, None);
        assert_eq!(
            group("gpt-oss-120b").efforts["medium"],
            "gpt-oss-120b-medium"
        );
        assert_eq!(
            group("claude-opus-5.5").efforts["low"],
            "claude-opus-5-5-low"
        );
        // Anthropic and OpenAI models share one quota pool; Google models keep their own.
        for model in ["gpt-oss-120b", "claude-opus-5.5"] {
            assert_eq!(group(model).quota_pool, Some(THIRD_PARTY_POOL), "{model}");
        }
        assert_eq!(flash.quota_pool, None);
        assert_eq!(
            group("tab_flash_lite_preview").upstream_model.as_deref(),
            Some("tab_flash_lite_preview")
        );
        assert_eq!(groups.len(), 8);
        assert_eq!(display_effort("Claude Sonnet 4.5 (Thinking)").1, None);
        assert_eq!(
            model_name("Claude Sonnet 4.5 (Thinking)"),
            "claude-sonnet-4.5-thinking"
        );
    }

    #[test]
    fn a_reset_without_a_fraction_is_spent_and_no_quota_is_unknown() {
        for (info, remaining) in [
            (
                json!({"quotaInfo": {"remainingFraction": 0.25}}),
                Some(0.25),
            ),
            (
                json!({"quotaInfo": {"resetTime": "2026-10-20T00:00:00Z"}}),
                Some(0.0),
            ),
            (json!({"quotaInfo": {"isExhausted": true}}), Some(0.0)),
            (json!({"quotaInfo": {}}), None),
            (json!({}), None),
        ] {
            assert_eq!(remaining_fraction(&info), remaining, "{info}");
        }
    }
}
