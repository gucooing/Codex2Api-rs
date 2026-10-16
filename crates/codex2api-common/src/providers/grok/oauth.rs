use crate::oauth::OAuthPolicy;
pub const POLICY: OAuthPolicy = OAuthPolicy {
    id: crate::GROK,
    client_id: crate::version::grok::CLIENT_ID,
    client_name: "Grok Build",
    scopes: crate::version::grok::SCOPE,
    callback_path: "/callback",
    requires_nonce: true,
    device_flow: "device-grok",
};
