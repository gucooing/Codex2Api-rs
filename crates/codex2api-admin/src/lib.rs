//! Administrator management API and embedded website, with independent sessions and CSRF.
mod frontend;
mod providers;
mod proxy_checks;
mod quota;
mod quota_monitor;
mod rest;
mod session;
mod state;
pub use state::AdminState;
pub const SESSION_COOKIE: &str = session::SESSION_COOKIE;
pub fn router(state: AdminState) -> axum::Router {
    api_router(state).merge(frontend::router())
}
/// Administrator REST only, for composition without the website's root redirect.
pub fn api_router(state: AdminState) -> axum::Router {
    rest::router(state)
}
