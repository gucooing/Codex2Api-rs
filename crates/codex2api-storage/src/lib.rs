//! SQLite persistence for admin user, accounts, tokens, and proxy API keys.
//!
//! `Storage` is the repository entry point; `types` contains persisted records;
//! `password` and `api_key` implement credential primitives. Migrations stay in
//! migrations/. This crate does not register HTTP routes or call official services.

mod api_key;
mod error;
mod oauth;
mod password;
mod proxy;
mod quota;
mod settings;
mod storage;
mod types;
mod usage;

pub use api_key::{
    PROXY_API_KEY_PREFIX_LEN, PROXY_API_KEY_TOKEN_PREFIX, generate_proxy_api_key, hash_api_key,
    proxy_api_key_prefix,
};
pub use error::{Result, StorageError};
pub use oauth::{
    OAuthAccess, OAuthAccountSummary, OAuthCredential, OAuthDevice, OAuthDeviceIdentity,
};
pub use password::{hash_password, verify_password};
pub use proxy::{OutboundProxy, ProxyConnectionCheck, ProxyQualityCheck, parse_proxy_url};
pub use quota::QuotaSnapshot;
pub use settings::{GatewaySettings, UaMode};
pub use storage::{DEFAULT_ADMIN_SESSION_TTL, DEFAULT_OAUTH_PENDING_TTL, Storage};
pub use types::{
    Account, AccountRuntime, AccountStatus, AccountTokens, AccountUpdate, AdminSession, AdminUser,
    IssuedProxyApiKey, NewAccount, OAuthPending, ProxyApiKey,
};
pub use usage::{USAGE_PAGE_SIZE, UsageFilter, UsageKeyOption, UsagePage, UsageRecord};

pub const DEFAULT_ADMIN_USERNAME: &str = "admin";
pub const DEFAULT_ADMIN_PASSWORD: &str = "admin";
pub const DEFAULT_DB_PATH: &str = "data/codex2api.sqlite";
