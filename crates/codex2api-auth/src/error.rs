use thiserror::Error;

use codex2api_accounts::AccountError;

pub type Result<T> = std::result::Result<T, AuthError>;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("oauth pending state not found or expired")]
    PendingNotFound,
    #[error("oauth state mismatch")]
    StateMismatch,
    #[error("missing authorization code")]
    MissingCode,
    #[error("oauth callback error: {code}")]
    Callback {
        code: String,
        description: Option<String>,
    },
    #[error("oauth login cancelled")]
    Cancelled,
    #[error("unable to bind OAuth callback on 127.0.0.1:{0} (or fallback)")]
    CallbackBind(u16),
    #[error("token endpoint returned status {status}: {message}")]
    TokenEndpoint { status: u16, message: String },
    #[error("refresh token is missing")]
    MissingRefreshToken,
    #[error("access token could not be refreshed: {0}")]
    Refresh(String),
    #[error("invalid ID token format")]
    InvalidIdToken,
    #[error("account `{0}` not found")]
    AccountNotFound(String),
    #[error("refreshed token belongs to a different ChatGPT account")]
    AccountMismatch,
    #[error("no tokens stored for account `{0}`")]
    TokensNotFound(String),
    #[error("chatgpt_account_id is required to bind an account")]
    MissingChatgptAccountId,
    #[error(transparent)]
    SupplierAccount(AccountError),
    #[error(transparent)]
    Storage(#[from] codex2api_storage::StorageError),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AuthError {
    pub fn token_endpoint(status: reqwest::StatusCode, message: impl Into<String>) -> Self {
        Self::TokenEndpoint {
            status: status.as_u16(),
            message: message.into(),
        }
    }
}

impl From<AccountError> for AuthError {
    fn from(err: AccountError) -> Self {
        match err {
            AccountError::NotFound(id) => Self::AccountNotFound(id),
            AccountError::MissingChatgptAccountId => Self::MissingChatgptAccountId,
            AccountError::Storage(storage) => Self::Storage(storage),
            AccountError::Json(json) => Self::Json(json),
            other => Self::SupplierAccount(other),
        }
    }
}
