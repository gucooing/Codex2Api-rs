use base64::Engine;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::error::{AuthError, Result};
use crate::oauth::OAuthConfig;
use codex2api_accounts::{AuthDotJson, TokenData};

pub use codex2api_accounts::{AuthDotJson as AccountAuthJson, TokenData as AccountTokenData};

pub const AUTH_MODE_CHATGPT: &str = "chatgpt";
const TOKEN_REFRESH_INTERVAL_DAYS: i64 = 8;
const ACCESS_TOKEN_REFRESH_WINDOW_MINUTES: i64 = 5;

/// Subset of official `$CODEX_HOME/auth.json` token payload used by callers.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenSet {
    pub auth_mode: Option<String>,
    pub id_token: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub account_id: Option<String>,
    pub last_refresh: Option<String>,
}

impl TokenSet {
    pub fn from_auth(auth: &AuthDotJson) -> Self {
        token_set_from_auth(auth)
    }
}

/// Tokens returned by the OAuth authorization-code exchange.
#[derive(Debug, Clone)]
pub struct ExchangedTokens {
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
}

impl ExchangedTokens {
    pub fn into_auth_json(&self, chatgpt_account_id: Option<String>) -> AuthDotJson {
        AuthDotJson::chatgpt(
            TokenData {
                id_token: self.id_token.clone(),
                access_token: self.access_token.clone(),
                refresh_token: self.refresh_token.clone(),
                account_id: chatgpt_account_id,
            },
            Some(Utc::now()),
        )
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RefreshResponse {
    pub id_token: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
}

/// Flat subset of useful claims in the ChatGPT id_token JWT.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct IdTokenInfo {
    pub email: Option<String>,
    /// ChatGPT subscription plan type raw value
    /// (e.g. "free", "plus", "pro", "business", "enterprise", "edu").
    pub chatgpt_plan_type: Option<String>,
    pub chatgpt_user_id: Option<String>,
    pub chatgpt_account_id: Option<String>,
    pub chatgpt_account_is_fedramp: bool,
    pub raw_jwt: String,
}

impl IdTokenInfo {
    pub fn plan_type_raw(&self) -> Option<&str> {
        self.chatgpt_plan_type.as_deref()
    }

    pub fn oauth_identity(&self) -> Result<codex2api_accounts::OauthIdentity> {
        let chatgpt_account_id = self
            .chatgpt_account_id
            .clone()
            .filter(|s| !s.trim().is_empty())
            .ok_or(AuthError::MissingChatgptAccountId)?;
        Ok(codex2api_accounts::OauthIdentity {
            chatgpt_account_id,
            chatgpt_user_id: self.chatgpt_user_id.clone(),
            email: self.email.clone(),
            plan_type: self.chatgpt_plan_type.clone(),
            display_name: self.email.clone(),
        })
    }
}

#[derive(Deserialize)]
struct IdClaims {
    #[serde(default)]
    email: Option<String>,
    #[serde(rename = "https://api.openai.com/profile", default)]
    profile: Option<ProfileClaims>,
    #[serde(rename = "https://api.openai.com/auth", default)]
    auth: Option<AuthClaims>,
}

#[derive(Deserialize)]
struct ProfileClaims {
    #[serde(default)]
    email: Option<String>,
}

#[derive(Deserialize)]
struct AuthClaims {
    #[serde(default)]
    chatgpt_plan_type: Option<PlanTypeValue>,
    #[serde(default)]
    chatgpt_user_id: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    chatgpt_account_id: Option<String>,
    #[serde(default)]
    chatgpt_account_is_fedramp: bool,
}

/// Accept either a string plan or an object `{ "plan_type": "pro" }`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum PlanTypeValue {
    String(String),
    Object { plan_type: Option<String> },
}

impl PlanTypeValue {
    fn into_raw(self) -> Option<String> {
        match self {
            Self::String(s) => Some(s),
            Self::Object { plan_type } => plan_type,
        }
    }
}

