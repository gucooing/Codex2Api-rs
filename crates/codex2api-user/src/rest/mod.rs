mod devices;
mod orders;
mod profile;
mod subscriptions;
mod usage;
mod wallet;
use crate::{UserState, auth, oauth};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
pub(crate) fn router(state: UserState) -> Router {
    let protected = Router::new()
        .route("/session", get(profile::session))
        .route("/logout", post(auth::logout))
        .route("/password", post(profile::password))
        .route("/plans", get(subscriptions::plans))
        .route("/subscriptions", get(subscriptions::subscriptions))
        .route("/usage", get(usage::usage))
        .route("/checkout/preview", post(orders::preview))
        .route("/orders", get(orders::orders).post(orders::create_order))
        .route("/orders/plans", get(orders::order_plans))
        .route("/orders/{id}", get(orders::order))
        .route("/orders/{id}/pay", post(orders::pay_order))
        .route("/orders/{id}/cancel", post(orders::cancel_order))
        .route("/wallet", get(wallet::wallet))
        .route("/devices", get(devices::devices))
        .route("/devices/{id}/revoke", post(devices::revoke_device))
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
