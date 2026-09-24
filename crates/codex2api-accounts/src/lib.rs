//! Per-account isolated Codex environments.
//!
//! Isolation is a SQLite account row plus `supplier_tokens`. Request headers
//! (`originator`, `User-Agent`, `x-codex-installation-id`) are built from that
//! row so upstream behavior matches a logged-in Codex CLI. Official `$CODEX_HOME`
//! files are not used.

mod auth_json;
mod error;
mod identity;
mod store;

pub use auth_json::{AuthDotJson, TokenData};
pub use error::{AccountError, Result};
pub use identity::{
    AccountIdentity, HostRuntime, HttpFingerprint, canonicalize_installation_id,
    new_installation_id,
};
pub use store::{
    BoundAccount, OauthIdentity, PendingAccount, SupplierAccountStore, SupplierContext,
};

#[cfg(test)]
mod tests;
