use crate::{PlatformAccount, Result, Storage, StorageError, VirtualPlan};
impl Storage {
    pub async fn platform_free_plan(&self, provider: &str) -> Result<VirtualPlan> {
        sqlx::query_as("SELECT p.* FROM virtual_plans p JOIN platform_free_plans f ON f.plan_id=p.id WHERE f.provider_id=?")
            .bind(provider).fetch_optional(self.pool()).await?.ok_or(StorageError::InvalidAdminUpdate("该平台未配置 Free 套餐"))
    }
    pub async fn effective_platform_account(&self, id: &str) -> Result<Option<PlatformAccount>> {
        self.effective_platform_account_at(id, chrono::Utc::now().timestamp())
            .await
    }
    pub async fn effective_platform_account_at(
        &self,
        id: &str,
        now: i64,
    ) -> Result<Option<PlatformAccount>> {
        let Some(mut account) = self.platform_account(id).await? else {
            return Ok(None);
        };
        if account.plan_type != "free" && account.effective_plan_at(now) == "free" {
            let plan = self.platform_free_plan(&account.provider_id).await?;
            account.plan_id = plan.id;
            account.plan_type = "free".into();
            account.subscription_expires_at = None;
        }
        Ok(Some(account))
    }
    pub(crate) async fn subscription_anchor_at(&self, id: &str, now: i64) -> Result<i64> {
        Ok(sqlx::query_scalar("SELECT CASE WHEN plan_type!='free' AND subscription_expires_at IS NOT NULL AND unixepoch(subscription_expires_at)<=? THEN unixepoch(subscription_expires_at) ELSE COALESCE(unixepoch(subscription_started_at),unixepoch(created_at)) END FROM platform_accounts WHERE id=?")
            .bind(now).bind(id).fetch_one(self.pool()).await?)
    }
}
