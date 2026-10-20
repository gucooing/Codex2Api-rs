//! An API-key account's credentials: the key, where it is accepted, which wire the
//! endpoint speaks, how the key is sent and which statuses retry the same account.
//! Stored as `supplier_tokens.raw_auth_json` with the key mirrored in `access_token`
//! for the pool's authorization check.
use super::{ApiAuthError, Result};
use codex2api_common::providers::api as channel;
use codex2api_storage::SupplierTokens;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiCredentials {
    pub api_key: String,
    pub base_url: String,
    pub wire: String,
    pub auth_scheme: String,
    /// HTTP statuses the administrator wants retried on the same account before it
    /// is avoided, for endpoints known to fail intermittently.
    #[serde(default)]
    pub retry_statuses: Vec<u16>,
    /// How many times one attempt retries the same account on those statuses.
    pub retry_count: u32,
}

impl ApiCredentials {
    /// Trim and check every field against the channel's contract.
    pub fn validate(&mut self) -> Result<()> {
        self.api_key = self.api_key.trim().to_owned();
        if self.api_key.is_empty()
            || self.api_key.len() > 4096
            || self
                .api_key
                .bytes()
                .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
        {
            return Err(ApiAuthError::Invalid(
                "API Key 须为不含空白字符的单行文本".into(),
            ));
        }
        self.base_url = normalize_base_url(&self.base_url)?;
        if !channel::valid_wire(&self.wire) {
            return Err(ApiAuthError::Invalid("不支持的上游线格式".into()));
        }
        if !channel::valid_auth_scheme(&self.wire, &self.auth_scheme) {
            return Err(ApiAuthError::Invalid(
                if self.wire == channel::WIRE_MESSAGES {
                    "Anthropic Messages 只支持 x-api-key 或 Authorization: Bearer 认证头"
                } else {
                    "OpenAI 线格式只支持 Authorization: Bearer 认证头"
                }
                .into(),
            ));
        }
        self.retry_statuses.sort_unstable();
        self.retry_statuses.dedup();
        if self.retry_statuses.len() > 16
            || self
                .retry_statuses
                .iter()
                .any(|status| !(400..=599).contains(status))
        {
            return Err(ApiAuthError::Invalid(
                "同号重试状态码须为 400 到 599 之间的 HTTP 状态".into(),
            ));
        }
        if !(1..=channel::MAX_RETRY_COUNT).contains(&self.retry_count) {
            return Err(ApiAuthError::Invalid(format!(
                "同号重试次数须为 1 到 {} 的整数",
                channel::MAX_RETRY_COUNT
            )));
        }
        Ok(())
    }

    pub fn from_supplier_tokens(tokens: &SupplierTokens) -> Result<Self> {
        let raw = tokens
            .raw_auth_json
            .as_deref()
            .ok_or_else(|| ApiAuthError::TokensNotFound("credentials missing".into()))?;
        let mut credentials: Self = serde_json::from_str(raw)?;
        credentials.validate()?;
        Ok(credentials)
    }

    pub fn to_supplier_tokens(&self, id: &str) -> SupplierTokens {
        SupplierTokens {
            account_id: id.into(),
            auth_mode: Some(codex2api_common::API.into()),
            access_token: Some(self.api_key.clone()),
            id_token: None,
            refresh_token: None,
            last_refresh: Some(chrono::Utc::now().to_rfc3339()),
            raw_auth_json: Some(serde_json::to_string(self).expect("API credentials serialize")),
            updated_at: String::new(),
        }
    }

    /// A stable, non-reversible identity of the key for duplicate detection.
    pub fn key_fingerprint(&self) -> String {
        let digest = Sha256::digest(self.api_key.as_bytes());
        digest.iter().take(16).map(|b| format!("{b:02x}")).collect()
    }

    /// The last characters of the key, the only part an administrator sees again.
    pub fn key_suffix(&self) -> String {
        let chars: Vec<char> = self.api_key.chars().collect();
        let keep = chars.len().min(4);
        chars[chars.len() - keep..].iter().collect()
    }

    /// The full URL of an endpoint path under the Base URL.
    pub fn endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    pub fn wire_path(&self) -> &'static str {
        channel::wire_path(&self.wire).expect("validated wire")
    }
}

/// An absolute http(s) URL without query, fragment or credentials, trailing slashes
/// removed. The administrator chooses whether `/v1` is part of it.
pub fn normalize_base_url(value: &str) -> Result<String> {
    let value = value.trim();
    let url = url::Url::parse(value).map_err(|_| ApiAuthError::Invalid("Base URL 无效".into()))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none_or(str::is_empty)
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || value.len() > 2048
    {
        return Err(ApiAuthError::Invalid(
            "Base URL 须为不含查询参数与凭据的 http(s) 地址".into(),
        ));
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_normalize_and_reject_invalid_inputs() {
        let mut credentials = ApiCredentials {
            api_key: " sk-test-1234 ".into(),
            base_url: "https://api.example.com/v1/".into(),
            wire: "chat_completions".into(),
            auth_scheme: "bearer".into(),
            retry_statuses: vec![502, 502, 500],
            retry_count: 2,
        };
        credentials.validate().unwrap();
        assert_eq!(credentials.api_key, "sk-test-1234");
        assert_eq!(credentials.base_url, "https://api.example.com/v1");
        assert_eq!(credentials.retry_statuses, vec![500, 502]);
        assert_eq!(
            credentials.endpoint("chat/completions"),
            "https://api.example.com/v1/chat/completions"
        );
        assert_eq!(credentials.key_suffix(), "1234");
        assert_eq!(credentials.key_fingerprint().len(), 32);
        for (field, value) in [
            ("base_url", "ftp://x"),
            ("base_url", "https://user:pw@host/v1"),
            ("base_url", "https://host/v1?x=1"),
            ("wire", "gemini"),
            ("auth_scheme", "basic"),
            // The OpenAI wires accept only a Bearer token.
            ("auth_scheme", "x_api_key"),
            ("api_key", "a b"),
        ] {
            let mut broken = credentials.clone();
            match field {
                "base_url" => broken.base_url = value.into(),
                "wire" => broken.wire = value.into(),
                "auth_scheme" => broken.auth_scheme = value.into(),
                _ => broken.api_key = value.into(),
            }
            assert!(broken.validate().is_err(), "{field}={value}");
        }
        let mut statuses = credentials.clone();
        statuses.retry_statuses = vec![200];
        assert!(statuses.validate().is_err());
        for count in [0, channel::MAX_RETRY_COUNT + 1] {
            let mut retries = credentials.clone();
            retries.retry_count = count;
            assert!(retries.validate().is_err(), "{count}");
        }
    }
}
