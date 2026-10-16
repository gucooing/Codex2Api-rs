#[derive(Debug, thiserror::Error)]
pub enum GrokAuthError {
    #[error("Grok authorization expired or unavailable")]
    PendingNotFound,
    #[error("Grok OAuth state mismatch")]
    StateMismatch,
    #[error("Missing Grok authorization code")]
    MissingCode,
    #[error("Grok ID token verification failed")]
    InvalidIdToken,
    #[error("Grok authorization identity does not match the supplier")]
    AccountMismatch,
    #[error("Grok refresh token is missing")]
    MissingRefreshToken,
    #[error("Grok supplier credentials are unavailable: {0}")]
    TokensNotFound(String),
    #[error("Grok authorization returned HTTP {status}: {message}")]
    TokenEndpoint { status: u16, message: String },
    #[error("Grok refresh returned HTTP {status}: {message}")]
    RefreshRejected {
        status: u16,
        code: Option<String>,
        message: String,
    },
    #[error(transparent)]
    Storage(#[from] codex2api_storage::StorageError),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
impl GrokAuthError {
    pub fn permanent_refresh_failure(&self) -> bool {
        matches!(self, Self::MissingRefreshToken)
            || matches!(self, Self::RefreshRejected {code:Some(code),..} if matches!(code.as_str(),"invalid_grant"|"invalid_client"))
    }
}