#[derive(Deserialize)]
struct StandardJwtClaims {
    #[serde(default)]
    exp: Option<i64>,
}

fn decode_jwt_payload<T: DeserializeOwned>(jwt: &str) -> Result<T> {
    let mut parts = jwt.split('.');
    let (_header_b64, payload_b64, _sig_b64) = match (parts.next(), parts.next(), parts.next()) {
        (Some(h), Some(p), Some(s)) if !h.is_empty() && !p.is_empty() && !s.is_empty() => (h, p, s),
        _ => return Err(AuthError::InvalidIdToken),
    };
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload_b64))
        .map_err(|_| AuthError::InvalidIdToken)?;
    Ok(serde_json::from_slice(&payload_bytes)?)
}

pub fn parse_jwt_expiration(jwt: &str) -> Result<Option<DateTime<Utc>>> {
    let claims: StandardJwtClaims = decode_jwt_payload(jwt)?;
    Ok(claims
        .exp
        .and_then(|exp| DateTime::<Utc>::from_timestamp(exp, 0)))
}

/// Read the subscription end date from the stored official ID token.
pub fn parse_chatgpt_subscription_expiration(jwt: &str) -> Result<Option<DateTime<Utc>>> {
    let claims: Value = decode_jwt_payload(jwt)?;
    Ok(claims
        .get("https://api.openai.com/auth")
        .and_then(|auth| auth.get("chatgpt_subscription_active_until"))
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|date| date.with_timezone(&Utc)))
}

