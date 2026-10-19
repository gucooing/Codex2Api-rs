//! ChatGPT protocol constants. Model prices live in the shared built-in model table.
pub mod oauth;
pub mod version;

pub const SEARCH_OPERATION: &str = "search";

pub fn billing_operation(endpoint: &str) -> Option<&'static str> {
    matches!(endpoint, "/v1/alpha/search").then_some(SEARCH_OPERATION)
}

pub const SUBSCRIPTION_TIERS: &[&str] = &[
    "free",
    "go",
    "plus",
    "prolite",
    "pro",
    "promax",
    "team",
    "business",
    "enterprise",
    "edu",
    "edu_plus",
    "edu_pro",
    "self_serve_business_prolite",
    "self_serve_business_usage_based",
    "ent26",
    "enterprise_cbp_automation",
    "enterprise_cbp_usage_based",
];
