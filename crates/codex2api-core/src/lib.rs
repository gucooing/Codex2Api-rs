//! Provider-independent identities and access rules. No HTTP, SQL or supplier SDKs.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const CHATGPT: &str = "chatgpt";

pub const PROVIDERS: &[(&str, &str)] = &[(CHATGPT, "ChatGPT")];

pub fn default_provider() -> String {
    CHATGPT.into()
}

/// A provider must explicitly opt in through a protocol adapter before serving traffic.
pub fn supported_provider(provider: &str) -> bool {
    PROVIDERS.iter().any(|(id, _)| *id == provider)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountScope {
    pub provider_id: String,
    pub virtual_account_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ModelRef {
    pub provider_id: String,
    pub model: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelAccess {
    None,
    AllEnabled,
    Selected(BTreeSet<ModelRef>),
}

impl ModelAccess {
    pub fn permits(&self, provider: &str, model: &str) -> bool {
        match self {
            Self::None => false,
            Self::AllEnabled => true,
            Self::Selected(models) => models.contains(&ModelRef {
                provider_id: provider.into(),
                model: model.into(),
            }),
        }
    }

    pub fn from_config(
        mode: &str,
        models: impl IntoIterator<Item = ModelRef>,
    ) -> Result<Self, PolicyError> {
        let models: BTreeSet<_> = models.into_iter().collect();
        match mode {
            "none" if models.is_empty() => Ok(Self::None),
            "all" if models.is_empty() => Ok(Self::AllEnabled),
            "selected"
                if !models.is_empty()
                    && models
                        .iter()
                        .all(|s| valid_model(&s.model) && !s.provider_id.is_empty()) =>
            {
                Ok(Self::Selected(models))
            }
            _ => Err(PolicyError::InvalidModelScope),
        }
    }
}

pub fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 256
        && model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._:/".contains(&b))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionKind {
    Responses,
    Compact,
    Image,
    Unpriced,
    InspectModel,
}

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("Invalid model access policy")]
    InvalidModelScope,
    #[error("The provider does not match this account")]
    ProviderMismatch,
    #[error("This provider has no installed protocol adapter")]
    UnsupportedProvider,
    #[error("An active subscription is required")]
    SubscriptionRequired,
    #[error("The model is not included in this account's effective entitlements")]
    ModelNotEntitled,
    #[error("The model is not configured, enabled and available")]
    ModelUnavailable,
    #[error("A valid model is required")]
    InvalidModel,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_selection_never_becomes_all_models() {
        assert!(ModelAccess::from_config("selected", []).is_err());
        assert!(!ModelAccess::None.permits(CHATGPT, "gpt-test"));
        assert!(ModelAccess::AllEnabled.permits(CHATGPT, "gpt-test"));
    }
}
