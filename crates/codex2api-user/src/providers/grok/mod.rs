use super::OAuthPolicy;
pub const POLICY: OAuthPolicy = OAuthPolicy {
    id: codex2api_core::GROK,
    client_id: codex2api_version::grok::CLIENT_ID,
    client_name: "Grok Build",
    scopes: codex2api_version::grok::SCOPE,
    callback_path: "/callback",
    requires_nonce: true,
    device_flow: "device-grok",
};
