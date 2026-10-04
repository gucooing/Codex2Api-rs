//! Browser consent policy registry; HTTP identity and consent steps are shared.
mod chatgpt;
mod grok;
pub struct OAuthPolicy {
    pub id: &'static str,
    pub client_id: &'static str,
    pub client_name: &'static str,
    pub scopes: &'static str,
    pub callback_path: &'static str,
    pub requires_nonce: bool,
    pub device_flow: &'static str,
}
const POLICIES: &[OAuthPolicy] = &[chatgpt::POLICY, grok::POLICY];
pub fn by_client(client: &str) -> Option<&'static OAuthPolicy> {
    POLICIES.iter().find(|p| p.client_id == client)
}
pub fn by_id(id: &str) -> Option<&'static OAuthPolicy> {
    POLICIES.iter().find(|p| p.id == id)
}
pub fn by_device_flow(flow: &str) -> Option<&'static OAuthPolicy> {
    POLICIES.iter().find(|p| p.device_flow == flow)
}
