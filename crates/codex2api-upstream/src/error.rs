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
    #[error("upstream returned HTTP {status}: {body}")]
    Status { status: u16, body: String },
    #[error("invalid header value: {0}")]
    InvalidHeader(#[from] http::header::InvalidHeaderValue),
    #[error("SSE stream error: {0}")]
    Stream(String),
    #[error("SSE stream idle timeout")]
    StreamIdleTimeout,
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Account(#[from] AccountError),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl UpstreamError {
    pub fn status(status: reqwest::StatusCode, body: impl Into<String>) -> Self {
        let body = truncate_body(body.into());
        if status == reqwest::StatusCode::UNAUTHORIZED {
            Self::Unauthorized
        } else {
            Self::Status {
                status: status.as_u16(),
                body,
            }
        }
    }

    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Self::Unauthorized)
    }
}

fn truncate_body(body: String) -> String {
    const MAX: usize = 4096;
    if body.len() <= MAX {
        body
    } else {
        let mut cut = body;
        cut.truncate(MAX);
        cut.push_str("…");
        cut
    }
}
