use crate::TokenPricePreset;

pub const SEARCH_OPERATION: &str = "search";

pub fn billing_operation(endpoint: &str) -> Option<&'static str> {
    matches!(endpoint, "/v1/alpha/search").then_some(SEARCH_OPERATION)
}
/// Verified public token prices; unsupported prices remain absent.
pub fn model_price_preset(model: &str) -> Option<Vec<TokenPricePreset>> {
    // Source: https://developers.openai.com/api/docs/pricing and each model page,
    // Standard/Fast/Flex checked 2026-10-03; Astra/Sol Ultrafast checked 2026-10-09.
    // GPT-5.5 cache creation has no surcharge: its effective write rate equals
    // uncached input (prompt-caching guide). Hidden aliases/image bands are absent.
    let (input, cached, cache_write, output) = match model {
        "gpt-6-astra" => (10_000_000, 1_000_000, 12_500_000, 50_000_000),
        "gpt-6.1-sol" => (2_000_000, 100_000, 2_500_000, 10_000_000),
        "gpt-6-sol" => (2_000_000, 200_000, 2_500_000, 10_000_000),
        "gpt-6-luna" => (100_000, 10_000, 125_000, 500_000),
        "gpt-5.6-sol" => (4_000_000, 400_000, 5_000_000, 20_000_000),
        "gpt-5.6-terra" => (2_000_000, 200_000, 2_500_000, 12_000_000),
        "gpt-5.6-luna" => (200_000, 20_000, 250_000, 1_200_000),
        "gpt-5.5" => (5_000_000, 500_000, 5_000_000, 30_000_000),
        _ => return None,
    };
    let mut prices = Vec::with_capacity(8);
    for (tier, numerator, denominator) in [
        ("standard", 1, 1),
        ("fast", 2, 1),
        ("flex", 1, 2),
        ("ultrafast", 6, 1),
    ] {
        if tier == "ultrafast" && !matches!(model, "gpt-6-astra" | "gpt-6.1-sol") {
            continue;
        }
        let limited_fast = model == "gpt-5.5" && tier == "fast";
        let (numerator, denominator) = if limited_fast {
            (5, 2)
        } else {
            (numerator, denominator)
        };
        for (start, input_multiplier, output_numerator, output_denominator) in
            [(0, 1, 1, 1), (272_001, 2, 3, 2)]
        {
            if limited_fast && start != 0 {
                continue;
            }
            prices.push(TokenPricePreset {
                tier,
                min_input_tokens: start,
                max_input_tokens: limited_fast.then_some(272_000),
                input_rate: input * input_multiplier * numerator / denominator,
                cached_rate: cached * input_multiplier * numerator / denominator,
                cache_write_rate: cache_write * input_multiplier * numerator / denominator,
                output_rate: output * output_numerator * numerator
                    / output_denominator
                    / denominator,
            });
        }
    }
    Some(prices)
}

pub const SUBSCRIPTION_TIERS: &[&str] = &[
    "free",
    "go",
    "plus",
    "prolite",
    "pro",
    "promax",
    "team",
    "business",
    "enterprise",
    "edu",
    "edu_plus",
    "edu_pro",
    "self_serve_business_prolite",
    "self_serve_business_usage_based",
    "ent26",
    "enterprise_cbp_automation",
    "enterprise_cbp_usage_based",
];
