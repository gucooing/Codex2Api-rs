//! User website API. No administrator or supplier services are available in this module.
mod auth;
mod error;
mod oauth;
mod providers;
mod rest;
use codex2api_storage::UserStore;
pub use oauth::AuthorizationRequest;

#[derive(Clone)]
pub struct UserState {
    pub storage: UserStore,
    pub public_base_url: String,
}
pub fn router(state: UserState) -> axum::Router {
    rest::router(state)
}
