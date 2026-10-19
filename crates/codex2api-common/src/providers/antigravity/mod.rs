//! Antigravity (Google Cloud Code Assist) protocol constants and the client's model
//! naming. Prices live only in the administrator's billing catalog.
pub mod version;

/// Reasoning-effort suffixes of Antigravity model IDs and the public effort each one
/// serves. Longer suffixes come first so `-extra-low` is never read as `-low`.
const EFFORT_SUFFIXES: [(&str, &str); 5] = [
    ("extra-low", "minimal"),
    ("extra-high", "xhigh"),
    ("low", "low"),
    ("medium", "medium"),
    ("high", "high"),
];

/// Public reasoning efforts that can name an Antigravity variant, in ascending order.
pub const EFFORTS: [&str; 5] = ["minimal", "low", "medium", "high", "xhigh"];

/// The base model of an Antigravity model ID and the reasoning effort its suffix
/// names. The effort variants of one model share its base; an ID without an effort
/// suffix is its own base.
pub fn effort_variant(id: &str) -> (&str, Option<&'static str>) {
    for (suffix, effort) in EFFORT_SUFFIXES {
        if let Some(base) = id
            .strip_suffix(suffix)
            .and_then(|rest| rest.strip_suffix('-'))
            .filter(|base| !base.is_empty())
        {
            return (base, Some(effort));
        }
    }
    (id, None)
}

/// The upstream model ID a request for `base` at `effort` names: the effort's suffix
/// appended to the base, when the effort has one.
pub fn effort_model(base: &str, effort: &str) -> Option<String> {
    EFFORT_SUFFIXES
        .iter()
        .find(|(_, public)| *public == effort)
        .map(|(suffix, _)| format!("{base}-{suffix}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effort_suffixes_split_into_base_models_and_join_back() {
        for (id, base, effort) in [
            ("claude-opus-5-5-high", "claude-opus-5-5", Some("high")),
            ("claude-opus-5-5-low", "claude-opus-5-5", Some("low")),
            (
                "gemini-3.8-flash-medium",
                "gemini-3.8-flash",
                Some("medium"),
            ),
            (
                "gemini-3.5-flash-extra-low",
                "gemini-3.5-flash",
                Some("minimal"),
            ),
            ("gemini-3.5-flash-lite", "gemini-3.5-flash-lite", None),
            (
                "gemini-2.5-flash-thinking",
                "gemini-2.5-flash-thinking",
                None,
            ),
            ("gemini-3-flash-agent", "gemini-3-flash-agent", None),
            ("chat_20706", "chat_20706", None),
            ("high", "high", None),
        ] {
            assert_eq!(effort_variant(id), (base, effort), "{id}");
            if let Some(effort) = effort {
                assert_eq!(effort_model(base, effort).as_deref(), Some(id));
            }
        }
        assert_eq!(effort_model("gemini-2.5-pro", "none"), None);
    }
}
