#[derive(Debug, thiserror::Error)]
pub enum AntigravityAuthError {
    #[error("Antigravity authorization expired or unavailable")]
    PendingNotFound,
    #[error("Antigravity OAuth state mismatch")]
    StateMismatch,
    #[error("Missing Antigravity authorization code")]
    MissingCode,
    #[error("Antigravity authorization identity does not match the supplier")]
    AccountMismatch,
    #[error("Antigravity refresh token is missing")]
    MissingRefreshToken,
    #[error("Antigravity supplier credentials are unavailable: {0}")]
    TokensNotFound(String),
    #[error("Antigravity account has no Cloud Code Assist project")]
    MissingProject,
    #[error("Google authorization returned HTTP {status}: {message}")]
    TokenEndpoint { status: u16, message: String },
    #[error("Google refresh returned HTTP {status}: {message}")]
    RefreshRejected {
        status: u16,
        code: Option<String>,
        message: String,
    },
    #[error("Cloud Code Assist returned HTTP {status}: {message}")]
    Service { status: u16, message: String },
    #[error(transparent)]
    Storage(#[from] codex2api_storage::StorageError),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
impl AntigravityAuthError {
    /// Google reports a revoked or expired refresh token as `invalid_grant`.
    pub fn permanent_refresh_failure(&self) -> bool {
        matches!(self, Self::MissingRefreshToken)
            || matches!(self, Self::RefreshRejected { code: Some(code), .. }
                if matches!(code.as_str(), "invalid_grant" | "invalid_client" | "unauthorized_client"))
    }
}
