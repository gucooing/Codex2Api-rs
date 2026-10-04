//! Public xAI USD token rates. These do not assert Grok Build account availability.
use crate::TokenPricePreset;
pub mod subscriptions;
pub use subscriptions::{SUBSCRIPTION_TIERS, subscription};
pub const PRESET_VERSION: &str = "2026-10-04";
pub const PRICED_MODELS: &[&str] = &[
    "grok-4.7",
    "grok-build-0.1",
    "grok-4.6",
    "grok-4.5",
    "grok-4.3",
    "grok-4.20-multi-agent-0309",
    "grok-4.20-0309-reasoning",
    "grok-4.20-0309-non-reasoning",
];
/// https://docs.x.ai/developers/pricing, prompt boundary >=200,000 tokens.
/// Grok cache creation is ordinary uncached input, without a separate write surcharge.
pub fn model_price_preset(model: &str) -> Option<Vec<TokenPricePreset>> {
    let (input, cached, output) = match model {
        "grok-4.7" | "grok-4.6" => (2_000_000, 500_000, 6_000_000),
        "grok-4.5" => (2_000_000, 300_000, 6_000_000),
        "grok-build-0.1" => (1_000_000, 200_000, 2_000_000),
        "grok-4.3"
        | "grok-4.20-multi-agent-0309"
        | "grok-4.20-0309-reasoning"
        | "grok-4.20-0309-non-reasoning" => (1_250_000, 200_000, 2_500_000),
        _ => return None,
    };
    Some(
        [0, 200_000]
            .into_iter()
            .map(|min| {
                let factor = if min == 0 { 1 } else { 2 };
                TokenPricePreset {
                    tier: "standard",
                    min_input_tokens: min,
                    max_input_tokens: None,
                    input_rate: input * factor,
                    cached_rate: cached * factor,
                    cache_write_rate: input * factor,
                    output_rate: output * factor,
                }
            })
            .collect(),
    )
}
