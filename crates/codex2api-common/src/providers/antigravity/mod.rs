//! Antigravity (Google Cloud Code Assist) supplier constants and default prices.
//! Antigravity has no per-token price of its own; each model family defaults to its
//! vendor's official API list price. These presets never assert availability.
pub mod version;
use crate::TokenPricePreset;

pub const PRESET_VERSION: &str = "2026-10-19";
pub const GOOGLE_PRICING: &str = "https://ai.google.dev/gemini-api/docs/pricing";
pub const ANTHROPIC_PRICING: &str = "https://platform.claude.com/docs/en/about-claude/pricing";

/// Priced families, usable as preset names when a supplier ID is opaque.
pub const PRICED_FAMILIES: &[&str] = &[
    "gemini-4-argon",
    "gemini-3.8-flash",
    "gemini-3.6-flash",
    "gemini-3.5-flash-lite",
    "gemini-3.1-pro",
    "gemini-3.1-flash-lite",
    "gemini-3-flash",
    "gemini-2.5-pro",
    "gemini-2.5-flash",
    "gemini-2.5-flash-lite",
    "claude-fable-5.1",
    "claude-fable-5",
    "claude-opus-5.5",
    "claude-opus-5",
    "claude-opus-4.8",
    "claude-opus-4.7",
    "claude-opus-4.6",
    "claude-opus-4.5",
    "claude-sonnet-5.5",
    "claude-sonnet-5",
    "claude-sonnet-4.6",
    "claude-sonnet-4.5",
    "claude-haiku-5.5",
    "claude-haiku-4.5",
];

/// Antigravity catalog IDs and display names carry a reasoning tier and sometimes a
/// routing token; both reduce to the vendor family, with version dots restored.
pub fn family(name: &str) -> Option<String> {
    const TIERS: [&str; 9] = [
        "low", "medium", "high", "extra", "thinking", "tiered", "agent", "preview", "new",
    ];
    let mut words: Vec<String> = Vec::new();
    for word in name
        .to_ascii_lowercase()
        .split([' ', '-', '_', '(', ')'])
        .filter(|w| !w.is_empty())
    {
        if word.bytes().all(|b| b.is_ascii_digit())
            && let Some(last) = words.last_mut()
            && last.bytes().all(|b| b.is_ascii_digit())
        {
            last.push('.');
            last.push_str(word);
            continue;
        }
        words.push(word.to_owned());
    }
    while words.last().is_some_and(|w| TIERS.contains(&w.as_str())) {
        words.pop();
    }
    (words.len() >= 2).then(|| words.join("-"))
}

pub fn vendor_source(model: &str) -> &'static str {
    if family(model).is_some_and(|f| f.starts_with("claude-")) {
        ANTHROPIC_PRICING
    } else {
        GOOGLE_PRICING
    }
}

