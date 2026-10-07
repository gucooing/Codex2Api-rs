//! Automatic route guards and transaction guards share these fixed business scopes.
//! Credential verification reads the combined identity view; business reads use scopes.
use crate::{Result, Storage, StorageError};

#[derive(Clone, Copy)]
pub enum AccountScope {
    User,
    VirtualUser,
    UserPlatform,
    VirtualAccount,
}

impl AccountScope {
    pub(crate) fn view(self) -> &'static str {
        match self {
            Self::User => "regular_users",
            Self::VirtualUser => "virtual_users",
            Self::UserPlatform => "regular_platforms",
            Self::VirtualAccount => "virtual_platforms",
        }
    }
}

impl Storage {
    /// Used by administrator route middleware before any scoped handler executes.
    pub async fn require_account_scope(&self, id: &str, scope: AccountScope) -> Result<()> {
        require_on(&mut *self.pool().acquire().await?, id, scope).await
    }
}

/// Transactional writes use the exact same scope rule, including non-HTTP callers.
pub(crate) async fn require_on(
    connection: &mut sqlx::SqliteConnection,
    id: &str,
    scope: AccountScope,
) -> Result<()> {
    let exists: bool = sqlx::query_scalar(&format!(
        "SELECT EXISTS(SELECT 1 FROM {} WHERE id=?)",
        scope.view()
    ))
    .bind(id)
    .fetch_one(connection)
    .await?;
    if !exists {
        return Err(StorageError::AccountNotFound(id.into()));
    }
    Ok(())
}
