//! Antigravity (Google Cloud Code Assist) protocol constants. Model prices live in the
//! shared built-in model table; this module only normalizes the client's model names.
pub mod version;

/// Antigravity catalog IDs and display names carry a reasoning tier and sometimes a
/// routing token; both reduce to the vendor model name, with version dots restored,
/// which is the name the built-in model table prices.
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

/// Vendor label of a model Antigravity serves, when its name reveals the vendor.
/// Antigravity itself is a supplier channel, never a model vendor.
pub fn platform_label(name: &str) -> Option<&'static str> {
    let family = family(name)?;
    if family.starts_with("claude-") {
        Some(crate::CLAUDE)
    } else if family.starts_with("gemini-") {
        Some(crate::GEMINI)
    } else if family.starts_with("gpt-") {
        Some(crate::CHATGPT)
    } else if family.starts_with("grok-") {
        Some(crate::GROK)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antigravity_ids_and_display_names_reduce_to_vendor_models() {
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
        assert!(crate::model_price_preset(&family("claude-opus-5-5-medium").unwrap()).is_some());
        assert!(crate::model_price_preset(&family("gpt-oss-120b-medium").unwrap()).is_none());
        assert_eq!(
            platform_label("Claude Opus 5.5 Medium"),
            Some(crate::CLAUDE)
        );
        assert_eq!(platform_label("gemini-3-flash-agent"), Some(crate::GEMINI));
        assert_eq!(
            platform_label("GPT-OSS 120B (Medium)"),
            Some(crate::CHATGPT)
        );
        assert_eq!(platform_label("llama-4-maverick-medium"), None);
    }
}
