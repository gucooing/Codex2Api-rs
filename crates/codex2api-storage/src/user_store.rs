//! Data capabilities exposed to the user HTTP module. No administrator, supplier,
//! generic SQL, user enumeration or administrative subscription writes are exposed.
use crate::{
    BrowserAuthorization, CheckoutInput, DeviceAuthorizationApproval, OrderFilter, OrderRequest,
    PlatformAccount, Result, Storage, SubscriptionOrder, User, UserSession,
};
use serde_json::{Value, json};

#[derive(Clone)]
pub struct UserStore {
    storage: Storage,
}
impl Storage {
    pub fn user_store(&self) -> UserStore {
        UserStore {
            storage: self.clone(),
        }
    }
}
impl UserStore {
    pub async fn public_user_url(&self, fallback: &str) -> Result<String> {
        Ok(self
            .storage
            .public_url_settings()
            .await?
            .map(|settings| settings.user_url)
            .unwrap_or_else(|| fallback.to_owned()))
    }

    pub async fn checkout_preview(&self, owner: &str, input: CheckoutInput) -> Result<Value> {
        self.storage.checkout_preview(owner, input).await
    }
    pub async fn usage(&self, owner: &str, filter: &crate::UserUsageFilter) -> Result<Value> {
        self.storage.user_usage(owner, filter).await
    }
    pub async fn user(&self, id: &str) -> Result<Option<User>> {
        self.storage.user(id).await
    }
    pub async fn user_by_username(&self, name: &str) -> Result<Option<User>> {
        self.storage.user_by_username(name).await
    }
    pub async fn allow_user_login_attempt(&self, name: &str) -> Result<bool> {
        self.storage.allow_user_login_attempt(name).await
    }
    pub async fn create_user_session(&self, user: &User, csrf: &str) -> Result<Option<String>> {
        self.storage.create_user_session(user, csrf).await
    }
    pub async fn user_session(&self, token: &str) -> Result<Option<UserSession>> {
        self.storage.user_session(token).await
    }
    pub async fn revoke_user_session(&self, hash: &str) -> Result<()> {
        self.storage.revoke_user_session(hash).await
    }
    pub async fn change_password(&self, user: &User, password_hash: String) -> Result<bool> {
        let mut changed = user.clone();
        changed.password_hash = password_hash;
        self.storage.save_user(&changed, Some(user.revision)).await
    }
    pub async fn create_order(&self, input: OrderRequest<'_>) -> Result<SubscriptionOrder> {
        self.storage.create_subscription_order(input).await
    }
    pub async fn order(&self, owner: &str, id: &str) -> Result<Option<SubscriptionOrder>> {
        self.storage.subscription_order(Some(owner), id).await
    }
    pub async fn orders(&self, owner: &str, filter: &OrderFilter) -> Result<Value> {
        self.storage.user_subscription_orders(owner, filter).await
    }
    pub async fn order_plans(&self, owner: &str) -> Result<Vec<Value>> {
        self.storage.order_plan_options(Some(owner)).await
    }
    pub async fn pay_order(&self, owner: &str, id: &str) -> Result<SubscriptionOrder> {
        self.storage.pay_subscription_order(owner, id).await
    }
    pub async fn cancel_order(&self, owner: &str, id: &str) -> Result<SubscriptionOrder> {
        self.storage
            .cancel_subscription_order(Some(owner), id)
            .await
    }
    pub async fn wallet_entries(
        &self,
        owner: &str,
        query: &crate::ListQuery,
    ) -> Result<crate::ListPage<Value>> {
        Ok(self
            .storage
            .wallet_entry_page(owner, query)
            .await?
            .map(|entry| entry.user_view()))
    }
    pub async fn plans(&self, query: &crate::ListQuery) -> Result<crate::ListPage<Value>> {
        let mut items = vec![];
        let page = self
            .storage
            .read_list::<crate::VirtualPlan>(query, "p.*", "virtual_plans p", "p.rowid", |q| {
                q.push(" AND p.allow_purchase=1 AND p.plan_type!='free'");
                crate::list_query::exact(q, "p.provider_id", &query.provider_id);
                crate::list_query::search(
                    q,
                    &["p.name", "json_extract(p.config,'$.description')"],
                    &query.search,
                );
            })
            .await?;
        for plan in page.items {
            let price = plan
                .sale_price_cents()?
                .map(|value| crate::format_units(value, 2));
            let models: Vec<(String,String,String)> = sqlx::query_as("SELECT m.provider_id,m.model,m.kind FROM model_catalog m JOIN virtual_plans p ON p.provider_id=m.provider_id WHERE p.id=? AND m.enabled=1 AND m.deleted=0 AND (json_extract(p.config,'$.model_access')='all' OR (json_extract(p.config,'$.model_access')='selected' AND EXISTS(SELECT 1 FROM json_each(p.config,'$.models') selected WHERE json_extract(selected.value,'$.provider_id')=m.provider_id AND json_extract(selected.value,'$.model')=m.model))) ORDER BY m.provider_id,m.model")
                .bind(&plan.id).fetch_all(self.storage.pool()).await?;
            let models: Vec<Value> = models.into_iter().map(|(provider,model,kind)|json!({"provider_id":provider,"model":model,"kind":kind})).collect();
            items.push(json!({"id":plan.id,"name":plan.name,"provider_id":plan.provider_id,"plan_type":plan.plan_type,
                "description":plan.description(),"sale_price_usd":price,"duration_days":plan.duration_days()?,"revision":plan.revision,
                "model_access":plan.config["model_access"],"models":models,"spending_windows":crate::plan_spending_windows(&plan.config)?}));
        }
        Ok(crate::ListPage {
            items,
            total: page.total,
            page: page.page,
            page_size: page.page_size,
        })
    }
    pub async fn subscriptions(
        &self,
        owner: &str,
        query: &crate::ListQuery,
    ) -> Result<crate::ListPage<Value>> {
        let mut items = vec![];
        let page = self
            .storage
            .subscription_page(&crate::ListQuery {
                user_id: owner.to_owned(),
                include_expired: true,
                ..query.clone()
            })
            .await?;
        for subscription in page.items {
            let account = self
                .storage
                .effective_platform_account(&subscription.virtual_account_id)
                .await?
                .ok_or_else(|| {
                    crate::StorageError::AccountNotFound(subscription.virtual_account_id.clone())
                })?;
            let plan = self
                .storage
                .virtual_plan(&account.plan_id)
                .await?
                .ok_or_else(|| crate::StorageError::AccountNotFound(account.plan_id.clone()))?;
            let pricing:Option<(Option<i64>,i64)>=sqlx::query_as("SELECT unit_price_cents,duration_days FROM subscription_pricing_periods WHERE subscription_id=? AND superseded_at_ms IS NULL AND starts_at_ms<=? AND ends_at_ms>? ORDER BY starts_at_ms DESC LIMIT 1")
                .bind(&account.id).bind(chrono::Utc::now().timestamp_millis()).bind(chrono::Utc::now().timestamp_millis()).fetch_optional(self.storage.pool()).await?;
            let now = chrono::Utc::now().timestamp();
            let anchor = self
                .storage
                .subscription_anchor_at(&account.id, now)
                .await?;
            let raw_windows = self
                .storage
                .nested_spending_windows(
                    &account.id,
                    &crate::plan_spending_windows(&plan.config)?,
                    if anchor > now { 0 } else { anchor },
                    now,
                )
                .await?;
            let mut windows = Vec::new();
            for window in raw_windows {
                let unpriced: i64 = if let Some(start) = window["started_at"].as_i64() {
                    sqlx::query_scalar("SELECT COUNT(*) FROM usage_records WHERE subject_kind='virtual_account' AND subject_id=? AND requested_at_ms>=? AND requested_at_ms<? AND cost_nano_usd IS NULL AND ((SELECT quota_reset_credit_id FROM platform_accounts WHERE id=?) IS NULL OR quota_reset_credit_id=(SELECT quota_reset_credit_id FROM platform_accounts WHERE id=?))")
                        .bind(&account.id).bind(start*1000).bind((now+1)*1000).bind(&account.id).bind(&account.id).fetch_one(self.storage.pool()).await?
                } else {
                    0
                };
                windows.push(json!({"duration_seconds":window["limit_window_seconds"],"reset_at":window["reset_at"],"used_percent":window["used_percent"],
                    "used_usd":window["used_usd"],"limit_usd":window["limit_usd"],"remaining_usd":window["effective_remaining_usd"],"unpriced_requests":unpriced}));
            }
            items.push(json!({"spending_windows":windows,"current_price_cents":pricing.as_ref().and_then(|p|p.0),"current_duration_days":pricing.map(|p|p.1),"id":account.id,"provider_id":account.provider_id,"plan_id":plan.id,"plan_name":plan.name,
                "plan_type":account.plan_type,"expires_at":account.subscription_expires_at,"enabled":account.enabled,"expired":false,"revision":subscription.revision}));
        }
        Ok(crate::ListPage {
            items,
            total: page.total,
            page: page.page,
            page_size: page.page_size,
        })
    }
    pub async fn devices(
        &self,
        owner: &str,
        query: &crate::ListQuery,
    ) -> Result<crate::ListPage<Value>> {
        let page = self.storage.read_list::<crate::VirtualDevice>(query,"d.*",
            "virtual_devices d JOIN platform_accounts p ON p.id=d.virtual_account_id JOIN regular_users u ON u.id=p.user_id",
            "d.created_at DESC,d.id", |q| { q.push(" AND u.id=").push_bind(owner.to_owned()); }).await?;
        Ok(page.map(|device|json!({"id":device.id,"provider_id":device.provider_id,"user_agent":device.user_agent,"created_at":device.created_at,"last_used_at":device.last_used_at})))
    }
    pub async fn revoke_device(&self, owner: &str, id: &str) -> Result<bool> {
        Ok(sqlx::query("DELETE FROM virtual_devices WHERE id=? AND virtual_account_id IN(SELECT s.id FROM platform_accounts s JOIN user_identities u ON u.id=s.user_id WHERE u.id=? AND u.enabled=1 AND u.kind='regular')")
            .bind(id).bind(owner).execute(self.storage.pool()).await?.rows_affected()==1)
    }
    pub async fn user_platform_account(
        &self,
        user: &str,
        provider: &str,
    ) -> Result<Option<PlatformAccount>> {
        self.storage.user_platform_account(user, provider).await
    }
    pub async fn create_oauth_browser_flow(
        &self,
        id: &str,
        cookie: &str,
        csrf: &str,
        request: &str,
    ) -> Result<()> {
        self.storage
            .create_oauth_browser_flow(id, cookie, csrf, request)
            .await
    }
    pub async fn oauth_browser_flow(
        &self,
        id: &str,
        cookie: &str,
        csrf: &str,
    ) -> Result<Option<String>> {
        self.storage.oauth_browser_flow(id, cookie, csrf).await
    }
    pub async fn cancel_oauth_browser_flow(
        &self,
        id: &str,
        cookie: &str,
        csrf: &str,
    ) -> Result<bool> {
        self.storage
            .cancel_oauth_browser_flow(id, cookie, csrf)
            .await
    }
    pub async fn clear_browser_identity(&self, id: &str) -> Result<()> {
        self.storage.clear_browser_identity(id).await
    }
    pub async fn bind_browser_identity(
        &self,
        id: &str,
        account: &PlatformAccount,
        session: Option<&str>,
    ) -> Result<bool> {
        self.storage
            .bind_browser_identity(id, account, session)
            .await
    }
    pub async fn browser_identity(&self, id: &str) -> Result<Option<PlatformAccount>> {
        self.storage.browser_identity(id).await
    }
    pub async fn authorize_oauth_browser_flow(
        &self,
        input: BrowserAuthorization<'_>,
    ) -> Result<bool> {
        self.storage.authorize_oauth_browser_flow(input).await
    }
    pub async fn approve_device_authorization(
        &self,
        code: &str,
        input: DeviceAuthorizationApproval<'_>,
    ) -> Result<bool> {
        self.storage.approve_device_authorization(code, input).await
    }
}
