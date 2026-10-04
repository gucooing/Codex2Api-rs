//! Initial service prices, shared by administration and catalog registration.
//! Existing prices and request snapshots are never updated from these presets.

pub const MODEL_PRESET_VERSION: &str = "2026-10-03";

#[derive(Clone, Debug)]
pub struct SupportedModel {
    pub provider_id: String,
    pub model: String,
    pub kind: String,
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
        _ => None,
    }
}
pub fn model_preset_source(provider: &str) -> (&'static str, &'static str) {
    match provider {
        crate::GROK => (
            "https://docs.x.ai/developers/pricing",
            crate::providers::grok::PRESET_VERSION,
        ),
        _ => (
            "https://developers.openai.com/api/docs/pricing",
            MODEL_PRESET_VERSION,
        ),
    }
}
