//! Fixed client identity of the API-key channel. Every API-key account sends the same
//! User-Agent; there is no per-account device fingerprint to freeze.

/// The User-Agent of every API-key request, from this process's own version.
pub const USER_AGENT: &str = concat!("codex2api/", env!("CARGO_PKG_VERSION"));

/// Anthropic Messages API version sent with the `x_api_key` scheme.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
pub const ANTHROPIC_VERSION_HEADER: &str = "anthropic-version";
pub const X_API_KEY_HEADER: &str = "x-api-key";

/// Path of the model listing every wire's vendor serves under the Base URL.
pub const MODELS_PATH: &str = "models";
