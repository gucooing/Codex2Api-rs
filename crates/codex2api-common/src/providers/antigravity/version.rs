//! Antigravity desktop client wire constants, independent of the Codex and Grok baselines.

/// Client version carried by the User-Agent and the onboarding metadata.
pub const VERSION: &str = "2.9.1";

/// Google OAuth endpoints of the official desktop client.
pub const OAUTH_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
pub const CLIENT_ID: &str =
    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
pub const CLIENT_SECRET: &str = "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";
/// Fixed loopback redirect of the desktop client; the browser result is pasted back.
pub const REDIRECT_URI: &str = "http://localhost:8085/callback";
pub const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/cclog https://www.googleapis.com/auth/experimentsandconfigs";
pub const ACCESS_TOKEN_TTL: i64 = 3600;

/// Cloud Code Assist service roots: production, and the daily root paid tiers use.
pub const BASE_URL: &str = "https://cloudcode-pa.googleapis.com";
pub const DAILY_BASE_URL: &str = "https://daily-cloudcode-pa.googleapis.com";
pub const PATH_STREAM_GENERATE: &str = "/v1internal:streamGenerateContent";
pub const PATH_LOAD_CODE_ASSIST: &str = "/v1internal:loadCodeAssist";
pub const PATH_ONBOARD_USER: &str = "/v1internal:onboardUser";
pub const PATH_FETCH_MODELS: &str = "/v1internal:fetchAvailableModels";

/// v1internal envelope values sent by the desktop client.
pub const ENVELOPE_USER_AGENT: &str = "antigravity";
pub const REQUEST_TYPE_AGENT: &str = "agent";
pub const REQUEST_ID_PREFIX: &str = "agent-";
pub const IDE_TYPE: &str = "ANTIGRAVITY";
pub const IDE_NAME: &str = "antigravity";
pub const ONBOARD_PLATFORM: &str = "PLATFORM_UNSPECIFIED";
pub const ONBOARD_PLUGIN_TYPE: &str = "GEMINI";
/// Documented Gemini escape hatch for replaying function calls without a signature.
pub const DUMMY_THOUGHT_SIGNATURE: &str = "skip_thought_signature_validator";

/// Official tier identifiers reported by loadCodeAssist.
pub const TIER_FREE: &str = "free-tier";
pub const TIER_PRO: &str = "g1-pro-tier";
pub const TIER_ULTRA: &str = "g1-ultra-tier";

pub fn tier_display(tier: &str) -> &str {
    match tier {
        TIER_FREE => "Free",
        TIER_PRO => "Pro",
        TIER_ULTRA => "Ultra",
        other => other,
    }
}

pub fn paid_tier(tier: &str) -> bool {
    matches!(tier, TIER_PRO | TIER_ULTRA)
}

/// User-Agent formula of the desktop client: `antigravity/{version} {os}/{arch}`
/// with Go-style platform tokens.
pub fn user_agent(os: &str, arch: &str) -> String {
    format!("antigravity/{VERSION} {os}/{arch}")
}
