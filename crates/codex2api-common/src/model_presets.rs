//! Initial service prices, shared by administration and catalog registration.
//! Existing prices and request snapshots are never updated from these presets.

pub const MODEL_PRESET_VERSION: &str = "2026-10-14";

#[derive(Clone, Debug)]
pub struct SupportedModel {
    pub provider_id: String,
    pub model: String,
    pub kind: String,
    /// Preset lookup name when the supplier's model ID is opaque; the ID otherwise.
    pub preset_key: Option<String>,
}
impl SupportedModel {
    pub fn preset_key(&self) -> &str {
        self.preset_key.as_deref().unwrap_or(&self.model)
    }
}

#[derive(Clone, Debug)]
pub struct TokenPricePreset {
    pub tier: &'static str,
    pub min_input_tokens: i64,
    pub max_input_tokens: Option<i64>,
    /// Integer micro-USD per million tokens, matching the billing ledger.
    pub input_rate: i64,
    pub cached_rate: i64,
    pub cache_write_rate: i64,
    pub output_rate: i64,
}

pub fn model_price_preset(provider: &str, model: &str) -> Option<Vec<TokenPricePreset>> {
    match provider {
        crate::CHATGPT => crate::providers::chatgpt::model_price_preset(model),
        crate::GROK => crate::providers::grok::model_price_preset(model),
        crate::ANTIGRAVITY => crate::providers::antigravity::model_price_preset(model),
        _ => None,
    }
}
pub fn model_preset_source(provider: &str, model: &str) -> (&'static str, &'static str) {
    match provider {
        crate::GROK => (
            "https://docs.x.ai/developers/pricing",
            crate::providers::grok::PRESET_VERSION,
        ),
        crate::ANTIGRAVITY => (
            crate::providers::antigravity::vendor_source(model),
            crate::providers::antigravity::PRESET_VERSION,
        ),
        _ => (
            "https://developers.openai.com/api/docs/pricing",
            MODEL_PRESET_VERSION,
        ),
    }
}

pub fn model_preset_verified_at(provider: &str) -> &'static str {
    match provider {
        crate::GROK => crate::providers::grok::PRESET_VERSION,
        crate::ANTIGRAVITY => crate::providers::antigravity::PRESET_VERSION,
        _ => "2026-10-09",
    }
}
