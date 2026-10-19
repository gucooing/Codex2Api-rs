//! Explicit administrator response contracts. These types never contain credentials.
use serde::{Deserialize, Serialize};
#[derive(Deserialize)]
pub struct AccountListQuery {
    pub search: Option<String>,
    pub limit: Option<u32>,
    pub provider_id: Option<String>,
    pub tag: Option<String>,
    #[serde(default)]
    pub for_routing: bool,
}
impl AccountListQuery {
    pub fn search_params(&self) -> Result<(&str, u32), super::error::ApiError> {
        let search = self.search.as_deref().unwrap_or("").trim();
        let limit = self.limit.unwrap_or(5);
        if search.len() > 1024 || !(1..=100).contains(&limit) {
            return Err(super::error::ApiError::bad("账户搜索参数无效"));
        }
        Ok((search, limit))
    }
}
#[derive(Serialize)]
pub struct Session {
    pub authenticated: bool,
    pub app_version: &'static str,
    pub codex_cli_version: &'static str,
    pub grok_build_version: &'static str,
    pub antigravity_version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub csrf_token: Option<String>,
}
#[derive(Serialize)]
pub struct SubscriptionAccount {
    pub revision: i64,
    pub user_id: String,
    pub id: String,
    pub provider_id: String,
    pub username: String,
    pub name: String,
    pub email: String,
    pub plan_id: String,
    pub plan_type: String,
    pub subscription_expires_at: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub plan_name: String,
    pub subscription_status: &'static str,
}
#[derive(Serialize)]
pub struct Plan {
    pub rpm_limit: u32,
    pub concurrency_limit: u32,
    pub description: String,
    pub tag_ids: Vec<String>,
    pub sale_price_usd: Option<String>,
    pub duration_days: i64,
    pub plan_type: String,
    pub id: String,
    pub provider_id: String,
    pub name: String,
    pub allow_purchase: bool,
    pub revision: i64,
    pub updated_at_ms: i64,
    pub models: Vec<codex2api_common::ModelRule>,
    pub spending_windows: Vec<SpendingWindow>,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct SpendingWindow {
    pub duration_seconds: i64,
    pub cost_limit_usd: Option<String>,
}
#[derive(Serialize)]
pub struct TokenPrice {
    pub tier: String,
    pub min_input_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<i64>,
    pub input_rate: String,
    pub cached_rate: String,
    pub cache_write_rate: String,
    pub output_rate: String,
}
#[derive(Serialize)]
pub struct ImagePrice {
    pub resolution: String,
    pub price: String,
}
#[derive(Serialize)]
pub struct ModelPreset {
    pub platform: String,
    pub model: String,
    pub kind: String,
    pub version: &'static str,
    pub source_url: Option<String>,
    pub verified_at: Option<&'static str>,
    pub token_prices: Vec<TokenPrice>,
}
#[derive(Serialize)]
pub struct Model {
    pub platform: String,
    pub model: String,
    pub kind: String,
    pub enabled: bool,
    pub revision: i64,
    pub token_prices: Vec<TokenPrice>,
    pub image_prices: Vec<ImagePrice>,
}
#[derive(Serialize)]
pub struct Items<T> {
    pub items: Vec<T>,
}
#[derive(Serialize)]
pub struct Usage {
    pub records: Vec<codex2api_storage::UsageRecord>,
    pub total: i64,
    pub page: u32,
    pub page_size: i64,
}
#[derive(Serialize)]
pub struct Route {
    /// Explicit override; absent while the subscription inherits its plan's pool.
    pub tag_ids: Option<Vec<String>>,
    /// The pool actually used: the override or the plan's tags.
    pub effective_tag_ids: Vec<String>,
    pub virtual_account_id: String,
    pub provider_id: String,
    pub revision: i64,
}
#[derive(Serialize)]
pub struct Routes {
    pub items: Vec<Route>,
}
pub fn value<T: Serialize>(v: T) -> serde_json::Value {
    serde_json::to_value(v).expect("admin DTO serialization")
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct OptionsQuery {
    pub provider_id: String,
    pub platform: String,
    pub search: String,
    pub plan_id: String,
}
