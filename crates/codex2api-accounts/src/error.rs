use thiserror::Error;

use codex2api_storage::StorageError;

pub type Result<T> = std::result::Result<T, AccountError>;

#[derive(Debug, Error)]
pub enum AccountError {
    #[error("account `{0}` not found")]
    NotFound(String),
    #[error("invalid account id `{0}`")]
    InvalidAccountId(String),
    #[error("invalid installation_id `{0}`")]
    InvalidInstallationId(String),
    #[error("invalid or empty HTTP fingerprint for account")]
    InvalidHttpFingerprint,
    #[error(
        "account `{0}` has an invalid or concurrently changed stored fingerprint; identity was not regenerated"
    )]
    InvalidStoredFingerprint(String),
    #[error("chatgpt_account_id is required to bind an account")]
    MissingChatgptAccountId,
    #[error("account `{0}` is not in pending status")]
    NotPending(String),
    #[error("account store has no storage backend")]
    StorageRequired,
    #[error(
        "refusing to rotate installation_id for chatgpt_account_id `{chatgpt_account_id}` \
         (account `{account_id}` keeps `{installation_id}`)"
    )]
    InstallationIdRotationRefused {
        chatgpt_account_id: String,
        account_id: String,
        installation_id: String,
    },
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Storage(#[from] StorageError),
}
