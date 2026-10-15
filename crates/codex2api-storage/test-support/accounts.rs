// Shared fixtures build the current user + platform schema through its real writes.
// Included only by tests and disposable review fixtures, never by the service.
use codex2api_storage::{PlatformAccount, Result, Storage, User, UserKind};

#[allow(dead_code, async_fn_in_trait)]
pub trait AccountFixture {
    async fn plan_fixture(&self, provider: &str) -> Result<codex2api_storage::VirtualPlan>;
    async fn save_account_fixture(&self, account: &PlatformAccount) -> Result<()>;
    async fn grant_subscription_fixture(
        &self,
        user: &str,
        plan_id: &str,
    ) -> Result<PlatformAccount>;
    async fn bind_supplier_fixture(
        &self,
        owner: &str,
        provider: &str,
        supplier: Option<&str>,
        expected: Option<i64>,
    ) -> Result<bool>;
}
impl AccountFixture for Storage {
    async fn plan_fixture(&self, provider: &str) -> Result<codex2api_storage::VirtualPlan> {
        if let Some(plan) = self
            .virtual_plans()
            .await?
            .into_iter()
            .find(|p| p.provider_id == provider)
        {
            return Ok(plan);
        }
        let mut plan = self.virtual_plan("plus").await?.expect("fixture template");
        plan.id = format!("fixture-plan-{provider}");
        plan.provider_id = provider.into();
        plan.plan_type = "supergrok".into();
        plan.name = "Fixture Grok".into();
        plan.allow_purchase = false;
        plan.config["model_access"] = serde_json::json!("all");
        plan.config["models"] = serde_json::json!([]);
        self.save_virtual_plan(&plan, None).await?;
        Ok(self.virtual_plan(&plan.id).await?.unwrap())
    }

    async fn bind_supplier_fixture(
        &self,
        owner: &str,
        provider: &str,
        supplier: Option<&str>,
        expected: Option<i64>,
    ) -> Result<bool> {
        let current = self
            .execution_route(owner, provider)
            .await?
            .map(|route| route.revision);
        if expected.is_some() && current != expected {
            return Ok(false);
        }
        let tag = supplier.map(|id| format!("fixture-{id}"));
        if let (Some(supplier), Some(tag)) = (supplier, &tag) {
            self.save_supplier_tag(tag, provider, tag).await?;
            self.edit_supplier_tags(&[supplier.into()], std::slice::from_ref(tag), false)
                .await?;
            for model in self.model_configs(provider).await? {
                self.add_supplier_model(supplier, &model.model).await?;
            }
        }
        self.save_pool_route(owner, provider, tag.as_deref(), current)
            .await
    }

    async fn save_account_fixture(&self, account: &PlatformAccount) -> Result<()> {
        let plan = self.virtual_plan(&account.plan_id).await?;
        if plan.as_ref().is_none_or(|p| {
            p.plan_type != account.plan_type || p.provider_id != account.provider_id
        }) {
            return Err(codex2api_storage::StorageError::InvalidAdminUpdate(
                "fixture plan does not match its platform",
            ));
        }
        let old = self.platform_account_user(&account.id).await?;
        let existing = match old {
            Some(user) => Some(user),
            None => self.user_by_username(&account.username).await?,
        };
        let user = User {
            kind: UserKind::Regular,
            id: existing
                .as_ref()
                .map(|u| u.id.clone())
                .unwrap_or_else(|| sqlx::types::Uuid::new_v4().to_string()),
            username: account.username.clone(),
            password_hash: account.password_hash.clone(),
            name: account.name.clone(),
            email: account.email.clone(),
            enabled: true,
            wallet_cents: 0,
            revision: existing.as_ref().map_or(1, |u| u.revision),
            created_at: account.created_at.clone(),
        };
        self.save_user(&user, existing.as_ref().map(|u| u.revision))
            .await?;
        if let Some(previous) = self.platform_account(&account.id).await? {
            if previous.provider_id != account.provider_id {
                return Err(codex2api_storage::StorageError::InvalidAdminUpdate(
                    "fixture provider mismatch",
                ));
            }
            if previous.plan_id != account.plan_id
                || previous.subscription_expires_at != account.subscription_expires_at
                || previous.enabled != account.enabled
            {
                self.save_user_subscription(codex2api_storage::SubscriptionChange {
                    subscription_id: Some(&account.id),
                    reissue: false,
                    user_id: &user.id,
                    plan_id: &account.plan_id,
                    expires_at: account.subscription_expires_at.as_deref(),
                    enabled: account.enabled,
                    revision: Some(self.platform_revision(&account.id).await?),
                })
                .await?;
            }
        } else {
            sqlx::query("INSERT INTO platform_accounts(id,user_id,provider_id,plan_id,plan_type,subscription_expires_at,subscription_started_at,enabled,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(&account.id).bind(&user.id).bind(&account.provider_id).bind(&account.plan_id).bind(&account.plan_type).bind(&account.subscription_expires_at).bind(&account.created_at).bind(account.enabled).bind(&account.created_at).execute(self.pool()).await?;
        }
        sqlx::query("INSERT INTO execution_routes(virtual_account_id,provider_id) VALUES($1,$2) ON CONFLICT DO NOTHING").bind(&account.id).bind(&account.provider_id).execute(self.pool()).await?;
        self.invalidate_cached_snapshots();
        Ok(())
    }
    async fn grant_subscription_fixture(
        &self,
        user: &str,
        plan_id: &str,
    ) -> Result<PlatformAccount> {
        let id = self
            .save_user_subscription(codex2api_storage::SubscriptionChange {
                subscription_id: None,
                reissue: false,
                user_id: user,
                plan_id,
                expires_at: None,
                enabled: true,
                revision: None,
            })
            .await?;
        self.platform_account(&id)
            .await?
            .ok_or_else(|| codex2api_storage::StorageError::AccountNotFound(id))
    }
}
