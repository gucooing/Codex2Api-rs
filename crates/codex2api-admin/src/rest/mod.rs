mod auth;
mod catalog;
mod consumers;
mod dto;
pub(crate) mod error;
mod proxies;
mod settings;
mod suppliers;
mod usage;
use crate::{AdminState, session};
use axum::{
    Router,
    routing::{get, post, put},
};
pub(crate) fn router(state: AdminState) -> Router {
    let protected = Router::new()
        .route("/logout", post(auth::logout))
        .route("/overview", get(settings::overview))
        .route("/suppliers", get(suppliers::list))
        .route("/suppliers/oauth/setup", get(suppliers::setup))
        .route("/suppliers/oauth/start", post(suppliers::start))
        .route("/suppliers/oauth/callback", post(suppliers::callback))
        .route("/suppliers/oauth/poll", post(suppliers::poll))
        .route("/suppliers/oauth/cancel", post(suppliers::cancel))
        .route(
            "/suppliers/{id}",
            get(suppliers::detail).delete(suppliers::delete),
        )
        .route("/suppliers/{id}/status", post(suppliers::status))
        .route("/suppliers/{id}/recover", post(suppliers::recover))
        .route("/suppliers/{id}/quota", get(suppliers::quota))
        .route("/suppliers/{id}/fingerprint", put(suppliers::fingerprint))
        .route("/suppliers/{id}/official", get(suppliers::official))
        .route("/suppliers/{id}/credits/consume", post(suppliers::credit))
        .route("/suppliers/{id}/relogin", post(suppliers::relogin))
        .route("/consumers", get(consumers::list).post(consumers::create))
        .route("/consumers/batch", post(consumers::batch))
        .route(
            "/consumers/{id}",
            get(consumers::detail)
                .put(consumers::update)
                .delete(consumers::delete),
        )
        .route("/consumers/{id}/usage", get(consumers::usage))
        .route(
            "/consumers/{id}/reset-credits",
            get(consumers::reset_credits).post(consumers::grant_reset_credits),
        )
        .route(
            "/consumers/{id}/reset-credits/consume",
            post(consumers::consume_reset_credit),
        )
        .route("/consumers/{id}/configs", get(consumers::configs))
        .route("/consumers/{id}/plugins", get(consumers::plugins))
        .route("/consumers/{id}/connectors", get(consumers::connectors))
        .route("/consumers/{id}/config/{key}", put(consumers::save_config))
        .route(
            "/consumers/{id}/client-state/{key}",
            get(consumers::client_state),
        )
        .route("/consumers/{id}/devices", get(consumers::devices))
        .route(
            "/consumers/{id}/devices/{device}/revoke",
            post(consumers::revoke),
        )
        .route(
            "/consumers/{id}/routing",
            get(consumers::routes).put(consumers::save_route),
        )
        .route("/consumers/{id}/logs", get(consumers::logs))
        .route("/consumers/{id}/records", get(consumers::records))
        .route("/plans", get(catalog::plans).post(catalog::create_plan))
        .route(
            "/plans/{id}",
            put(catalog::update_plan).delete(catalog::delete_plan),
        )
        .route("/models", get(catalog::models).post(catalog::save_model))
        .route("/models/status", post(catalog::model_status))
        .route("/models/delete", post(catalog::delete_model))
        .route("/usage", get(usage::page))
        .route("/proxies", get(proxies::list).post(proxies::create))
        .route(
            "/proxies/{id}",
            get(proxies::detail)
                .put(proxies::update)
                .delete(proxies::delete),
        )
        .route("/proxies/{id}/check/{action}", post(proxies::check))
        .route(
            "/settings/gateway",
            get(settings::gateway).put(settings::save_gateway),
        )
        .route(
            "/settings/security",
            get(settings::security).put(settings::save_security),
        )
        .route(
            "/settings/desktop",
            get(settings::desktop).put(settings::save_desktop),
        )
        .route("/diagnostics", get(settings::diagnostics))
        .route("/resources", get(settings::resources))
        .route("/missing-endpoints", get(settings::missing))
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
