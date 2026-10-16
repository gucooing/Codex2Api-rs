//! Provider authorization contracts and validation shared with browser consent.
use crate::providers::{chatgpt::oauth as chatgpt, grok::oauth as grok};
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

use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct AuthorizationRequest {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub state: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub nonce: Option<String>,
}
impl AuthorizationRequest {
    pub fn provider(&self) -> &'static str {
        crate::oauth::by_client(&self.client_id).map_or("", |p| p.id)
    }
    fn allowed_scopes(&self) -> &'static str {
        crate::oauth::by_client(&self.client_id).map_or("", |p| p.scopes)
    }
    pub fn valid(&self) -> bool {
        let Ok(uri) = url::Url::parse(&self.redirect_uri) else {
            return false;
        };
        self.response_type == "code"
            && crate::oauth::by_client(&self.client_id).is_some_and(|p| {
                (!p.requires_nonce
                    || self.nonce.as_deref().is_some_and(|s| {
                        !s.is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control)
                    }))
                    && uri.path() == p.callback_path
            })
            && self.code_challenge_method == "S256"
            && self.code_challenge.len() == 43
            && self
                .code_challenge
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            && !self.state.is_empty()
            && self.state.len() <= 1024
            && !self.state.chars().any(char::is_control)
            && self
                .scope
                .split_whitespace()
                .all(|s| self.allowed_scopes().split_whitespace().any(|a| a == s))
            && uri.scheme() == "http"
            && matches!(uri.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
            && uri.port().is_some_and(|p| p != 0)
            && uri.username().is_empty()
            && uri.password().is_none()
            && uri.query().is_none()
            && uri.fragment().is_none()
    }
    pub fn scopes(&self) -> String {
        let requested = if self.scope.trim().is_empty() {
            "openid profile email offline_access"
        } else {
            &self.scope
        };
        self.allowed_scopes()
            .split_whitespace()
            .filter(|s| requested.split_whitespace().any(|v| v == *s))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(policy: &OAuthPolicy) -> AuthorizationRequest {
        AuthorizationRequest {
            response_type: "code".into(),
            client_id: policy.client_id.into(),
            redirect_uri: format!("http://127.0.0.1:1455{}", policy.callback_path),
            state: "keep+exact/state=value".into(),
            code_challenge: "A".repeat(43),
            code_challenge_method: "S256".into(),
            scope: policy.scopes.into(),
            nonce: Some("nonce".into()),
        }
    }

    #[test]
    fn authorization_rejects_foreign_callbacks_and_invalid_pkce() {
        for policy in POLICIES {
            let valid = request(policy);
            assert!(valid.valid());
            for callback in [
                "https://127.0.0.1:1455/auth/callback",
                "http://localhost.evil:1455/auth/callback",
                "http://user@localhost:1455/auth/callback",
                "http://127.0.0.1/auth/callback",
                "http://127.0.0.1:1455/wrong",
                "http://127.0.0.1:1455/auth/callback?next=external",
            ] {
                let mut value = valid.clone();
                value.redirect_uri = callback.into();
                assert!(!value.valid(), "{callback}");
            }
            for challenge in ["", "short", &"!".repeat(43)] {
                let mut value = valid.clone();
                value.code_challenge = challenge.into();
                assert!(!value.valid());
            }
            let mut value = valid.clone();
            value.code_challenge_method = "plain".into();
            assert!(!value.valid());
            value = valid.clone();
            value.state = "bad\nstate".into();
            assert!(!value.valid());
            value = valid;
            value.scope.push_str(" unauthorized:scope");
            assert!(!value.valid());
        }
    }

    #[test]
    fn authorization_keeps_provider_specific_scopes_and_nonce_rules() {
        for policy in POLICIES {
            let mut value = request(policy);
            assert_eq!(value.provider(), policy.id);
            assert_eq!(value.scopes(), policy.scopes);
            value.nonce = None;
            assert_eq!(value.valid(), !policy.requires_nonce);
            value.client_id = "unknown-client".into();
            assert!(!value.valid());
        }
    }
}
