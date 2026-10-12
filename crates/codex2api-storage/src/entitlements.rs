//! Resolve business policy once; protocol handlers must not interpret plan JSON.
use crate::{Result, Storage, StorageError};
use codex2api_core::ModelAccess;

#[derive(Clone)]
pub struct EffectiveEntitlements {
    pub virtual_account_id: String,
    pub provider_id: String,
    pub subscription_active: bool,
    pub execution_enabled: bool,
    pub models: ModelAccess,
    pub(crate) expires_at: Option<i64>,
    pub(crate) plan_type: String,
    pub(crate) spending_windows: Vec<crate::SpendingWindow>,
}

impl Storage {
    pub async fn effective_entitlements(
        &self,
        owner: &str,
    ) -> Result<std::sync::Arc<EffectiveEntitlements>> {
        let key = format!("entitlements:{owner}");
        if let Some(value) = self.runtime.cached(&key) {
            return Ok(value);
        }
        let _fill = self.runtime.cache_fill(&key).await;
        if let Some(value) = self.runtime.cached(&key) {
            return Ok(value);
        }
        let epoch = self.runtime.epoch();
        let now = chrono::Utc::now().timestamp();
        let value = std::sync::Arc::new(self.effective_entitlements_at(owner, now).await?);
        let ttl = value.expires_at.map_or(30_000, |at| {
            at.saturating_mul(1000)
                .saturating_sub(chrono::Utc::now().timestamp_millis())
                .clamp(0, 30_000)
        });
        self.runtime.cache(
            key,
            &value,
            epoch,
            std::time::Duration::from_millis(ttl as u64),
        );
        Ok(value)
    }

    pub async fn effective_entitlements_at(
        &self,
        owner: &str,
        now: i64,
    ) -> Result<EffectiveEntitlements> {
        let account = self
            .effective_platform_account_at(owner, now)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(owner.into()))?;
        let plan = self
            .virtual_plan(&account.plan_id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(account.plan_id.clone()))?;
        if account.provider_id != plan.provider_id {
            return Err(StorageError::Constraint("账户与套餐的提供商不一致".into()));
        }
        let subscription_active = account.effective_plan_at(now) != "free";
        let enabled = account.enabled && plan.config["model_access"] != "none";
        let models = if enabled {
            plan.model_access()?
        } else {
            ModelAccess::None
        };
        Ok(EffectiveEntitlements {
            plan_type: account.effective_plan_at(now).to_owned(),
            spending_windows: crate::plan_spending_windows(&plan.config)?,
            expires_at: account
                .subscription_expires_at
                .as_deref()
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                .map(|at| at.timestamp()),
            virtual_account_id: account.id,
            provider_id: account.provider_id,
            subscription_active,
            execution_enabled: enabled,
            models,
        })
    }

    pub async fn available_virtual_models(
        &self,
        owner: &str,
        provider: &str,
    ) -> Result<Vec<crate::ModelConfig>> {
        let entitlements = self.effective_entitlements(owner).await?;
        if provider != entitlements.provider_id {
            return Ok(Vec::new());
        }
        Ok(self
            .model_catalog_snapshot(provider)
            .await?
            .iter()
            .filter(|model| {
                model.enabled
                    && !model.deleted
                    && entitlements.models.permits(provider, &model.model)
            })
            .cloned()
            .collect())
    }
}
