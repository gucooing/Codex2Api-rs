#[derive(Debug, thiserror::Error)]
pub enum ApiAuthError {
    #[error("{0}")]
    Invalid(String),
    #[error("the supplier account is not an API-key account")]
    AccountMismatch,
    #[error("API-key supplier credentials are unavailable: {0}")]
    TokensNotFound(String),
    #[error(transparent)]
    Storage(#[from] codex2api_storage::StorageError),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
