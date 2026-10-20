//! API-key supplier channel constants: the wire formats an account may speak, the
//! authentication header schemes each wire accepts, the same-account retry bounds and
//! the fixed client identity. The channel has no device fingerprint and no official
//! URLs; every account names its own Base URL.
pub mod version;

/// Wire formats an API-key account may be configured with.
pub const WIRES: [&str; 3] = ["openai_responses", "chat_completions", "messages"];

pub const WIRE_OPENAI_RESPONSES: &str = "openai_responses";
pub const WIRE_CHAT_COMPLETIONS: &str = "chat_completions";
pub const WIRE_MESSAGES: &str = "messages";
/// `Authorization: Bearer`.
pub const AUTH_BEARER: &str = "bearer";
/// Anthropic's `x-api-key`.
pub const AUTH_X_API_KEY: &str = "x_api_key";

/// Same-account retries of an attempt whose status the administrator listed.
pub const DEFAULT_RETRY_COUNT: u32 = 2;
pub const MAX_RETRY_COUNT: u32 = 10;
/// Longest account name, in characters.
pub const MAX_NAME_CHARS: usize = 128;

/// The endpoint path of a generation under the account's Base URL.
pub fn wire_path(wire: &str) -> Option<&'static str> {
    Some(match wire {
        WIRE_OPENAI_RESPONSES => "responses",
        WIRE_CHAT_COMPLETIONS => "chat/completions",
        WIRE_MESSAGES => "messages",
        _ => return None,
    })
}

/// The official Base URL of the wire's vendor, offered as the form default.
pub fn default_base_url(wire: &str) -> &'static str {
    match wire {
        WIRE_MESSAGES => "https://api.anthropic.com/v1",
        _ => "https://api.openai.com/v1",
    }
}

/// The header schemes an endpoint of the wire accepts, the vendor's own first: the
/// OpenAI wires take only a Bearer token; Messages takes `x-api-key` or a Bearer token.
pub fn auth_schemes(wire: &str) -> &'static [&'static str] {
    match wire {
        WIRE_OPENAI_RESPONSES | WIRE_CHAT_COMPLETIONS => &[AUTH_BEARER],
        WIRE_MESSAGES => &[AUTH_X_API_KEY, AUTH_BEARER],
        _ => &[],
    }
}

/// The vendor's own authentication scheme for the wire.
pub fn default_auth_scheme(wire: &str) -> &'static str {
    auth_schemes(wire).first().copied().unwrap_or(AUTH_BEARER)
}

pub fn valid_wire(wire: &str) -> bool {
    WIRES.contains(&wire)
}

pub fn valid_auth_scheme(wire: &str, scheme: &str) -> bool {
    auth_schemes(wire).contains(&scheme)
}
