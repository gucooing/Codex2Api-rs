//! Grok supplier device identity. No Codex headers, token types or home directories.
use codex2api_storage::SupplierAccount;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrokIdentity {
    pub account_id: String,
    pub installation_id: String,
    pub originator: String,
    pub user_agent: String,
    pub os_type: String,
    pub os_version: String,
    pub arch: String,
    pub timezone: Option<String>,
}
impl GrokIdentity {
    pub fn generate() -> Self {
        let profiles = [
            ("Windows", "10.0.26100", "x86_64"),
            ("Windows", "10.0.26200", "aarch64"),
            ("Mac OS", "15.6.0", "aarch64"),
            ("Ubuntu", "24.4.0", "x86_64"),
            ("Ubuntu", "24.4.0", "aarch64"),
        ];
        let (os, version, arch) = profiles[rand::random_range(0..profiles.len())];
        Self::new(
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
            os.into(),
            version.into(),
            arch.into(),
            None,
        )
    }
    pub fn new(
        account_id: String,
        installation_id: String,
        os_type: String,
        os_version: String,
        arch: String,
        timezone: Option<String>,
    ) -> Self {
        let user_agent = codex2api_version::grok::user_agent(&os_type, &arch);
        Self {
            account_id,
            installation_id,
            originator: "grok-pager".into(),
            user_agent,
            os_type,
            os_version,
            arch,
            timezone,
        }
    }
    pub fn from_account(account: &SupplierAccount) -> Self {
        let timezone = serde_json::from_str::<serde_json::Value>(&account.http_fingerprint_json)
            .ok()
            .and_then(|v| v["timezone"].as_str().map(str::to_owned));
        Self::new(
            account.id.clone(),
            account.installation_id.clone(),
            account.os_type.clone(),
            account.os_version.clone(),
            account.arch.clone(),
            timezone,
        )
    }
    pub fn fingerprint_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}
