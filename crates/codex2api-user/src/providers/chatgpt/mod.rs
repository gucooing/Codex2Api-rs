use super::OAuthPolicy;
pub const POLICY: OAuthPolicy = OAuthPolicy {
    id: codex2api_core::CHATGPT,
    client_id: codex2api_version::OAUTH_CLIENT_ID,
    client_name: "Codex",
    scopes: codex2api_version::OAUTH_SCOPE,
    callback_path: "/auth/callback",
    requires_nonce: false,
    device_flow: "device",
};
