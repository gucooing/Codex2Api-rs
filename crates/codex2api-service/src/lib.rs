//! Application services shared by HTTP, WebSocket and provider protocol adapters.
//! This crate makes business decisions; it does not know HTTP headers or frame formats.

use codex2api_core::{ExecutionKind, PolicyError};
use codex2api_storage::{PriceRequirement, Storage};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error(transparent)]
    Storage(#[from] codex2api_storage::StorageError),
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error("The account has reached a subscription spending limit")]
    BudgetExceeded,
    #[error("Reliable billing is unavailable for this model or operation")]
    PricingUnavailable,
    #[error("The virtual account has reached its requests-per-minute limit")]
    RpmExceeded { retry_after: u32 },
    #[error("The account has reached its concurrent generation limit")]
    ConcurrencyExceeded,
}

pub type Result<T> = std::result::Result<T, ServiceError>;

pub struct ExecutionRequest<'a> {
    pub kind: ExecutionKind,
    pub model: Option<&'a str>,
    pub service_tier: Option<&'a str>,
    pub image_size: Option<&'a str>,
}

#[derive(Clone)]
pub struct ExecutionService {
    storage: Storage,
}

impl ExecutionService {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    pub async fn admit_request(&self, owner: &str, consume: bool) -> Result<()> {
        if let Some(retry_after) = self
            .storage
            .admit_virtual_request(owner, consume, chrono::Utc::now().timestamp_millis())
            .await?
        {
            return Err(ServiceError::RpmExceeded { retry_after });
        }
        Ok(())
    }

    pub async fn check_budget(&self, owner: &str) -> Result<()> {
        let access = self.storage.effective_entitlements(owner).await?;
        if !access.execution_enabled {
            return Err(PolicyError::SubscriptionRequired.into());
        }
        if !self.storage.virtual_spending_status(owner).await?.0 {
            return Err(ServiceError::BudgetExceeded);
        }
        Ok(())
    }

    pub async fn authorize(
        &self,
        owner: &str,
        provider: &str,
        request: ExecutionRequest<'_>,
    ) -> Result<()> {
        if !codex2api_core::supported_provider(provider) {
            return Err(PolicyError::UnsupportedProvider.into());
        }
        let entitlement = self.storage.effective_entitlements(owner).await?;
        if entitlement.provider_id != provider {
            return Err(PolicyError::ProviderMismatch.into());
        }
        if !entitlement.execution_enabled {
            return Err(PolicyError::SubscriptionRequired.into());
        }
        let model = request
            .model
            .filter(|model| codex2api_core::valid_model(model))
            .ok_or(PolicyError::InvalidModel)?;
        if !entitlement.models.permits(provider, model) {
            return Err(PolicyError::ModelNotEntitled.into());
        }
        let configured = self.storage.model_config(provider, model).await?;
        if !configured.is_some_and(|model| model.enabled && !model.deleted) {
            return Err(PolicyError::ModelUnavailable.into());
        }
        if request.kind == ExecutionKind::InspectModel {
            return Ok(());
        }
        let (allowed, limited) = self.storage.virtual_spending_status(owner).await?;
        if !allowed {
            return Err(ServiceError::BudgetExceeded);
        }
        if !limited {
            return Ok(());
        }
        let requirement = match request.kind {
            ExecutionKind::Responses | ExecutionKind::Compact => {
                PriceRequirement::Tokens(codex2api_storage::price_tier(request.service_tier))
            }
            ExecutionKind::Image => PriceRequirement::Image(request.image_size),
            ExecutionKind::Operation(operation) => PriceRequirement::Operation(operation),
            ExecutionKind::InspectModel => return Ok(()),
            ExecutionKind::Unpriced => return Err(ServiceError::PricingUnavailable),
        };
        if !self
            .storage
            .price_available(provider, model, requirement)
            .await?
        {
            return Err(ServiceError::PricingUnavailable);
        }
        Ok(())
    }

    pub async fn check_unpriced(&self, owner: &str) -> Result<()> {
        self.check_budget(owner).await?;
        if self.storage.virtual_spending_limited(owner).await? {
            return Err(ServiceError::PricingUnavailable);
        }
        Ok(())
    }
}
