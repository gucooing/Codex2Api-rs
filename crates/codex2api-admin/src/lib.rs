//! Administrator JSON API. Cookie sessions and CSRF checks are independent of consumer OAuth.
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
    rest::router(state)
}
