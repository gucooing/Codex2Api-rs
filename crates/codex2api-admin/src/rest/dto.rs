//! Explicit administrator response contracts. These types never contain credentials.
use serde::{Deserialize, Serialize};
#[derive(Deserialize)]
pub struct AccountListQuery {
    pub search: Option<String>,
    pub limit: Option<u32>,
    pub provider_id: Option<String>,
    #[serde(default)]
    pub for_routing: bool,
}
impl AccountListQuery {
    pub fn search_params(&self) -> Result<Option<(&str, u32)>, super::error::ApiError> {
        if self.search.is_none()
            && self.limit.is_none()
            && self.provider_id.is_none()
            && !self.for_routing
        {
            return Ok(None);
        }
        let search = self.search.as_deref().unwrap_or("").trim();
        let limit = self.limit.unwrap_or(5);
        if search.len() > 1024 || !(1..=100).contains(&limit) {
            return Err(super::error::ApiError::bad("账户搜索参数无效"));
        }
        Ok(Some((search, limit)))
    }
}
#[derive(Serialize)]
pub struct Session {
    pub authenticated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub csrf_token: Option<String>,
}
#[derive(Serialize)]
pub struct Overview {
    pub supplier_count: usize,
    pub consumer_count: usize,
    pub enabled_consumers: usize,
    pub models_count: usize,
}
#[derive(Serialize)]
pub struct Consumer {
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
    pub effective_plan: String,
    pub plan_name: String,
    pub subscription_status: &'static str,
}
impl From<&codex2api_storage::VirtualAccount> for Consumer {
    fn from(a: &codex2api_storage::VirtualAccount) -> Self {
        Self {
            id: a.id.clone(),
            provider_id: a.provider_id.clone(),
            username: a.username.clone(),
            name: a.name.clone(),
            email: a.email.clone(),
            plan_id: a.plan_id.clone(),
            plan_type: a.plan_type.clone(),
            subscription_expires_at: a.subscription_expires_at.clone(),
            enabled: a.enabled,
            created_at: a.created_at.clone(),
            effective_plan: a.effective_plan().into(),
            plan_name: String::new(),
            subscription_status: if a.effective_plan() != "free" {
                "active"
            } else if a.plan_type != "free" {
                "expired"
            } else {
                "free"
            },
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRef {
    pub provider_id: String,
    pub model: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub id: String,
    pub provider_id: String,
    pub name: String,
    pub enabled: bool,
    pub revision: i64,
    pub updated_at_ms: i64,
    pub model_access: String,
    pub models: Vec<ModelRef>,
    pub free_model_access: String,
    pub free_models: Vec<ModelRef>,
    pub free_access_enabled: bool,
    pub primary_cost_limit_usd: Option<String>,
    pub weekly_cost_limit_usd: Option<String>,
    pub free_primary_cost_limit_usd: Option<String>,
    pub free_weekly_cost_limit_usd: Option<String>,
    pub spending_windows: Vec<SpendingWindow>,
    pub free_spending_windows: Vec<SpendingWindow>,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct SpendingWindow {
    pub duration_seconds: i64,
    pub cost_limit_usd: Option<String>,
}
#[derive(Serialize)]
pub struct Plans {
    pub items: Vec<Plan>,
}
#[derive(Serialize)]
pub struct TokenPrice {
    pub tier: String,
    pub min_input_tokens: i64,
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
pub struct Model {
    pub provider_id: String,
    pub model: String,
    pub kind: String,
    pub enabled: bool,
    pub revision: i64,
    pub codex_metadata_status: &'static str,
    pub codex_metadata_source: Option<&'static str>,
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
    pub virtual_account_id: String,
    pub provider_id: String,
    pub supplier_account_id: Option<String>,
    pub revision: i64,
}
#[derive(Serialize)]
pub struct Routes {
    pub items: Vec<Route>,
}
pub fn value<T: Serialize>(v: T) -> serde_json::Value {
    serde_json::to_value(v).expect("admin DTO serialization")
}
