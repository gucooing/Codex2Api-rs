//! Every built-in model of every platform in one table, with its verified public
//! price. The platform is an identification label only. Startup registers the whole
//! table into the catalog; existing rows, prices and request snapshots are never
//! updated from it.

/// Version written into preset write requests; bump when any price row changes.
pub const MODEL_PRESET_VERSION: &str = "2026-10-19";

#[derive(Clone, Debug)]
pub struct SupportedModel {
    /// Identification label of the model's platform; never a routing restriction.
    pub platform: String,
    pub model: String,
    pub kind: String,
}
impl SupportedModel {
    pub fn builtin(platform: &str, model: &str) -> Self {
        Self {
            platform: platform.into(),
            model: model.into(),
            kind: "text".into(),
        }
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

/// Standard rates of one context band: (start, input, cached input, cache write, output).
pub type Band = (i64, i64, i64, i64, i64);
/// A service tier priced as a ratio of every standard band: (tier, numerator, denominator).
pub type Tier = (&'static str, i64, i64);

pub struct BuiltinModel {
    pub platform: &'static str,
    pub model: &'static str,
    /// Official public price page the rates were read from.
    pub source: &'static str,
    /// Date the rates were last checked against that page.
    pub verified: &'static str,
    pub bands: &'static [Band],
    pub tiers: &'static [Tier],
}

const OPENAI: &str = "https://developers.openai.com/api/docs/pricing";
const XAI: &str = "https://docs.x.ai/developers/pricing";
const GOOGLE: &str = "https://ai.google.dev/gemini-api/docs/pricing";
const ANTHROPIC: &str = "https://platform.claude.com/docs/en/about-claude/pricing";

const FAST_FLEX: &[Tier] = &[("fast", 2, 1), ("flex", 1, 2)];
const FAST_FLEX_ULTRAFAST: &[Tier] = &[("fast", 2, 1), ("flex", 1, 2), ("ultrafast", 6, 1)];

const fn openai(
    model: &'static str,
    bands: &'static [Band],
    tiers: &'static [Tier],
) -> BuiltinModel {
    BuiltinModel {
        platform: crate::CHATGPT,
        model,
        source: OPENAI,
        verified: "2026-10-09",
        bands,
        tiers,
    }
}
const fn xai(model: &'static str, bands: &'static [Band]) -> BuiltinModel {
    BuiltinModel {
        platform: crate::GROK,
        model,
        source: XAI,
        verified: "2026-10-04",
        bands,
        tiers: &[],
    }
}
/// Models Antigravity serves are labelled by their vendor, not by the supplier platform.
const fn gemini(model: &'static str, bands: &'static [Band]) -> BuiltinModel {
    BuiltinModel {
        platform: crate::GEMINI,
        model,
        source: GOOGLE,
        verified: "2026-10-19",
        bands,
        tiers: &[],
    }
}
const fn claude(model: &'static str, bands: &'static [Band]) -> BuiltinModel {
    BuiltinModel {
        platform: crate::CLAUDE,
        model,
        source: ANTHROPIC,
        verified: "2026-10-19",
        bands,
        tiers: &[],
    }
}

/// OpenAI: Standard/Fast/Flex (Ultrafast for Astra and 6.1 Sol) with the long-context
/// band from 272,001 input tokens; GPT-5.5 cache creation has no surcharge and its Fast
/// tier is 2.5x. xAI: Standard only, long-context band from 200,000 input tokens, cache
/// creation at the ordinary input rate. Google: standard paid tier, context-cache reads
/// at 10% of input, no cache-write surcharge, 200K split where listed. Anthropic: base
/// input, cache hits, 5-minute cache writes and output; Haiku 5.5 splits at 100K.
pub const BUILTIN_MODELS: &[BuiltinModel] = &[
    openai(
        "gpt-6-astra",
        &[
            (0, 10_000_000, 1_000_000, 12_500_000, 50_000_000),
            (272_001, 20_000_000, 2_000_000, 25_000_000, 75_000_000),
        ],
        FAST_FLEX_ULTRAFAST,
    ),
    openai(
        "gpt-6.1-sol",
        &[
            (0, 2_000_000, 100_000, 2_500_000, 10_000_000),
            (272_001, 4_000_000, 200_000, 5_000_000, 15_000_000),
        ],
        FAST_FLEX_ULTRAFAST,
    ),
    openai(
        "gpt-6-sol",
        &[
            (0, 2_000_000, 200_000, 2_500_000, 10_000_000),
            (272_001, 4_000_000, 400_000, 5_000_000, 15_000_000),
        ],
        FAST_FLEX,
    ),
    openai(
        "gpt-6-luna",
        &[
            (0, 100_000, 10_000, 125_000, 500_000),
            (272_001, 200_000, 20_000, 250_000, 750_000),
        ],
        FAST_FLEX,
    ),
    openai(
        "gpt-5.6-sol",
        &[
            (0, 4_000_000, 400_000, 5_000_000, 20_000_000),
            (272_001, 8_000_000, 800_000, 10_000_000, 30_000_000),
        ],
        FAST_FLEX,
    ),
    openai(
        "gpt-5.6-terra",
        &[
            (0, 2_000_000, 200_000, 2_500_000, 12_000_000),
            (272_001, 4_000_000, 400_000, 5_000_000, 18_000_000),
        ],
        FAST_FLEX,
    ),
    openai(
        "gpt-5.6-luna",
        &[
            (0, 200_000, 20_000, 250_000, 1_200_000),
            (272_001, 400_000, 40_000, 500_000, 1_800_000),
        ],
        FAST_FLEX,
    ),
    openai(
        "gpt-5.5",
        &[
            (0, 5_000_000, 500_000, 5_000_000, 30_000_000),
            (272_001, 10_000_000, 1_000_000, 10_000_000, 45_000_000),
        ],
        &[("fast", 5, 2), ("flex", 1, 2)],
    ),
    xai(
        "grok-4.7",
        &[
            (0, 2_000_000, 500_000, 2_000_000, 6_000_000),
            (200_000, 4_000_000, 1_000_000, 4_000_000, 12_000_000),
        ],
    ),
    xai(
        "grok-build-0.1",
        &[
            (0, 1_000_000, 200_000, 1_000_000, 2_000_000),
            (200_000, 2_000_000, 400_000, 2_000_000, 4_000_000),
        ],
    ),
    xai(
        "grok-4.6",
        &[
            (0, 2_000_000, 500_000, 2_000_000, 6_000_000),
            (200_000, 4_000_000, 1_000_000, 4_000_000, 12_000_000),
        ],
    ),
    xai(
        "grok-4.5",
        &[
            (0, 2_000_000, 300_000, 2_000_000, 6_000_000),
            (200_000, 4_000_000, 600_000, 4_000_000, 12_000_000),
        ],
    ),
    xai(
        "grok-4.3",
        &[
            (0, 1_250_000, 200_000, 1_250_000, 2_500_000),
            (200_000, 2_500_000, 400_000, 2_500_000, 5_000_000),
        ],
    ),
    xai(
        "grok-4.20-multi-agent-0309",
        &[
            (0, 1_250_000, 200_000, 1_250_000, 2_500_000),
            (200_000, 2_500_000, 400_000, 2_500_000, 5_000_000),
        ],
    ),
    xai(
        "grok-4.20-0309-reasoning",
        &[
            (0, 1_250_000, 200_000, 1_250_000, 2_500_000),
            (200_000, 2_500_000, 400_000, 2_500_000, 5_000_000),
        ],
    ),
    xai(
        "grok-4.20-0309-non-reasoning",
        &[
            (0, 1_250_000, 200_000, 1_250_000, 2_500_000),
            (200_000, 2_500_000, 400_000, 2_500_000, 5_000_000),
        ],
    ),
    gemini(
        "gemini-4-argon",
        &[(0, 2_000_000, 100_000, 2_000_000, 10_000_000)],
    ),
    gemini(
        "gemini-3.8-flash",
        &[(0, 750_000, 75_000, 750_000, 3_750_000)],
    ),
    gemini(
        "gemini-3.6-flash",
        &[(0, 750_000, 75_000, 750_000, 3_750_000)],
    ),
    gemini(
        "gemini-3.5-flash-lite",
        &[(0, 300_000, 30_000, 300_000, 2_500_000)],
    ),
    gemini(
        "gemini-3.1-pro",
        &[
            (0, 2_000_000, 200_000, 2_000_000, 12_000_000),
            (200_001, 4_000_000, 400_000, 4_000_000, 18_000_000),
        ],
    ),
    gemini(
        "gemini-3.1-flash-lite",
        &[(0, 250_000, 25_000, 250_000, 1_500_000)],
    ),
    gemini(
        "gemini-3-flash",
        &[(0, 500_000, 50_000, 500_000, 3_000_000)],
    ),
    gemini(
        "gemini-2.5-pro",
        &[
            (0, 1_250_000, 125_000, 1_250_000, 10_000_000),
            (200_001, 2_500_000, 250_000, 2_500_000, 15_000_000),
        ],
    ),
    gemini(
        "gemini-2.5-flash",
        &[(0, 300_000, 30_000, 300_000, 2_500_000)],
    ),
    gemini(
        "gemini-2.5-flash-lite",
        &[(0, 100_000, 10_000, 100_000, 400_000)],
    ),
    claude(
        "claude-fable-5-1",
        &[(0, 10_000_000, 250_000, 12_500_000, 50_000_000)],
    ),
    claude(
        "claude-fable-5",
        &[(0, 10_000_000, 1_000_000, 12_500_000, 50_000_000)],
    ),
    claude(
        "claude-opus-5-5",
        &[(0, 4_000_000, 200_000, 5_000_000, 20_000_000)],
    ),
    claude(
        "claude-opus-5",
        &[(0, 5_000_000, 500_000, 6_250_000, 25_000_000)],
    ),
    claude(
        "claude-opus-4-8",
        &[(0, 5_000_000, 500_000, 6_250_000, 25_000_000)],
    ),
    claude(
        "claude-opus-4-7",
        &[(0, 5_000_000, 500_000, 6_250_000, 25_000_000)],
    ),
    claude(
        "claude-opus-4-6",
        &[(0, 5_000_000, 500_000, 6_250_000, 25_000_000)],
    ),
    claude(
        "claude-opus-4-5",
        &[(0, 5_000_000, 500_000, 6_250_000, 25_000_000)],
    ),
    claude(
        "claude-sonnet-5-5",
        &[(0, 2_000_000, 100_000, 2_500_000, 10_000_000)],
    ),
    claude(
        "claude-sonnet-5",
        &[(0, 2_000_000, 200_000, 2_500_000, 10_000_000)],
    ),
    claude(
        "claude-sonnet-4-6",
        &[(0, 3_000_000, 300_000, 3_750_000, 15_000_000)],
    ),
    claude(
        "claude-sonnet-4-5",
        &[(0, 3_000_000, 300_000, 3_750_000, 15_000_000)],
    ),
    claude(
        "claude-haiku-5-5",
        &[
            (0, 100_000, 10_000, 125_000, 500_000),
            (100_001, 500_000, 50_000, 625_000, 2_500_000),
        ],
    ),
    claude(
        "claude-haiku-4-5",
        &[(0, 1_000_000, 100_000, 1_250_000, 5_000_000)],
    ),
];

pub fn builtin_model(model: &str) -> Option<&'static BuiltinModel> {
    BUILTIN_MODELS.iter().find(|entry| entry.model == model)
}

/// Every model defined in code, across all platforms, in one list.
pub fn builtin_models() -> Vec<SupportedModel> {
    BUILTIN_MODELS
        .iter()
        .map(|entry| SupportedModel::builtin(entry.platform, entry.model))
        .collect()
}

/// Verified price rules for a model name: every standard band, then each extra tier
/// applied to every band.
pub fn model_price_preset(model: &str) -> Option<Vec<TokenPricePreset>> {
    let entry = builtin_model(model)?;
    let mut prices = Vec::with_capacity(entry.bands.len() * (entry.tiers.len() + 1));
    for (tier, numerator, denominator) in
        std::iter::once(&("standard", 1, 1)).chain(entry.tiers.iter())
    {
        for &(start, input, cached, write, output) in entry.bands {
            prices.push(TokenPricePreset {
                tier,
                min_input_tokens: start,
                max_input_tokens: None,
                input_rate: input * numerator / denominator,
                cached_rate: cached * numerator / denominator,
                cache_write_rate: write * numerator / denominator,
                output_rate: output * numerator / denominator,
            });
        }
    }
    Some(prices)
}

/// The official reference, preset version and verification date of a built-in model.
pub fn model_preset_source(model: &str) -> Option<(&'static str, &'static str, &'static str)> {
    let entry = builtin_model(model)?;
    Some((entry.source, MODEL_PRESET_VERSION, entry.verified))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_models_are_unique_and_expand_bands_by_tier() {
        let mut names: Vec<&str> = BUILTIN_MODELS.iter().map(|m| m.model).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), BUILTIN_MODELS.len());
        for entry in BUILTIN_MODELS {
            assert!(crate::valid_model(entry.model), "{}", entry.model);
            assert!(
                crate::valid_model_platform(entry.platform),
                "{}",
                entry.model
            );
            assert_eq!(entry.bands[0].0, 0, "{}", entry.model);
        }
        let astra = model_price_preset("gpt-6-astra").unwrap();
        assert_eq!(astra.len(), 8);
        let fast_long = astra
            .iter()
            .find(|p| p.tier == "fast" && p.min_input_tokens == 272_001)
            .unwrap();
        assert_eq!(
            (fast_long.input_rate, fast_long.output_rate),
            (40_000_000, 150_000_000)
        );
        let fast55 = model_price_preset("gpt-5.5")
            .unwrap()
            .into_iter()
            .find(|p| p.tier == "fast" && p.min_input_tokens == 0)
            .unwrap();
        assert_eq!(fast55.input_rate, 12_500_000);
        let pro = model_price_preset("gemini-3.1-pro").unwrap();
        assert_eq!(
            (pro.len(), pro[1].min_input_tokens, pro[1].output_rate),
            (2, 200_001, 18_000_000)
        );
        assert!(model_price_preset("gpt-oss-120b").is_none());
    }
}
