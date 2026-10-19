use codex2api_storage::SupplierTokens;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Google OAuth tokens plus the Cloud Code Assist project the account executes under.
#[derive(Clone, Serialize, Deserialize)]
pub struct AntigravityCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub project_id: Option<String>,
    /// Official tier identifier from loadCodeAssist, such as `g1-pro-tier`.
    pub tier: Option<String>,
    /// Google userinfo of the owner (id, email, name).
    pub profile: Value,
}
impl AntigravityCredentials {
    pub fn from_supplier_tokens(tokens: &SupplierTokens) -> super::Result<Self> {
        Ok(serde_json::from_str(
            tokens
                .raw_auth_json
                .as_deref()
                .ok_or_else(|| super::problem("Antigravity 凭据缺失"))?,
        )?)
    }
    pub fn to_supplier_tokens(&self, id: &str) -> SupplierTokens {
        SupplierTokens {
            account_id: id.into(),
            auth_mode: Some(codex2api_common::ANTIGRAVITY.into()),
            access_token: Some(self.access_token.clone()),
            id_token: None,
            refresh_token: self.refresh_token.clone(),
            last_refresh: Some(chrono::Utc::now().to_rfc3339()),
            raw_auth_json: Some(
                serde_json::to_string(self).expect("Antigravity credentials serialize"),
            ),
            updated_at: String::new(),
        }
    }
    pub fn paid(&self) -> bool {
        self.tier
            .as_deref()
            .is_some_and(codex2api_common::version::antigravity::paid_tier)
    }
}
