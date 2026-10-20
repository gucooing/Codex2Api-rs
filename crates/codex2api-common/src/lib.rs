//! Shared contracts, provider constants and utilities. No business services or storage.
//! Optional frontend utilities build and serve each business's embedded website.

use serde::{Deserialize, Serialize};

mod model_presets;
pub mod providers;
pub use model_presets::{
    BUILTIN_MODELS, BuiltinModel, MODEL_PRESET_VERSION, SupportedModel, TokenPricePreset,
    builtin_models, model_preset_source, model_price_preset,
};

pub const CHATGPT: &str = "chatgpt";
pub const GROK: &str = "grok";
pub const ANTIGRAVITY: &str = "antigravity";
/// The API-key channel: any OpenAI Responses, Chat Completions or Anthropic Messages
/// endpoint reached with a key, configured per account.
pub const API: &str = "api";

pub const PROVIDERS: &[(&str, &str)] = &[
    (CHATGPT, "ChatGPT"),
    (GROK, "Grok"),
    (ANTIGRAVITY, "Antigravity"),
    (API, "API"),
];

pub const GEMINI: &str = "gemini";
pub const CLAUDE: &str = "claude";
/// Catalog models of vendors without their own label.
pub const OTHER: &str = "other";

/// Identification labels of catalog models: the model's vendor. Supplier channels
/// such as Antigravity are not vendors. Never a routing restriction.
pub const MODEL_PLATFORMS: &[(&str, &str)] = &[
    (CHATGPT, "ChatGPT"),
    (GROK, "Grok"),
    (GEMINI, "Gemini"),
    (CLAUDE, "Claude"),
    (OTHER, "Other"),
];

pub fn valid_model_platform(label: &str) -> bool {
    MODEL_PLATFORMS.iter().any(|(id, _)| *id == label)
}

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

/// One manually granted model with its optional billing multiplier and per-model
/// limits. Plans and API groups share this shape; presence in a list grants access.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRule {
    pub model: String,
    #[serde(default)]
    pub multiplier: Option<String>,
    #[serde(default)]
    pub concurrency_limit: u32,
    #[serde(default)]
    pub rpm_limit: u32,
}

impl ModelRule {
    pub fn granted(model: &str) -> Self {
        Self {
            model: model.into(),
            multiplier: None,
            concurrency_limit: 0,
            rpm_limit: 0,
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
    Operation(&'static str),
}

pub fn billing_operation(provider: &str, endpoint: &str) -> Option<&'static str> {
    match provider {
        CHATGPT => providers::chatgpt::billing_operation(endpoint),
        _ => None,
    }
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

pub fn subscription_tiers(provider: &str) -> &'static [&'static str] {
    match provider {
        CHATGPT => providers::chatgpt::SUBSCRIPTION_TIERS,
        GROK => providers::grok::SUBSCRIPTION_TIERS,
        _ => &[],
    }
}

pub fn valid_subscription_tier(provider: &str, tier: &str) -> bool {
    subscription_tiers(provider).contains(&tier)
}

#[cfg(feature = "frontend")]
pub mod frontend;
#[cfg(feature = "frontend-build")]
pub mod frontend_build;
pub mod oauth;
pub mod version;
