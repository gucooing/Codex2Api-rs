//! HTTP entry point for every Codex-client API. All routes are registered here.
//!
//! - `/v1`: inference, search, images, models and realtime (see codex_routes).
//! - `/backend-api/wham`: account, configuration, usage and cloud tasks (see backend_routes).
//! - `/v1/usage`: short alias for WHAM usage.
//! - `/healthz`, `/version`: unauthenticated process metadata.
//!
//! Compatibility prefixes are listed in routes(). Handlers authenticate proxy API keys;
//! upstream owns only outbound official URLs/protocols, never inbound routing.

mod auth;
mod error;
mod handlers;
mod response;
mod state;
mod usage;
mod user_agent;

use axum::Router;
use axum::extract::{DefaultBodyLimit, Extension};
use axum::routing::{get, patch, post};
use codex2api_upstream::{BackendEndpoint, ChatgptEndpoint, Endpoint, RealtimeKind};
use handlers::{backend, chatgpt, codex, oauth, realtime, system, websocket};

pub use error::{ApiError, OpenAiError, OpenAiErrorBody, Result, openai_json};
pub use state::ApiState;

/// Complete Codex-client API, including existing compatibility prefixes.
pub fn routes() -> Router<ApiState> {
    let codex = codex_routes();
    let backend = backend_routes();
    Router::new()
        .route("/healthz", get(system::healthz))
        .route("/version", get(system::version))
        .route(
            "/v1/usage",
            get(backend::forward).layer(Extension(BackendEndpoint::Usage)),
        )
        .nest("/v1", codex.clone())
        .nest("/backend-api/codex", codex)
        .nest("/backend-api/wham", backend.clone())
        .nest("/wham", backend.clone())
        .nest("/api/codex", backend.clone())
        .nest("/v1/api/codex", backend.clone())
        .nest("/v1/wham", backend)
        .layer(DefaultBodyLimit::max(codex2api_upstream::MAX_REQUEST_BYTES))
}

pub fn router(state: ApiState) -> Router {
    let oauth_api = Router::new()
        .nest(
            "/backend-api/codex",
            codex_routes().route(
                "/{call_id}",
                get(realtime::socket).layer(Extension(RealtimeKind::CodexSideband)),
            ),
        )
        .nest("/backend-api/wham", backend_routes())
        .route(
            "/backend-api/accounts/check/v4-2023-04-27",
            get(chatgpt::forward).layer(Extension(ChatgptEndpoint::AccountsCheck)),
        )
        .route(
            "/backend-api/subscriptions",
            get(chatgpt::forward).layer(Extension(ChatgptEndpoint::Subscriptions)),
        )
        .route(
            "/backend-api/settings/account_user_setting",
            patch(chatgpt::forward).layer(Extension(ChatgptEndpoint::Privacy)),
        )
        .route(
            "/v1/responses/input_tokens",
            post(codex::forward).layer(Extension(Endpoint::InputTokens)),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            oauth::require_oauth,
        ))
        .route(
            "/oauth/token",
            post(oauth::token).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/oauth/revoke",
            post(oauth::revoke).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .layer(DefaultBodyLimit::max(codex2api_upstream::MAX_REQUEST_BYTES));
    routes().nest(oauth::PREFIX, oauth_api).with_state(state)
}

/// Relative to /v1 (or /backend-api/codex). GET inference routes upgrade to WebSocket.
fn codex_routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/responses/compact",
            post(codex::forward).layer(Extension(Endpoint::Compact)),
        )
        .route(
            "/responses/input_tokens",
            post(codex::forward).layer(Extension(Endpoint::InputTokens)),
        )
        .route(
            "/responses/{*subpath}",
            post(codex::forward).layer(Extension(Endpoint::Compact)),
        )
        // Inference.
        .route(
            "/responses",
            post(codex::forward)
                .get(websocket::responses_websocket)
                .layer(Extension(Endpoint::Responses)),
        )
        .route(
            "/guardian",
            post(codex::forward)
                .get(websocket::responses_websocket)
                .layer(Extension(Endpoint::Guardian)),
        )
        .route(
            "/guardian-classifier",
            post(codex::forward)
                .get(websocket::responses_websocket)
                .layer(Extension(Endpoint::GuardianClassifier)),
        )
        // Models, search, images and memory.
        .route(
            "/models",
            get(codex::forward).layer(Extension(Endpoint::Models)),
        )
        .route(
            "/alpha/search",
            post(codex::forward).layer(Extension(Endpoint::Search)),
        )
        .route(
            "/images/generations",
            post(codex::forward).layer(Extension(Endpoint::ImageGeneration)),
        )
        .route(
            "/images/edits",
            post(codex::forward).layer(Extension(Endpoint::ImageEdit)),
        )
        .route(
            "/memories/trace_summarize",
            post(codex::forward).layer(Extension(Endpoint::MemorySummary)),
        )
        // Realtime: WebSocket, call creation and existing-call sideband.
        .route(
            "/realtime",
            get(realtime::socket).layer(Extension(RealtimeKind::Realtime)),
        )
        .route(
            "/realtime/calls",
            post(realtime::call).layer(Extension(RealtimeKind::Realtime)),
        )
        .route(
            "/live",
            get(realtime::socket)
                .post(realtime::call)
                .layer(Extension(RealtimeKind::Live)),
        )
        .route(
            "/live/{call_id}",
            get(realtime::socket).layer(Extension(RealtimeKind::Live)),
        )
}

/// Relative to /backend-api/wham; the other mounts above are compatibility aliases.
fn backend_routes() -> Router<ApiState> {
    Router::new()
        // Account, configuration and messages: read-only official endpoints.
        .route(
            "/accounts/check",
            get(backend::forward).layer(Extension(BackendEndpoint::Accounts)),
        )
        .route(
            "/profiles/me",
            get(backend::forward).layer(Extension(BackendEndpoint::Profile)),
        )
        .route(
            "/config/bundle",
            get(backend::forward).layer(Extension(BackendEndpoint::Config)),
        )
        .route(
            "/settings/user",
            get(backend::forward).layer(Extension(BackendEndpoint::Settings)),
        )
        .route(
            "/workspace-messages",
            get(backend::forward).layer(Extension(BackendEndpoint::Messages)),
        )
        // Usage and quota reset.
        .route(
            "/usage",
            get(backend::forward).layer(Extension(BackendEndpoint::Usage)),
        )
        .route(
            "/usage/thread_usage/query",
            post(backend::forward).layer(Extension(BackendEndpoint::ThreadUsage)),
        )
        .route(
            "/usage/thread-estimates/query",
            post(backend::forward).layer(Extension(BackendEndpoint::TurnEstimates)),
        )
        .route(
            "/rate-limit-reset-credits",
            get(backend::forward).layer(Extension(BackendEndpoint::Credits)),
        )
        .route(
            "/rate-limit-reset-credits/consume",
            post(backend::forward).layer(Extension(BackendEndpoint::ConsumeCredit)),
        )
        // Cloud tasks.
        .route(
            "/tasks/list",
            get(backend::forward).layer(Extension(BackendEndpoint::Tasks)),
        )
        .route(
            "/tasks",
            post(backend::forward).layer(Extension(BackendEndpoint::CreateTask)),
        )
        .route(
            "/tasks/{task_id}",
            get(backend::forward).layer(Extension(BackendEndpoint::Task)),
        )
        .route(
            "/tasks/{task_id}/turns/{turn_id}/sibling_turns",
            get(backend::forward).layer(Extension(BackendEndpoint::SiblingTurns)),
        )
}
