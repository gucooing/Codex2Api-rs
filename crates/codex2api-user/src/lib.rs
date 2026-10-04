//! User website API. No administrator or supplier services are available in this module.
mod auth;
mod error;
mod oauth;
mod providers;
mod rest;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
use codex2api_storage::UserStore;
pub use oauth::AuthorizationRequest;

#[derive(Clone)]
pub struct UserState {
    pub storage: UserStore,
    pub public_base_url: String,
}
pub fn router(state: UserState) -> Router {
    let protected = Router::new()
        .route("/session", get(rest::session))
        .route("/logout", post(auth::logout))
        .route("/password", post(rest::password))
        .route("/plans", get(rest::plans))
        .route("/subscriptions", get(rest::subscriptions))
        .route("/usage", get(rest::usage))
        .route("/checkout/preview", post(rest::preview))
        .route("/orders", get(rest::orders).post(rest::create_order))
        .route("/orders/plans", get(rest::order_plans))
        .route("/orders/{id}", get(rest::order))
        .route("/orders/{id}/pay", post(rest::pay_order))
        .route("/orders/{id}/cancel", post(rest::cancel_order))
        .route("/wallet", get(rest::wallet))
        .route("/devices", get(rest::devices))
        .route("/devices/{id}/revoke", post(rest::revoke_device))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_session,
        ));
    Router::new()
        .nest(
            "/user/api",
            Router::new()
                .merge(protected)
                .route("/login", post(auth::login))
                .route("/oauth/authorize/bootstrap", get(oauth::bootstrap))
                .route("/oauth/authorize/identify", post(oauth::identify))
                .route("/oauth/authorize/reset", post(oauth::reset))
                .route("/oauth/authorize/cancel", post(oauth::cancel))
                .route("/oauth/authorize/approve", post(oauth::approve))
                .route("/oauth/device/bootstrap", get(oauth::device_bootstrap))
                .route("/oauth/device/identify", post(oauth::identify))
                .route("/oauth/device/reset", post(oauth::reset))
                .route("/oauth/device/cancel", post(oauth::cancel))
                .route("/oauth/device/approve", post(oauth::device_approve)),
        )
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::browser_boundary,
        ))
        .with_state(state)
}
