use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use codex2api_storage::AccountTokens;

use crate::error::Result;

/// In-memory ChatGPT token payload. Shape matches official `auth.json` so
/// request/token fields stay compatible; persistence is SQLite `account_tokens`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct AuthDotJson {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<String>,

    #[serde(
        rename = "OPENAI_API_KEY",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub openai_api_key: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenData>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<DateTime<Utc>>,

    #[serde(flatten, default)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TokenData {
    #[serde(default)]
    pub id_token: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
}

impl AuthDotJson {
    pub fn chatgpt(tokens: TokenData, last_refresh: Option<DateTime<Utc>>) -> Self {
        Self {
            auth_mode: Some("chatgpt".to_string()),
            openai_api_key: None,
            tokens: Some(tokens),
            last_refresh,
            extra: serde_json::Map::new(),
        }
    }

    pub fn chatgpt_account_id(&self) -> Option<&str> {
        self.tokens
            .as_ref()
            .and_then(|t| t.account_id.as_deref())
            .filter(|s| !s.is_empty())
    }

    pub fn to_account_tokens(&self, account_id: &str) -> AccountTokens {
        let nested = self.tokens.as_ref();
        AccountTokens {
            account_id: account_id.to_string(),
            auth_mode: self.auth_mode.clone(),
            id_token: nested
                .map(|t| t.id_token.clone())
                .filter(|s| !s.is_empty()),
            access_token: nested
                .map(|t| t.access_token.clone())
                .filter(|s| !s.is_empty()),
            refresh_token: nested
                .map(|t| t.refresh_token.clone())
                .filter(|s| !s.is_empty()),
            last_refresh: self
                .last_refresh
                .map(|dt| dt.to_rfc3339_opts(SecondsFormat::Millis, true)),
            raw_auth_json: serde_json::to_string(self).ok(),
            updated_at: String::new(),
        }
    }

    pub fn from_account_tokens(tokens: &AccountTokens) -> Result<Self> {
        if let Some(raw) = tokens.raw_auth_json.as_deref() {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return Ok(serde_json::from_str(trimmed)?);
            }
        }

        let nested = match (
            tokens.id_token.as_deref(),
            tokens.access_token.as_deref(),
            tokens.refresh_token.as_deref(),
        ) {
            (None, None, None) => None,
            _ => Some(TokenData {
                id_token: tokens.id_token.clone().unwrap_or_default(),
                access_token: tokens.access_token.clone().unwrap_or_default(),
                refresh_token: tokens.refresh_token.clone().unwrap_or_default(),
                account_id: None,
            }),
        };

        let last_refresh = tokens.last_refresh.as_deref().and_then(|s| {
            DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        });

        Ok(Self {
            auth_mode: tokens.auth_mode.clone(),
            openai_api_key: None,
            tokens: nested,
            last_refresh,
            extra: serde_json::Map::new(),
        })
    }
}
