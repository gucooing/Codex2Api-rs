use thiserror::Error;

use codex2api_accounts::AccountError;
use codex2api_auth::AuthError;

pub type Result<T> = std::result::Result<T, UpstreamError>;

/// Errors talking to official Codex `/responses`.
#[derive(Debug, Error)]
pub enum UpstreamError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("request body exceeds the configured size limit")]
    RequestTooLarge,
    #[error("unsupported request Content-Encoding")]
    UnsupportedEncoding,
    #[error("request encoding failed: {0}")]
    Encoding(#[from] std::io::Error),
    #[error("account `{0}` has no access token")]
    MissingAccessToken(String),
    #[error("upstream returned 401 unauthorized")]
    Unauthorized,
    #[error("failed to refresh access token: {0}")]
    Refresh(#[source] AuthError),
    // The protocol body is kept intact; Display is deliberately content-free.
    // Callers record a separate bounded, redacted diagnostic in the usage ledger.
    #[error("upstream returned HTTP {status}")]
    Status {
        status: u16,
        body: String,
        headers: http::HeaderMap,
    },
    #[error("invalid header value: {0}")]
    InvalidHeader(#[from] http::header::InvalidHeaderValue),
    #[error("SSE stream error: {0}")]
    Stream(String),
    #[error("SSE stream idle timeout")]
    StreamIdleTimeout,
    #[error("supplier workspace routing failed: {0}")]
    WorkspaceRouting(String),
    #[error("supplier credentials or workspace routing changed; reconnect and retry")]
    WorkspaceChanged,
    #[error(transparent)]
    Storage(#[from] codex2api_storage::StorageError),
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    SupplierAccount(#[from] AccountError),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl UpstreamError {
    pub fn status(status: reqwest::StatusCode, body: impl Into<String>) -> Self {
        Self::status_with_headers(status, body, http::HeaderMap::new())
    }

    pub fn status_with_headers(
        status: reqwest::StatusCode,
        body: impl Into<String>,
        headers: http::HeaderMap,
    ) -> Self {
        Self::Status {
            status: status.as_u16(),
            body: body.into(),
            headers,
        }
    }

    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Self::Unauthorized | Self::Status { status: 401, .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_preserves_long_unicode_json_and_classifies_unauthorized_without_logging_content() {
        let body = serde_json::json!({"error":{
            "code":"invalid_prompt", "message":"中文🦀".repeat(700)
        }})
        .to_string();
        for status in [
            reqwest::StatusCode::BAD_REQUEST,
            reqwest::StatusCode::UNAUTHORIZED,
        ] {
            let error = UpstreamError::status(status, body.clone());
            assert_eq!(
                error.is_unauthorized(),
                status == reqwest::StatusCode::UNAUTHORIZED
            );
            assert_eq!(
                error.to_string(),
                format!("upstream returned HTTP {}", status.as_u16())
            );
            let UpstreamError::Status {
                body: preserved, ..
            } = error
            else {
                panic!("missing HTTP error")
            };
            assert_eq!(preserved, body);
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&preserved).unwrap()["error"]["code"],
                "invalid_prompt"
            );
        }
    }
}
