use crate::oauth::OAuthPolicy;
pub const POLICY: OAuthPolicy = OAuthPolicy {
    id: crate::CHATGPT,
    client_id: crate::version::OAUTH_CLIENT_ID,
    client_name: "Codex",
    scopes: crate::version::OAUTH_SCOPE,
    callback_path: super::version::OAUTH_CALLBACK_PATH,
    requires_nonce: false,
    device_flow: "device",
};
