//! User business API, browser consent and embedded website, restricted to UserStore.
mod auth;
mod error;
mod frontend;
mod oauth;
mod rest;
use codex2api_storage::UserStore;

#[derive(Clone)]
pub struct UserState {
    pub storage: UserStore,
    pub public_base_url: String,
    pub api_base_url: String,
    pub api_concurrency: std::sync::Arc<dyn Fn(&str) -> u32 + Send + Sync>,
}
pub fn router(state: UserState) -> axum::Router {
    api_router(state).merge(frontend::router())
}
/// User REST and consent only, without the website's root redirect.
pub fn api_router(state: UserState) -> axum::Router {
    rest::router(state)
}
