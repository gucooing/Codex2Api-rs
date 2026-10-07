// Shared fixtures build the current user + platform schema through its real writes.
// Included only by tests and disposable review fixtures, never by the service.
use codex2api_storage::{PlatformAccount, Result, Storage, User, UserKind, VirtualUserChange};

#[allow(dead_code, async_fn_in_trait)]
pub trait AccountFixture {
    async fn save_account_fixture(&self, account: &PlatformAccount) -> Result<()>;
}
impl AccountFixture for Storage {
    async fn save_account_fixture(&self, account: &PlatformAccount) -> Result<()> {
        let plan = self.virtual_plan(&account.plan_id).await?;
        if plan.as_ref().is_none_or(|p| {
            p.plan_type != account.plan_type || p.provider_id != account.provider_id
        }) {
            return Err(codex2api_storage::StorageError::InvalidAdminUpdate(
                "fixture plan does not match its platform",
            ));
        }
        let old = self.virtual_user(&account.id).await?;
        let user = User {
            kind: UserKind::Virtual,
            id: old
                .as_ref()
                .map(|u| u.id.clone())
                .unwrap_or_else(|| sqlx::types::Uuid::new_v4().to_string()),
            username: account.username.clone(),
            password_hash: account.password_hash.clone(),
            name: account.name.clone(),
            email: account.email.clone(),
            enabled: account.enabled,
            wallet_cents: 0,
            revision: old.as_ref().map_or(1, |u| u.revision),
            created_at: account.created_at.clone(),
        };
        let revision = if self.platform_account(&account.id).await?.is_some() {
            Some(self.platform_revision(&account.id).await?)
        } else {
            None
        };
        self.save_virtual_user(VirtualUserChange {
            user: &user,
            platform_id: &account.id,
            provider_id: &account.provider_id,
            plan_id: &account.plan_id,
            expires_at: account.subscription_expires_at.as_deref(),
            user_revision: old.as_ref().map(|u| u.revision),
            revision,
        })
        .await
    }
}