/// Google: standard paid tier, context-cache reads at 10% of input, no cache-write
/// surcharge, 200K prompt split where listed. Anthropic: base input, cache hits,
/// 5-minute cache writes and output; Claude 4.6+ keeps one rate across the context
/// window and Haiku 5.5 splits at 100K prompt tokens.
pub fn model_price_preset(model: &str) -> Option<Vec<TokenPricePreset>> {
    let family = family(model)?;
    type Rates = (i64, i64, i64, i64);
    let (base, long): (Rates, Option<(i64, Rates)>) = match family.as_str() {
        "gemini-4-argon" => ((2_000_000, 100_000, 2_000_000, 10_000_000), None),
        "gemini-3.8-flash" | "gemini-3.6-flash" => ((750_000, 75_000, 750_000, 3_750_000), None),
        "gemini-3.5-flash-lite" => ((300_000, 30_000, 300_000, 2_500_000), None),
        "gemini-3.1-flash-lite" => ((250_000, 25_000, 250_000, 1_500_000), None),
        "gemini-3.1-pro" => (
            (2_000_000, 200_000, 2_000_000, 12_000_000),
            Some((200_001, (4_000_000, 400_000, 4_000_000, 18_000_000))),
        ),
        "gemini-3-flash" => ((500_000, 50_000, 500_000, 3_000_000), None),
        "gemini-2.5-pro" => (
            (1_250_000, 125_000, 1_250_000, 10_000_000),
            Some((200_001, (2_500_000, 250_000, 2_500_000, 15_000_000))),
        ),
        "gemini-2.5-flash" => ((300_000, 30_000, 300_000, 2_500_000), None),
        "gemini-2.5-flash-lite" => ((100_000, 10_000, 100_000, 400_000), None),
        "claude-fable-5.1" => ((10_000_000, 250_000, 12_500_000, 50_000_000), None),
        "claude-fable-5" => ((10_000_000, 1_000_000, 12_500_000, 50_000_000), None),
        "claude-opus-5.5" => ((4_000_000, 200_000, 5_000_000, 20_000_000), None),
        "claude-opus-5" | "claude-opus-4.8" | "claude-opus-4.7" | "claude-opus-4.6"
        | "claude-opus-4.5" => ((5_000_000, 500_000, 6_250_000, 25_000_000), None),
        "claude-sonnet-5.5" => ((2_000_000, 100_000, 2_500_000, 10_000_000), None),
        "claude-sonnet-5" => ((2_000_000, 200_000, 2_500_000, 10_000_000), None),
        "claude-sonnet-4.6" | "claude-sonnet-4.5" => {
            ((3_000_000, 300_000, 3_750_000, 15_000_000), None)
        }
        "claude-haiku-5.5" => (
            (100_000, 10_000, 125_000, 500_000),
            Some((100_001, (500_000, 50_000, 625_000, 2_500_000))),
        ),
        "claude-haiku-4.5" => ((1_000_000, 100_000, 1_250_000, 5_000_000), None),
        _ => return None,
    };
    let preset = |min: i64, (input, cached, write, output): Rates| TokenPricePreset {
        tier: "standard",
        min_input_tokens: min,
        max_input_tokens: None,
        input_rate: input,
        cached_rate: cached,
        cache_write_rate: write,
        output_rate: output,
    };
    let mut prices = vec![preset(0, base)];
    if let Some((threshold, rates)) = long {
        prices.push(preset(threshold, rates));
    }
    Some(prices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antigravity_ids_and_display_names_reduce_to_vendor_families() {
        for (name, expected) in [
            ("Gemini 4 Argon High", Some("gemini-4-argon")),
            ("gemini-4-argon-high", Some("gemini-4-argon")),
            ("Gemini 3.8 Flash Medium", Some("gemini-3.8-flash")),
            ("gemini-3.5-flash-extra-low", Some("gemini-3.5-flash")),
            ("Claude Opus 5.5 Medium", Some("claude-opus-5.5")),
            ("claude-opus-4-6-thinking", Some("claude-opus-4.6")),
            ("Gemini 3.1 Pro Preview", Some("gemini-3.1-pro")),
            ("gemini-3-flash-agent", Some("gemini-3-flash")),
            ("GPT-OSS 120B (Medium)", Some("gpt-oss-120b")),
            ("gemini-pro-agent", Some("gemini-pro")),
            ("Medium", None),
        ] {
            assert_eq!(family(name).as_deref(), expected, "{name}");
        }
        assert!(model_price_preset("gpt-oss-120b-medium").is_none());
        assert!(model_price_preset("gemini-pro-agent").is_none());
        let pro = model_price_preset("Gemini 3.1 Pro High").unwrap();
        assert_eq!(
            (pro[1].min_input_tokens, pro[1].output_rate),
            (200_001, 18_000_000)
        );
        let opus = model_price_preset("claude-opus-5-5-medium").unwrap();
        assert_eq!(
            (opus.len(), opus[0].cached_rate, opus[0].output_rate),
            (1, 200_000, 20_000_000)
        );
        assert_eq!(vendor_source("claude-sonnet-5-5-medium"), ANTHROPIC_PRICING);
        assert_eq!(vendor_source("gemini-3.8-flash-medium"), GOOGLE_PRICING);
    }
}
