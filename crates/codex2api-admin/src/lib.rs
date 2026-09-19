//! Admin HTTP entry point. All website routes and access boundaries are listed here.
//!
//! handlers/ handles requests; services.rs calls official services;
//! views/ renders HTML; models.rs carries page data;
//! state.rs owns dependencies/OAuth progress; session.rs owns cookie authentication.
//! The public Codex API is registered separately in codex2api-api.

mod handlers;
mod models;
mod proxy_checks;
mod quota;
mod response;
mod services;
mod session;
mod state;
mod views;

use axum::Router;
use axum::routing::{get, post};
use handlers::{
    accounts, auth, fingerprint, keys, oauth, oauth_credentials, official, proxies, settings,
    turn_state, usage_records,
};

pub use state::AdminState;
pub const SESSION_COOKIE: &str = session::SESSION_COOKIE;

/// Mount all administrator routes, protecting account operations with a session.
pub fn router(state: AdminState) -> Router {
    let protected = Router::new()
        // Account list, account state and proxy keys.
        .route("/admin", get(accounts::dashboard))
        .route("/admin/", get(accounts::dashboard))
        .route(
            "/admin/settings",
            get(settings::gateway_page).post(settings::save_gateway),
        )
        .route(
            "/admin/settings/security",
            get(settings::page).post(settings::save),
        )
        .route("/admin/usage", get(usage_records::page))
        .route("/admin/keys", get(keys::page).post(keys::create))
        .route("/admin/keys/{key_id}/{action}", post(keys::action))
        .route(
            "/admin/oauth/credentials",
            get(oauth_credentials::page).post(oauth_credentials::create),
        )
        .route(
            "/admin/oauth/credentials/{id}/{action}",
            post(oauth_credentials::action),
        )
        .route(
            "/admin/oauth/accounts/{account_id}",
            get(oauth_credentials::detail),
        )
        .route(
            "/admin/oauth/accounts/{account_id}/delete",
            post(oauth_credentials::delete_account),
        )
        .route("/admin/proxies", get(proxies::page).post(proxies::create))
        .route(
            "/admin/proxies/{id}/edit",
            get(proxies::edit).post(proxies::update),
        )
        .route("/admin/proxies/{id}/{action}", post(proxies::check))
        .route("/admin/proxies/{id}/delete", post(proxies::delete))
        // Account tabs: info / fingerprint / usage (official) / details.
        .route("/admin/accounts/{id}", get(accounts::account_page))
        .route("/admin/accounts/{id}/quota", get(accounts::account_quota))
        .route("/admin/accounts/{id}/fingerprint", post(fingerprint::save))
        .route("/admin/accounts/{id}/turn-state", post(turn_state::save))
        .route(
            "/admin/accounts/{id}/turn-state/clear",
            post(turn_state::clear),
        )
        .route(
            "/admin/accounts/{id}/delete",
            post(accounts::delete_account),
        )
        .route(
            "/admin/accounts/{id}/enable",
            post(accounts::enable_account),
        )
        .route(
            "/admin/accounts/{id}/disable",
            post(accounts::disable_account),
        )
        // Compatibility GET redirects to account tabs; POST consumes a reset credit.
        .route(
            "/admin/accounts/{id}/official",
            get(official::page).post(official::action),
        )
        // OAuth starts require an admin session.
        .route("/admin/oauth", get(oauth::oauth_pending_page))
        .route("/admin/oauth/setup", get(oauth::oauth_setup))
        .route("/admin/oauth/start", post(oauth::oauth_start))
        .route("/admin/oauth/cancel", post(oauth::cancel_draft))
        .route("/admin/oauth/callback", post(oauth::submit_callback))
        .route("/admin/oauth/device/poll", post(oauth::poll_device))
        .route("/admin/accounts/{id}/relogin", post(oauth::oauth_relogin))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            session::require_session,
        ));

    Router::new()
        .route("/admin/login", get(auth::login_page).post(auth::login_post))
        .route("/admin/logout", post(auth::logout))
        .merge(protected)
        .with_state(state)
}
