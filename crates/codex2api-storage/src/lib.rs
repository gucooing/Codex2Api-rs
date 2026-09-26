//! SQLite persistence for administrators, provider accounts, virtual accounts and usage.
//!
//! `Storage` is the repository entry point; `types` contains persisted records;
//! `password` and `token` implement credential primitives. Migrations stay in
//! migrations/. This crate does not register HTTP routes or call official services.

mod billing;
mod model_catalog;
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
mod password;
mod proxy;
mod quota;
mod remote_servers;
mod reset_credits;
pub use consumer_management::{ConsumerFilters, ConsumerSelection, ResetCardGrant};
mod settings;
mod spending_windows;
mod storage;
mod supplier_health;
mod supplier_routing;
pub use spending_windows::{SpendingWindow, plan_spending_windows, spending_windows};
pub use supplier_health::SupplierHealth;
pub use supplier_routing::SupplierAuthSnapshot;
mod types;
mod usage;
mod virtual_accounts;
mod virtual_plans;
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
