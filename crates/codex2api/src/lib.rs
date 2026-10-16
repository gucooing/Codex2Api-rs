//! One process, independent OAuth client, standard AI API and browser listeners.
mod request_debug;
use anyhow::Result;
use axum::Router;
use codex2api_storage::Storage;
use codex2api_suppliers::AuthService;
use codex2api_suppliers::SupplierAccountStore;
use codex2api_upstream::UpstreamPool;

pub struct ApplicationRouters {
    pub oauth: Router,
    pub api: Router,
    pub admin: Router,
    pub user: Router,
    quota_monitor: codex2api_admin::AdminState,
}
impl ApplicationRouters {
    pub fn spawn_quota_monitor(&self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(self.quota_monitor.clone().monitor_supplier_quota())
    }
}
pub fn routers(
    storage: Storage,
    oauth_origin: &str,
    user_origin: &str,
    admin_origin: &str,
    api_origin: &str,
) -> Result<ApplicationRouters> {
    let oauth_origin = validate_origin(oauth_origin)?;
    let user_origin = validate_origin(user_origin)?;
    let admin_origin = validate_origin(admin_origin)?;
    let api_origin = validate_origin(api_origin)?;
    let accounts = SupplierAccountStore::open(storage.clone());
    let auth = AuthService::new(accounts.clone())?;
    let upstream = UpstreamPool::new(auth.clone());
    let api = codex2api_api::ApiState::new(storage.clone(), upstream.clone());
    let api_runtime = api.runtime().clone();
    let mut oauth =
        codex2api_oauth::OAuthState::new(storage.clone(), accounts.clone(), upstream.clone())
            .with_oauth_base_url(&oauth_origin)?;
    oauth.user_base_url = user_origin.clone();
    let mut admin = codex2api_admin::AdminState::from_parts(storage.clone(), accounts, auth)
        .with_upstream(upstream)
        .with_secure_cookies(admin_origin.starts_with("https://"));
    admin.api_concurrency = std::sync::Arc::new(move |key| api_runtime.current_for_key(key));
    admin.public_url_defaults = codex2api_storage::PublicUrlSettings {
        oauth_url: oauth_origin,
        user_url: user_origin.clone(),
        admin_url: admin_origin,
        api_url: api_origin.clone(),
        revision: 0,
    };
    let user = codex2api_user::UserState {
        storage: storage.user_store(),
        public_base_url: user_origin,
        api_base_url: api_origin,
        api_concurrency: admin.api_concurrency.clone(),
    };
    Ok(ApplicationRouters {
        oauth: codex2api_oauth::router(oauth)
            .layer(axum::middleware::from_fn(request_debug::trace)),
        api: codex2api_api::router(api).layer(axum::middleware::from_fn(request_debug::trace)),
        admin: codex2api_admin::router(admin.clone()),
        user: codex2api_user::router(user),
        quota_monitor: admin,
    })
}
pub fn validate_origin(value: &str) -> Result<String> {
    let url = url::Url::parse(value)?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none(),
        "Public URL must be an HTTP(S) origin without path, credentials, query or fragment"
    );
    Ok(url.origin().ascii_serialization())
}
