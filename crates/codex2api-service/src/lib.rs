//! Application services shared by HTTP, WebSocket and provider protocol adapters.
//! This crate makes business decisions; it does not know HTTP headers or frame formats.

use codex2api_core::{ExecutionKind, PolicyError};
use codex2api_storage::Storage;

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

    pub async fn check_budget(&self, owner: &str) -> Result<()> {
        let access = self.storage.effective_entitlements(owner).await?;
        if !access.execution_enabled {
            return Err(PolicyError::SubscriptionRequired.into());
        }
        if self.storage.virtual_quota(owner).await?["rate_limit"]["allowed"] == false {
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
        self.check_budget(owner).await?;
        if !self.storage.virtual_spending_limited(owner).await? {
            return Ok(());
        }
        let priced = match request.kind {
            ExecutionKind::Responses | ExecutionKind::Compact => {
                let tier = codex2api_storage::price_tier(request.service_tier);
                self.storage
                    .model_prices(provider)
                    .await?
                    .iter()
                    .any(|price| {
                        price.model == model
                            && Some(price.tier.as_str()) == tier
                            && price.min_input_tokens == 0
                    })
            }
            ExecutionKind::Image => {
                self.storage
                    .image_prices(provider)
                    .await?
                    .iter()
                    .any(|price| {
                        price.model == model
                            && request
                                .image_size
                                .filter(|size| *size != "auto")
                                .is_none_or(|size| {
                                    size == price.resolution
                                        || codex2api_storage::image_resolution_tier(size).as_deref()
                                            == Some(&price.resolution)
                                })
                    })
            }
            ExecutionKind::InspectModel => true,
            ExecutionKind::Unpriced => false,
        };
        if !priced {
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
