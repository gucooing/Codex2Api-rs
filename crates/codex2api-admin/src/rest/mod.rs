mod account_scope;
mod api_groups;
mod api_keys;
mod auth;
mod coupons;
mod dto;
pub(crate) mod error;
mod models;
mod orders;
mod overview;
mod plans;
mod platform_accounts;
mod proxies;
mod settings;
mod subscription_accounts;
mod subscriptions;
mod supplier_models;
mod supplier_tags;
pub(crate) mod supplier_transfer;
mod suppliers;
mod usage;
mod users;
mod wallet;
use crate::{AdminState, session};
use axum::{
    Router,
    routing::{get, post},
};
pub(crate) fn router(state: AdminState) -> Router {
    let protected = Router::new()
        .merge(api_groups::router())
        .merge(api_keys::router())
        .merge(users::router())
        .merge(subscriptions::router())
        .merge(models::router())
        .merge(suppliers::router())
        .merge(supplier_models::router())
        .merge(supplier_transfer::router())
        .merge(wallet::router())
        .merge(coupons::router())
        .merge(orders::router())
        .merge(overview::router())
        .merge(supplier_tags::router())
        .merge(plans::router())
        .merge(usage::router())
        .merge(proxies::router())
        .merge(settings::router())
        .route("/logout", post(auth::logout))
        .route_layer(axum::middleware::from_fn_with_state(
            state.storage.clone(),
            account_scope::enforce,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            session::require_session,
        ));
    Router::new()
        .nest(
            "/admin/api",
            Router::new()
                .route("/login", post(auth::login))
                .route("/session", get(auth::current))
                .route("/session/refresh", post(auth::refresh))
                .merge(protected),
        )
        .with_state(state)
        .layer(axum::middleware::from_fn(no_store))
}
async fn no_store(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(req).await;
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response.headers_mut().insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    response
}
