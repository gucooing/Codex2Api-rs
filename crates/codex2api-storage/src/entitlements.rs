//! Resolve business policy once; protocol handlers must not interpret plan JSON.
use crate::{Result, Storage, StorageError};
use codex2api_common::ModelRule;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct EffectiveEntitlements {
    pub virtual_account_id: String,
    /// The client platform of the subscription, fixed by its OAuth protocol.
    pub provider_id: String,
    pub subscription_active: bool,
    pub execution_enabled: bool,
    /// The plan's manual model list while the subscription is active.
    pub models: Vec<ModelRule>,
    pub(crate) expires_at: Option<i64>,
    pub(crate) plan_type: String,
    pub(crate) spending_windows: Vec<crate::SpendingWindow>,
}

impl EffectiveEntitlements {
    pub fn model_rule(&self, model: &str) -> Option<&ModelRule> {
        self.models.iter().find(|rule| rule.model == model)
    }
    pub fn permits(&self, model: &str) -> bool {
        self.execution_enabled && self.model_rule(model).is_some()
    }
}

impl Storage {
    pub async fn effective_entitlements(
        &self,
        owner: &str,
    ) -> Result<std::sync::Arc<EffectiveEntitlements>> {
        let key = format!("entitlements:{owner}");
        let mut value = self
            .runtime
            .snapshots
            .read(key.clone(), async {
                let now = chrono::Utc::now().timestamp();
                let value = std::sync::Arc::new(self.effective_entitlements_at(owner, now).await?);
                let ttl = value.expires_at.map_or(30_000, |at| {
                    at.saturating_mul(1000)
                        .saturating_sub(chrono::Utc::now().timestamp_millis())
                        .clamp(0, 30_000)
                });
                Ok((value, std::time::Duration::from_millis(ttl as u64)))
            })
            .await?;
        if value
            .expires_at
            .is_some_and(|at| at <= chrono::Utc::now().timestamp())
        {
            let value = std::sync::Arc::make_mut(&mut value);
            value.subscription_active = false;
            value.execution_enabled = false;
            value.models.clear();
        }
        Ok(value)
    }

    pub async fn effective_entitlements_at(
        &self,
        owner: &str,
        now: i64,
    ) -> Result<EffectiveEntitlements> {
        let account = self
            .platform_account(owner)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(owner.into()))?;
        let plan = self
            .virtual_plan(&account.plan_id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(account.plan_id.clone()))?;
        if account.provider_id != plan.provider_id {
            return Err(StorageError::Constraint("账户与套餐的提供商不一致".into()));
        }
        let subscription_active = account.subscription_active_at(now);
        let models = if subscription_active {
            plan.models()?
        } else {
            Vec::new()
        };
        Ok(EffectiveEntitlements {
            plan_type: account.plan_type.as_str().to_owned(),
            spending_windows: crate::plan_spending_windows(&plan.config)?,
            expires_at: account
                .subscription_expires_at
                .as_deref()
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                .map(|at| at.timestamp()),
            virtual_account_id: account.id,
            provider_id: account.provider_id,
            subscription_active,
            execution_enabled: subscription_active,
            models,
        })
    }

    /// Enabled catalog models granted by the subscription's plan, on any platform.
    pub async fn available_virtual_models(&self, owner: &str) -> Result<Vec<crate::ModelConfig>> {
        let entitlements = self.effective_entitlements(owner).await?;
        Ok(self
            .model_catalog_snapshot()
            .await?
            .iter()
            .filter(|model| model.enabled && !model.deleted && entitlements.permits(&model.model))
            .cloned()
            .collect())
    }
}
