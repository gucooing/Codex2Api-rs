//! PostgreSQL persistence for administrators, provider accounts, virtual accounts and usage.
//!
//! `Storage` is the repository entry point; `types` contains persisted records;
//! `password` and `token` implement credential primitives. `migrations/0000_baseline.sql`
//! creates fresh databases and numbered migrations evolve every database. This crate
//! does not register HTTP routes or call official services.

mod account_scope;
pub use account_scope::AccountScope;
mod admission_cache;
mod api_access;
mod api_admission;
pub use admission_cache::{AdmissionError, ApiLimits, ApiPermit};
pub use api_admission::chatgpt_only_endpoint;
mod api_groups;
mod usage_wallet;
pub use api_access::{ApiKeyView, ApiPrincipal, CreatedApiKey};
pub use api_groups::{
    ApiCatalogModel, ApiGroup, ApiImagePrice, ApiListQuery, ApiPublicGroup, ApiTokenPrice,
    multiplier_micros,
};
#[cfg(test)]
extern crate self as codex2api_storage;
mod billing;
mod billing_cache;
mod dispatch;
pub use dispatch::SupplierPool;
mod operation_prices;
pub use operation_prices::OperationPrice;
mod list_query;
mod management_lists;
mod management_records;
mod model_catalog;
pub use list_query::{ListPage, ListQuery};
mod model_presets;
pub use model_presets::preset_model_prices;
mod token;
pub use model_catalog::{
    IMAGE_RESOLUTION_TIERS, IMAGE_UNIFORM_RESOLUTION, ImagePrice, ImageUsage, ModelConfig,
    image_resolution, image_resolution_tier,
};
mod entitlements;
mod error;
mod execution_routes;
pub use entitlements::EffectiveEntitlements;
pub use execution_routes::{ExecutionRoute, RouteOverride};
mod client_fields;
mod client_ownership;
mod desktop_support;
mod oauth;
mod oauth_authorization;
mod oauth_device;
mod providers;
mod subscription_management;
pub use oauth_device::{
    DeviceAuthorization, DeviceAuthorizationApproval, DeviceAuthorizationCode,
    DeviceAuthorizationPoll, GrokDevicePoll,
};
mod password;
mod proxy;
mod quota;
mod remote_servers;
mod reset_credits;
pub use subscription_management::{ResetCardGrant, SubscriptionFilters, SubscriptionSelection};
#[cfg(test)]
mod cache_tests;
mod public_urls;
mod redis_store;
mod request_limits;
mod request_writer;
mod routing_cache;
mod runtime;
mod snapshots;
pub use routing_cache::RoutingKey;
mod supplier_models;
mod supplier_transfer;
pub use supplier_transfer::{SupplierTransferAccount, SupplierTransferProxy};
mod supplier_transport;
pub use runtime::{RuntimeMetrics, RuntimePermit};
pub use supplier_models::{SupplierModel, SupplierScheduling};
pub use supplier_transport::SupplierTransportSnapshot;
mod settings;
mod spending_windows;
mod storage;
mod supplier_health;
mod supplier_pools;
pub use request_limits::VirtualRpmLimit;
pub use supplier_pools::PoolCandidate;
pub use supplier_pools::SupplierTag;
mod supplier_routing;
pub use spending_windows::{SpendingWindow, plan_spending_windows, spending_windows};
pub use supplier_health::{SupplierHealth, SupplierModelCooldown};
pub use supplier_routing::SupplierAuthSnapshot;
mod types;
mod usage;
mod usage_statistics;
pub use usage_statistics::{UsageGroup, UsageStatistics, UsageStatisticsRow, UsageTotals};
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
mod platform_accounts;
mod virtual_plans;
pub use user_subscriptions::{SubscriptionChange, UserSubscription, WalletEntry};
mod subscription_orders;
pub use subscription_orders::{CheckoutInput, OrderFilter, OrderRequest, SubscriptionOrder};
mod coupons;
pub use coupons::{Coupon, CouponInput};
pub use users::{User, UserKind, UserSession, UserView};
mod web_sessions;
pub use virtual_plans::{VirtualPlan, plan_owned_config};
pub use web_sessions::{WEB_ACCESS_TTL_SECONDS, WEB_REFRESH_TTL_SECONDS, WebSessionTokens};
mod family_notices;
mod virtual_client_state;
mod virtual_management;
pub use family_notices::{FamilyGraduationNotice, FamilyNoticeRecipient};

pub use billing::{
    ModelPrice, PriceRequirement, UsageCalculation, decimal_units, format_units, price_tier,
};
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
pub use platform_accounts::{MissingEndpoint, PlatformAccount, VirtualAccess, VirtualDevice};
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
    UsagePage, UsageRecord, UsageSource, VirtualDailyModelTokens, table_page_size,
};
pub use virtual_client_state::VirtualClientState;
pub use virtual_management::{VirtualConfigSpec, validate_virtual_config, virtual_config_specs};

pub const DEFAULT_ADMIN_USERNAME: &str = "admin";
pub const DEFAULT_ADMIN_PASSWORD: &str = "admin";

mod virtual_contracts;
pub use virtual_contracts::{
    editable_virtual_contract, prepare_virtual_contract, validate_virtual_contract,
};

pub use oauth_authorization::{BrowserAuthorization, CodeRedemption};

#[cfg(feature = "test-support")]
pub mod test_support;

mod pg_types;