pub fn parse_chatgpt_jwt_claims(jwt: &str) -> Result<IdTokenInfo> {
    let claims: IdClaims = decode_jwt_payload(jwt)?;
    let email = claims
        .email
        .or_else(|| claims.profile.and_then(|profile| profile.email));
    match claims.auth {
        Some(auth) => Ok(IdTokenInfo {
            email,
            raw_jwt: jwt.to_string(),
            chatgpt_plan_type: auth.chatgpt_plan_type.and_then(PlanTypeValue::into_raw),
            chatgpt_user_id: auth.chatgpt_user_id.or(auth.user_id),
            chatgpt_account_id: auth.chatgpt_account_id,
            chatgpt_account_is_fedramp: auth.chatgpt_account_is_fedramp,
        }),
        None => Ok(IdTokenInfo {
            email,
            raw_jwt: jwt.to_string(),
            chatgpt_plan_type: None,
            chatgpt_user_id: None,
            chatgpt_account_id: None,
            chatgpt_account_is_fedramp: false,
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenEndpointErrorDetail {
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub display_message: String,
}

pub fn parse_token_endpoint_error(body: &str) -> TokenEndpointErrorDetail {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return TokenEndpointErrorDetail {
            error_code: None,
            error_message: None,
            display_message: "unknown error".to_string(),
        };
    }
    let parsed = serde_json::from_str::<Value>(trimmed).ok();
    if let Some(json) = parsed {
        let error_code = json
            .get("error")
            .and_then(Value::as_str)
            .filter(|error_code| !error_code.trim().is_empty())
            .map(ToString::to_string)
            .or_else(|| {
                json.get("error")
                    .and_then(Value::as_object)
                    .and_then(|error_obj| error_obj.get("code"))
                    .and_then(Value::as_str)
                    .filter(|code| !code.trim().is_empty())
                    .map(ToString::to_string)
            });
        if let Some(description) = json.get("error_description").and_then(Value::as_str)
            && !description.trim().is_empty()
        {
            return TokenEndpointErrorDetail {
                error_code,
                error_message: Some(description.to_string()),
                display_message: description.to_string(),
            };
        }
        if let Some(error_obj) = json.get("error")
            && let Some(message) = error_obj.get("message").and_then(Value::as_str)
            && !message.trim().is_empty()
        {
            return TokenEndpointErrorDetail {
                error_code,
                error_message: Some(message.to_string()),
                display_message: message.to_string(),
            };
        }
        if let Some(code) = error_code {
            return TokenEndpointErrorDetail {
                error_code: Some(code.clone()),
                error_message: None,
                display_message: code,
            };
        }
    }
    TokenEndpointErrorDetail {
        error_code: None,
        error_message: None,
        display_message: trimmed.to_string(),
    }
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn should_refresh(auth: &AuthDotJson) -> bool {
    if let Some(tokens) = auth.tokens.as_ref()
        && !tokens.access_token.is_empty()
        && let Ok(Some(expires_at)) = parse_jwt_expiration(&tokens.access_token)
    {
        return expires_at
            <= Utc::now() + chrono::Duration::minutes(ACCESS_TOKEN_REFRESH_WINDOW_MINUTES);
    }
    let Some(last_refresh) = auth.last_refresh else {
        return false;
    };
    last_refresh < Utc::now() - chrono::Duration::days(TOKEN_REFRESH_INTERVAL_DAYS)
}

pub fn apply_refresh(auth: &mut AuthDotJson, refresh: &RefreshResponse) -> Result<()> {
    let tokens = auth.tokens.get_or_insert_with(TokenData::default);
    if let Some(id_token) = refresh.id_token.as_ref() {
        tokens.id_token = id_token.clone();
        if let Ok(claims) = parse_chatgpt_jwt_claims(id_token)
            && let Some(account_id) = claims.chatgpt_account_id
        {
            tokens.account_id = Some(account_id);
        }
    }
    if let Some(access_token) = refresh.access_token.as_ref() {
        tokens.access_token = access_token.clone();
    }
    if let Some(refresh_token) = refresh.refresh_token.as_ref() {
        tokens.refresh_token = refresh_token.clone();
    }
    auth.last_refresh = Some(Utc::now());
    Ok(())
}

pub fn token_set_from_auth(auth: &AuthDotJson) -> TokenSet {
    let tokens = auth.tokens.as_ref();
    TokenSet {
        auth_mode: auth.auth_mode.clone(),
        id_token: tokens.map(|t| t.id_token.clone()),
        access_token: tokens.map(|t| t.access_token.clone()),
        refresh_token: tokens.map(|t| t.refresh_token.clone()),
        account_id: tokens.and_then(|t| t.account_id.clone()),
        last_refresh: auth
            .last_refresh
            .map(|dt| dt.to_rfc3339_opts(SecondsFormat::Millis, true)),
    }
}

/// HTTP client for OAuth token *exchange* (official `create_raw_auth_client`).
/// No Codex originator / User-Agent — the official CLI omits them on this call.
pub fn default_http_client() -> Result<reqwest::Client> {
    Ok(crate::transport::http_builder()?
        .timeout(std::time::Duration::from_secs(30))
        .build()?)
}

/// HTTP client for token *refresh* (official `create_client` / `default_headers`).
/// `user_agent` must be the account's frozen official CLI User-Agent.
pub fn refresh_http_client(user_agent: &str) -> Result<reqwest::Client> {
    Ok(crate::transport::http_builder()?
        .timeout(std::time::Duration::from_secs(30))
        .default_headers({
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                "originator",
                reqwest::header::HeaderValue::from_static(codex2api_version::DEFAULT_ORIGINATOR),
            );
            if let Ok(ua) = reqwest::header::HeaderValue::from_str(user_agent) {
                headers.insert(reqwest::header::USER_AGENT, ua);
            }
            headers
        })
        .build()?)
}

/// Refresh using a User-Agent frozen on the account row (official CLI UA).
pub async fn refresh_chatgpt_tokens(
    cfg: &OAuthConfig,
    refresh_token: &str,
    user_agent: &str,
) -> Result<RefreshResponse> {
    let http = refresh_http_client(user_agent)?;
    crate::oauth::refresh_tokens(&http, cfg, refresh_token).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_jwt(payload: serde_json::Value) -> String {
        #[derive(Serialize)]
        struct Header {
            alg: &'static str,
            typ: &'static str,
        }
        let header = Header {
            alg: "none",
            typ: "JWT",
        };
        fn b64url_no_pad(bytes: &[u8]) -> String {
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
        }
        let header_b64 = b64url_no_pad(&serde_json::to_vec(&header).unwrap());
        let payload_b64 = b64url_no_pad(&serde_json::to_vec(&payload).unwrap());
        let signature_b64 = b64url_no_pad(b"sig");
        format!("{header_b64}.{payload_b64}.{signature_b64}")
    }

    #[test]
    fn subscription_expiration_uses_the_subscription_claim_only() {
        let jwt = fake_jwt(serde_json::json!({
            "exp": 2_000_000_000,
            "https://api.openai.com/auth": {
                "chatgpt_subscription_active_until": "2026-10-17T08:00:00+08:00"
            }
        }));
        assert_eq!(
            parse_chatgpt_subscription_expiration(&jwt).unwrap(),
            Some(
                DateTime::parse_from_rfc3339("2026-10-17T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );
        for value in [
            Value::Null,
            serde_json::json!("invalid"),
            serde_json::json!(123),
        ] {
            let jwt = fake_jwt(serde_json::json!({
                "exp": 2_000_000_000,
                "https://api.openai.com/auth": {"chatgpt_subscription_active_until": value}
            }));
            assert_eq!(parse_chatgpt_subscription_expiration(&jwt).unwrap(), None);
        }
        let jwt = fake_jwt(serde_json::json!({"exp": 2_000_000_000}));
        assert_eq!(parse_chatgpt_subscription_expiration(&jwt).unwrap(), None);
    }

    #[test]
    fn id_token_info_parses_email_plan_and_ids() {
        let jwt = fake_jwt(serde_json::json!({
            "email": "user@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_plan_type": "pro",
                "chatgpt_user_id": "user_123",
                "chatgpt_account_id": "acct_456"
            }
        }));
        let info = parse_chatgpt_jwt_claims(&jwt).expect("should parse");
        assert_eq!(info.email.as_deref(), Some("user@example.com"));
        assert_eq!(info.chatgpt_plan_type.as_deref(), Some("pro"));
        assert_eq!(info.chatgpt_user_id.as_deref(), Some("user_123"));
        assert_eq!(info.chatgpt_account_id.as_deref(), Some("acct_456"));
    }

    #[test]
    fn id_token_info_falls_back_to_profile_email_and_user_id() {
        let jwt = fake_jwt(serde_json::json!({
            "https://api.openai.com/profile": { "email": "profile@example.com" },
            "https://api.openai.com/auth": {
                "user_id": "legacy_user",
                "chatgpt_account_id": "acct_1"
            }
        }));
        let info = parse_chatgpt_jwt_claims(&jwt).expect("should parse");
        assert_eq!(info.email.as_deref(), Some("profile@example.com"));
        assert_eq!(info.chatgpt_user_id.as_deref(), Some("legacy_user"));
    }

    #[test]
    fn parse_token_endpoint_error_prefers_error_description() {
        let detail = parse_token_endpoint_error(
            r#"{"error":"invalid_grant","error_description":"refresh token expired"}"#,
        );
        assert_eq!(detail.error_code.as_deref(), Some("invalid_grant"));
        assert_eq!(detail.display_message, "refresh token expired");
    }

    #[test]
    fn parse_token_endpoint_error_reads_nested_error() {
        let detail = parse_token_endpoint_error(
            r#"{"error":{"code":"proxy_auth_required","message":"proxy authentication required"}}"#,
        );
        assert_eq!(detail.error_code.as_deref(), Some("proxy_auth_required"));
        assert_eq!(detail.display_message, "proxy authentication required");
    }
}
