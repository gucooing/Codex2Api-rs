use thiserror::Error;

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("invalid admin credentials")]
    InvalidCredentials,
    #[error("{0}")]
    InvalidAdminUpdate(&'static str),
    #[error("admin user not found")]
    AdminNotFound,
    #[error("admin session not found or expired")]
    SessionNotFound,
    #[error("account `{0}` not found")]
    AccountNotFound(String),
    #[error("chatgpt_account_id already bound")]
    DuplicateChatgptAccountId,
    #[error("oauth pending state not found or expired")]
    OAuthPendingNotFound,
    #[error("proxy api key not found")]
    ApiKeyNotFound,
    #[error("OAuth 凭据或绑定账户不可用，请选择已启用且完成授权的账户")]
    OAuthCredentialUnavailable,
    #[error("代理不存在")]
    ProxyNotFound,
    #[error("代理配置已更改，请重新检测")]
    ProxyChanged,
    #[error("该代理绑定了 {0} 个账户，删除前需要确认解除绑定")]
    ProxyHasBindings(i64),
    #[error("请填写有效的代理名称和 http、https、socks5 或 socks5h 代理地址")]
    InvalidProxy,
    #[error("invalid account status: {0}")]
    InvalidAccountStatus(String),
    #[error("unique constraint violated: {0}")]
    Constraint(String),
    #[error("password hashing failed: {0}")]
    Password(String),
    #[error("invalid password hash: {0}")]
    InvalidPasswordHash(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlx(sqlx::Error),
    #[error("migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl From<sqlx::Error> for StorageError {
    fn from(err: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &err {
            if db.is_unique_violation() {
                let msg = db.message().to_string();
                if msg.contains("chatgpt_account_id") {
                    return StorageError::DuplicateChatgptAccountId;
                }
                return StorageError::Constraint(msg);
            }
        }
        StorageError::Sqlx(err)
    }
}
