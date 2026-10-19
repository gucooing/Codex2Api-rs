//! Antigravity supplier device identity: Go-style platform tokens of the desktop client.
use codex2api_storage::SupplierAccount;
use serde::{Deserialize, Serialize};

pub const OS_TYPES: [&str; 3] = ["windows", "darwin", "linux"];
pub const ARCHS: [&str; 2] = ["amd64", "arm64"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AntigravityIdentity {
    pub account_id: String,
    pub installation_id: String,
    pub os_type: String,
    pub os_version: String,
    pub arch: String,
    pub timezone: Option<String>,
}
impl AntigravityIdentity {
    /// Random per account, then frozen. Never derived from the proxy host.
    pub fn generate() -> Self {
        let profiles = [
            ("windows", "10.0.26100", "amd64"),
            ("windows", "10.0.22631", "amd64"),
            ("darwin", "15.6.0", "arm64"),
            ("darwin", "14.7.1", "amd64"),
            ("linux", "6.8.0", "amd64"),
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
        Self {
            account_id,
            installation_id,
            os_type,
            os_version,
            arch,
            timezone,
        }
    }
    pub fn user_agent(&self) -> String {
        codex2api_common::version::antigravity::user_agent(&self.os_type, &self.arch)
    }
    pub fn from_account(account: &SupplierAccount) -> serde_json::Result<Self> {
        let mut identity: Self = serde_json::from_str(&account.http_fingerprint_json)?;
        identity.account_id = account.id.clone();
        Ok(identity)
    }
    pub fn from_transport(
        account: &codex2api_storage::SupplierTransportSnapshot,
    ) -> serde_json::Result<Self> {
        let mut identity: Self = serde_json::from_str(&account.http_fingerprint_json)?;
        identity.account_id = account.id.clone();
        Ok(identity)
    }
    pub fn fingerprint_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}
