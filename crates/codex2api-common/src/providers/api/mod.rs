//! API-key supplier channel constants: the wire formats an account may speak, the
//! authentication header schemes, and the fixed client identity. The channel has no
//! device fingerprint and no official URLs; every account names its own Base URL.
pub mod version;

/// Wire formats an API-key account may be configured with.
pub const WIRES: [&str; 3] = ["openai_responses", "chat_completions", "messages"];
/// How the key travels: `Authorization: Bearer` or Anthropic's `x-api-key`.
pub const AUTH_SCHEMES: [&str; 2] = ["bearer", "x_api_key"];

pub const WIRE_OPENAI_RESPONSES: &str = "openai_responses";
pub const WIRE_CHAT_COMPLETIONS: &str = "chat_completions";
pub const WIRE_MESSAGES: &str = "messages";
pub const AUTH_BEARER: &str = "bearer";
pub const AUTH_X_API_KEY: &str = "x_api_key";

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

/// The vendor's own authentication scheme for the wire.
pub fn default_auth_scheme(wire: &str) -> &'static str {
    match wire {
        WIRE_MESSAGES => AUTH_X_API_KEY,
        _ => AUTH_BEARER,
    }
}

pub fn valid_wire(wire: &str) -> bool {
    WIRES.contains(&wire)
}

pub fn valid_auth_scheme(scheme: &str) -> bool {
    AUTH_SCHEMES.contains(&scheme)
}
