//! SQLite persistence for administrators, provider accounts, virtual accounts and usage.
//!
//! `Storage` is the repository entry point; `types` contains persisted records;
//! `password` and `token` implement credential primitives. Migrations stay in
//! migrations/. This crate does not register HTTP routes or call official services.

mod billing;
mod model_catalog;
mod model_presets;
pub use model_presets::preset_model_prices;
mod token;
pub use model_catalog::{
    IMAGE_RESOLUTION_TIERS, ImagePrice, ImageUsage, ModelConfig, image_resolution,
    image_resolution_tier,
};
mod entitlements;
mod error;
mod execution_routes;
pub use entitlements::EffectiveEntitlements;
pub use execution_routes::ExecutionRoute;
mod client_fields;
mod client_ownership;
mod consumer_management;
mod desktop_support;
mod oauth;
mod oauth_authorization;
mod oauth_device;
mod providers;
pub use oauth_device::{
    DeviceAuthorization, DeviceAuthorizationApproval, DeviceAuthorizationCode,
    DeviceAuthorizationPoll, GrokDevicePoll,
};
mod password;
mod proxy;
mod quota;
mod remote_servers;
mod reset_credits;
pub use consumer_management::{ConsumerFilters, ConsumerSelection, ResetCardGrant};
mod public_urls;
mod request_limits;
mod settings;
mod spending_windows;
mod storage;
mod supplier_health;
mod supplier_pools;
pub use request_limits::VirtualRpmLimit;
pub use supplier_pools::SupplierTag;
mod supplier_routing;
pub use spending_windows::{SpendingWindow, plan_spending_windows, spending_windows};
pub use supplier_health::SupplierHealth;
pub use supplier_routing::SupplierAuthSnapshot;
mod types;
mod usage;
mod usage_statistics;
pub use usage_statistics::{UsageGroup, UsageStatistics, UsageStatisticsRow, UsageTotals};
mod free_plan;
mod jwt;
mod user_subscriptions;
mod users;
mod wallet;
pub use jwt::TokenPurpose;
pub use wallet::{WalletAdjustment, WalletEntryFilter};
mod user_store;
mod user_usage;
pub use user_store::UserStore;
pub use user_usage::UserUsageFilter;
mod virtual_accounts;
mod virtual_plans;
pub use user_subscriptions::{SubscriptionChange, UserSubscription, WalletEntry};
mod subscription_orders;
pub use subscription_orders::{CheckoutInput, OrderFilter, OrderRequest, SubscriptionOrder};
mod coupons;
pub use coupons::{Coupon, CouponInput};
pub use users::{User, UserSession, UserView};
pub use virtual_plans::{VirtualPlan, plan_owned_config};
mod family_notices;
mod virtual_client_state;
mod virtual_management;
pub use family_notices::{FamilyGraduationNotice, FamilyNoticeRecipient};

pub use billing::{ModelPrice, decimal_units, format_units, price_tier};
pub use client_fields::{
    ClientField, client_fields, prepare_client_config, statsig_hash, validate_client_fields,
};
pub use client_ownership::{
    admin_client_fields, client_state_only, client_writable, validate_admin_client_change,
    validate_client_change,
};
pub use desktop_support::{DesktopResource, DesktopSupportSettings};
pub use error::{Result, StorageError};
pub use oauth::{OAuthDeviceIdentity, oauth_secret};
pub use password::{hash_password, verify_password};
pub use proxy::{OutboundProxy, ProxyConnectionCheck, ProxyQualityCheck, parse_proxy_url};
pub use public_urls::{PublicUrlSettings, normalize_public_origin};
pub use quota::{QuotaSnapshot, SupplierInfoSection};
pub use remote_servers::{RemoteServer, RemoteServerRegistration};
pub use settings::{GatewaySettings, UaMode};
pub use storage::{DEFAULT_ADMIN_SESSION_TTL, DEFAULT_OAUTH_PENDING_TTL, Storage};
pub use token::hash_token;
pub use types::{
    AdminSession, AdminUser, NewSupplierAccount, OAuthPending, SupplierAccount,
    SupplierAccountUpdate, SupplierRuntime, SupplierStatus, SupplierTokens,
};
pub use usage::{
    AccountDailyUsage, AccountUsageSummary, SupplierCycleUsage, USAGE_PAGE_SIZE, UsageFilter,
    UsagePage, UsageRecord, VirtualDailyModelTokens, table_page_size,
};
pub use virtual_accounts::{MissingEndpoint, VirtualAccess, VirtualAccount, VirtualDevice};
pub use virtual_client_state::VirtualClientState;
pub use virtual_management::{VirtualConfigSpec, validate_virtual_config, virtual_config_specs};

pub const DEFAULT_ADMIN_USERNAME: &str = "admin";
pub const DEFAULT_ADMIN_PASSWORD: &str = "admin";
pub const DEFAULT_DB_PATH: &str = "data/codex2api.sqlite";

mod virtual_contracts;
pub use virtual_contracts::{
    editable_virtual_contract, prepare_virtual_contract, validate_virtual_contract,
};

pub use oauth_authorization::{BrowserAuthorization, CodeRedemption};
