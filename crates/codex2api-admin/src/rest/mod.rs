mod account_scope;
mod auth;
mod catalog;
mod consumers;
mod coupons;
mod dto;
pub(crate) mod error;
mod orders;
mod proxies;
mod settings;
mod suppliers;
mod tags;
mod usage;
mod users;
mod wallet;
use crate::{AdminState, session};
use axum::{
    Router,
    routing::{get, post, put},
};
pub(crate) fn router(state: AdminState) -> Router {
    let user_routes = Router::new()
        .route("/users", get(users::list).post(users::create))
        .route("/users/options", get(users::options))
        .route("/users/{id}/wallet-adjustments", post(users::adjust_wallet))
        .route("/users/{id}", get(users::detail).put(users::update));
    let virtual_routes = platform_resources("/consumers")
        .route("/consumers", get(consumers::list).post(consumers::create))
        .route("/consumers/batch", post(consumers::batch))
        .route(
            "/consumers/{id}",
            get(consumers::detail)
                .put(consumers::update)
                .delete(consumers::delete),
        );
    let subscription_routes = platform_resources("/subscriptions").route(
        "/subscriptions/{id}",
        get(consumers::detail).put(users::update_subscription),
    );
    let protected = Router::new()
        .merge(user_routes)
        .merge(virtual_routes)
        .merge(subscription_routes)
        .route(
            "/suppliers/grok/{id}/profile",
            post(crate::providers::grok::refresh_profile),
        )
        .nest(
            "/suppliers/chatgpt/oauth",
            crate::providers::chatgpt_oauth_routes(),
        )
        .nest(
            "/suppliers/grok/oauth",
            crate::providers::grok::oauth_routes(),
        )
        .route(
            "/suppliers/grok/{id}/models",
            get(crate::providers::grok::model_catalog)
                .post(crate::providers::grok::sync_model_catalog),
        )
        .route(
            "/models/grok/sync",
            post(crate::providers::grok::sync_all_models),
        )
        .route("/wallet-entries", get(wallet::list))
        .route("/coupons", get(coupons::list).post(coupons::create))
        .route("/coupons/{id}", put(coupons::update))
        .route("/orders", get(orders::list))
        .route("/orders/plans", get(orders::plans))
        .route("/orders/{id}", get(orders::detail))
        .route("/orders/{id}/cancel", post(orders::cancel))
        .route(
            "/subscriptions",
            get(users::subscriptions).post(users::grant),
        )
        .route("/logout", post(auth::logout))
        .route("/overview", get(settings::overview))
        .route("/overview/usage", get(usage::statistics))
        .route("/suppliers", get(suppliers::list))
        .route("/supplier-tags", get(tags::list).post(tags::create))
        .route(
            "/supplier-tags/{id}",
            put(tags::update).delete(tags::delete),
        )
        .route("/suppliers/tags", post(tags::batch))
        .route(
            "/suppliers/oauth/setup",
            get(crate::providers::chatgpt::setup),
        )
        .route(
            "/suppliers/oauth/start",
            post(crate::providers::chatgpt::start),
        )
        .route(
            "/suppliers/oauth/callback",
            post(crate::providers::chatgpt::callback),
        )
        .route(
            "/suppliers/oauth/poll",
            post(crate::providers::chatgpt::poll),
        )
        .route(
            "/suppliers/oauth/cancel",
            post(crate::providers::chatgpt::cancel),
        )
        .route(
            "/suppliers/{id}",
            get(suppliers::detail).delete(suppliers::delete),
        )
        .route("/suppliers/{id}/status", post(suppliers::status))
        .route("/suppliers/{id}/reset-state", post(suppliers::reset_state))
        .route("/suppliers/{id}/quota", get(suppliers::quota))
        .route(
            "/suppliers/{id}/fingerprint",
            put(crate::providers::fingerprint),
        )
        .route("/suppliers/{id}/official", get(crate::providers::official))
        .route(
            "/suppliers/{id}/credits/consume",
            post(crate::providers::credit),
        )
        .route("/suppliers/{id}/relogin", post(crate::providers::relogin))
        .route("/plans", get(catalog::plans).post(catalog::create_plan))
        .route(
            "/plans/{id}",
            put(catalog::update_plan).delete(catalog::delete_plan),
        )
        .route("/models", get(catalog::models).post(catalog::save_model))
        .route("/models/presets", get(catalog::model_presets))
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
            "/settings/public-urls",
            get(settings::public_urls).put(settings::save_public_urls),
        )
        .route(
            "/settings/security",
            get(settings::security).put(settings::save_security),
        )
        .route(
            "/settings/desktop",
            get(settings::desktop).put(settings::save_desktop),
        )
        .route("/resources", get(settings::resources))
        .route("/missing-endpoints", get(settings::missing))
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

fn platform_resources(prefix: &str) -> Router<AdminState> {
    Router::new()
        .route(
            &format!("{prefix}/{{id}}/rate-limit"),
            get(consumers::rate_limit).put(consumers::save_rate_limit),
        )
        .route(&format!("{prefix}/{{id}}/usage"), get(consumers::usage))
        .route(
            &format!("{prefix}/{{id}}/reset-credits"),
            get(consumers::reset_credits).post(consumers::grant_reset_credits),
        )
        .route(
            &format!("{prefix}/{{id}}/reset-credits/consume"),
            post(consumers::consume_reset_credit),
        )
        .route(&format!("{prefix}/{{id}}/configs"), get(consumers::configs))
        .route(&format!("{prefix}/{{id}}/plugins"), get(consumers::plugins))
        .route(
            &format!("{prefix}/{{id}}/connectors"),
            get(consumers::connectors),
        )
        .route(
            &format!("{prefix}/{{id}}/config/{{key}}"),
            put(consumers::save_config),
        )
        .route(
            &format!("{prefix}/{{id}}/client-state/{{key}}"),
            get(consumers::client_state),
        )
        .route(&format!("{prefix}/{{id}}/devices"), get(consumers::devices))
        .route(
            &format!("{prefix}/{{id}}/devices/{{device}}/revoke"),
            post(consumers::revoke),
        )
        .route(
            &format!("{prefix}/{{id}}/routing"),
            get(consumers::routes).put(consumers::save_route),
        )
        .route(&format!("{prefix}/{{id}}/logs"), get(consumers::logs))
        .route(&format!("{prefix}/{{id}}/records"), get(consumers::records))
}
