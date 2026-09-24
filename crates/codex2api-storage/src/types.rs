use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::error::{Result, StorageError};

/// SupplierAccount lifecycle stored in `accounts.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SupplierStatus {
    Pending,
    Active,
    Disabled,
}

impl SupplierStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }
}

impl fmt::Display for SupplierStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SupplierStatus {
    type Err = StorageError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "pending" => Ok(Self::Pending),
            "active" => Ok(Self::Active),
            "disabled" => Ok(Self::Disabled),
            other => Err(StorageError::InvalidAccountStatus(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AdminUser {
    pub id: i64,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AdminSession {
    pub id: String,
    pub admin_user_id: i64,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierAccount {
    pub provider_id: String,
    pub id: String,
    pub proxy_id: Option<String>,
    pub status: SupplierStatus,
    pub display_name: Option<String>,
    pub chatgpt_account_id: Option<String>,
    pub chatgpt_user_id: Option<String>,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub installation_id: String,
    pub originator: String,
    pub user_agent: String,
    pub os_type: String,
    pub os_version: String,
    pub arch: String,
    pub home_dir: String,
    /// Official Codex CLI application-layer HTTP fingerprint, frozen per account.
    pub http_fingerprint_json: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}

/// Fields required to insert an account row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSupplierAccount {
    pub provider_id: String,
    pub id: Option<String>,
    pub status: SupplierStatus,
    pub display_name: Option<String>,
    pub chatgpt_account_id: Option<String>,
    pub chatgpt_user_id: Option<String>,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub installation_id: String,
    pub originator: String,
    pub user_agent: String,
    pub os_type: String,
    pub os_version: String,
    pub arch: String,
    pub home_dir: String,
    pub http_fingerprint_json: String,
}

impl NewSupplierAccount {
    // The frozen supplier identity is persisted as these independent fields.
    #[allow(clippy::too_many_arguments)]
    pub fn pending_identity(
        installation_id: impl Into<String>,
        originator: impl Into<String>,
        user_agent: impl Into<String>,
        os_type: impl Into<String>,
        os_version: impl Into<String>,
        arch: impl Into<String>,
        home_dir: impl Into<String>,
        http_fingerprint_json: impl Into<String>,
    ) -> Self {
        Self {
            provider_id: codex2api_core::CHATGPT.into(),
            id: None,
            status: SupplierStatus::Pending,
            display_name: None,
            chatgpt_account_id: None,
            chatgpt_user_id: None,
            email: None,
            plan_type: None,
            installation_id: installation_id.into(),
            originator: originator.into(),
            user_agent: user_agent.into(),
            os_type: os_type.into(),
            os_version: os_version.into(),
            arch: arch.into(),
            home_dir: home_dir.into(),
            http_fingerprint_json: http_fingerprint_json.into(),
        }
    }
}

/// Partial update. `Some` replaces the stored value; `None` leaves it unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SupplierAccountUpdate {
    pub status: Option<SupplierStatus>,
    pub display_name: Option<String>,
    pub chatgpt_account_id: Option<String>,
    pub chatgpt_user_id: Option<String>,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub originator: Option<String>,
    pub user_agent: Option<String>,
    pub os_type: Option<String>,
    pub os_version: Option<String>,
    pub arch: Option<String>,
    pub home_dir: Option<String>,
    pub http_fingerprint_json: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, FromRow)]
pub struct SupplierTokens {
    pub account_id: String,
    pub auth_mode: Option<String>,
    pub id_token: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub last_refresh: Option<String>,
    pub raw_auth_json: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OAuthPending {
    pub state: String,
    pub code_verifier: String,
    pub redirect_uri: String,
    pub account_id: Option<String>,
    pub created_at: String,
    pub expires_at: String,
    pub flow_data_json: String,
    pub last_polled_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, FromRow)]
pub struct SupplierRuntime {
    pub account_id: String,
    pub session_id: Option<String>,
    pub extra_json: Option<String>,
    pub updated_at: String,
}

impl SupplierRuntime {
    pub fn extra_value(&self) -> Result<Option<serde_json::Value>> {
        match &self.extra_json {
            Some(raw) => Ok(Some(serde_json::from_str(raw)?)),
            None => Ok(None),
        }
    }
}
