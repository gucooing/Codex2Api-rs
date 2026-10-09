use crate::{Result, Storage, TokenPurpose};
use serde_json::json;

pub const WEB_ACCESS_TTL_SECONDS: u32 = 15 * 60;
pub const WEB_REFRESH_TTL_SECONDS: u32 = 30 * 24 * 60 * 60;
pub(crate) const REFRESH_GRACE_SECONDS: i64 = 30;

pub struct WebSessionTokens {
    pub access_token: String,
    pub refresh_token: String,
}

impl Storage {
    pub(crate) async fn web_session_tokens(
        &self,
        access_purpose: TokenPurpose,
        refresh_purpose: TokenPurpose,
        owner: &str,
        id: &str,
        expires: i64,
        refresh: (i64, i64),
    ) -> Result<WebSessionTokens> {
        let now = chrono::Utc::now().timestamp();
        let access_token = self
            .sign_jwt(
                access_purpose,
                json!({
                    "sub":owner,"jti":id,"iat":now,
                    "exp":expires.min(now + i64::from(WEB_ACCESS_TTL_SECONDS))
                }),
            )
            .await?;
        let refresh_token = self
            .sign_jwt(
                refresh_purpose,
                json!({
                    "sub":owner,"jti":id,"iat":refresh.1,"exp":expires,"version":refresh.0
                }),
            )
            .await?;
        Ok(WebSessionTokens {
            access_token,
            refresh_token,
        })
    }
}
