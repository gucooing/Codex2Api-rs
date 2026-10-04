use codex2api_storage::SupplierTokens;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Serialize, Deserialize)]
pub struct GrokCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub expires_at: Option<i64>,
    pub profile: Value,
}
impl GrokCredentials {
    pub fn from_supplier_tokens(tokens: &SupplierTokens) -> super::Result<Self> {
        Ok(serde_json::from_str(
            tokens
                .raw_auth_json
                .as_deref()
                .ok_or_else(|| super::problem("Grok 凭据缺失"))?,
        )?)
    }
    pub fn to_supplier_tokens(&self, id: &str) -> SupplierTokens {
        SupplierTokens {
            account_id: id.into(),
            auth_mode: Some("grok".into()),
            access_token: Some(self.access_token.clone()),
            id_token: self.id_token.clone(),
            refresh_token: self.refresh_token.clone(),
            last_refresh: Some(chrono::Utc::now().to_rfc3339()),
            raw_auth_json: Some(serde_json::to_string(self).expect("Grok credentials serialize")),
            updated_at: String::new(),
        }
    }
}
