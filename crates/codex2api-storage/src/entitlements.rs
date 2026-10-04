//! Resolve business policy once; protocol handlers must not interpret plan JSON.
use crate::{Result, Storage, StorageError};
use codex2api_core::ModelAccess;

pub struct EffectiveEntitlements {
    pub virtual_account_id: String,
    pub provider_id: String,
    pub subscription_active: bool,
    pub execution_enabled: bool,
    pub models: ModelAccess,
}

impl Storage {
    pub async fn effective_entitlements(&self, owner: &str) -> Result<EffectiveEntitlements> {
        self.effective_entitlements_at(owner, chrono::Utc::now().timestamp())
            .await
    }

    pub async fn effective_entitlements_at(
        &self,
        owner: &str,
        now: i64,
    ) -> Result<EffectiveEntitlements> {
        let account = self
            .effective_virtual_account_at(owner, now)
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
            .model_configs(provider)
            .await?
            .into_iter()
            .filter(|model| {
                model.enabled
                    && !model.deleted
                    && entitlements.models.permits(provider, &model.model)
            })
            .collect())
    }
}
