//! Application composition: both HTTP surfaces share one account/auth/upstream context.
//! Actual endpoint lists live in codex2api-api::routes and codex2api-admin::router.
//! main.rs owns environment settings, storage startup, listening and shutdown.

use anyhow::Result;
use axum::Router;
use codex2api_accounts::AccountStore;
use codex2api_auth::AuthService;
use codex2api_storage::Storage;
use codex2api_upstream::UpstreamPool;

pub fn router(storage: Storage) -> Result<Router> {
    let accounts = AccountStore::open(storage.clone());
    let auth = AuthService::new(accounts.clone())?;
    let upstream = UpstreamPool::new(auth.clone());
    let api_state =
        codex2api_api::ApiState::new(storage.clone(), accounts.clone(), upstream.clone());
    let admin_state =
        codex2api_admin::AdminState::from_parts(storage, accounts, auth).with_upstream(upstream);

    Ok(Router::new()
        .merge(codex2api_api::router(api_state))
        .merge(codex2api_admin::router(admin_state)))
}
