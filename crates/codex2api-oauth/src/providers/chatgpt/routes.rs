//! ChatGPT/Codex protocol route registration. No other provider shares these contracts.
use super::handlers::{self, backend, chatgpt, codex, oauth, realtime, system, websocket};
use crate::OAuthState;
use axum::Router;
use axum::extract::{DefaultBodyLimit, Extension};
use axum::routing::{any, get, patch, post};
use codex2api_upstream::{BackendEndpoint, ChatgptEndpoint, Endpoint};

pub(crate) fn usage_endpoint(method: &str, path: &str) -> Option<&'static str> {
    if !matches!(method, "POST" | "GET") {
        return None;
    }
    [
        "/responses",
        "/responses/compact",
        "/responses/{*subpath}",
        "/guardian",
        "/guardian-classifier",
        "/alpha/search",
        "/images/generations",
        "/images/edits",
        "/memories/trace_summarize",
    ]
    .into_iter()
    .find(|suffix| path.ends_with(suffix))
    .and_then(|suffix| {
        if method == "GET" && !matches!(suffix, "/responses" | "/guardian" | "/guardian-classifier")
        {
            return None;
        }
        Some(match suffix {
            "/responses" => "/v1/responses",
            "/responses/compact" => "/v1/responses/compact",
            "/responses/{*subpath}" => "/v1/responses/{*subpath}",
            "/guardian" => "/v1/guardian",
            "/guardian-classifier" => "/v1/guardian-classifier",
            "/alpha/search" => "/v1/alpha/search",
            "/images/generations" => "/v1/images/generations",
            "/images/edits" => "/v1/images/edits",
            _ => "/v1/memories/trace_summarize",
        })
    })
}

pub fn router(state: OAuthState) -> Router {
    use super::handlers::desktop::{self, DesktopEndpoint as D};
    use super::handlers::virtual_data;
    let mut desktop_routes = Router::new();
    for (path, key) in [
        ("referrals/invite/eligibility", "referrals"),
        ("amphora/notifications", "notifications"),
        ("payments/payment_methods", "payment_methods"),
        ("trusted_contact/enabled", "trusted_contact"),
        ("amphora", "family"),
        ("checkout_pricing_config/configs/{country_code}", "pricing"),
        ("gift-credits/senders/eligibility", "gift_credits"),
        ("accounts/{account_id}/settings", "account_settings"),
        ("notifications/settings", "notification_settings"),
        ("pins", "pins"),
        ("aip/first-party/eligibility", "first_party"),
        ("gizmos/snorlax/sidebar", "projects"),
        ("system_hints", "system_hints"),
        ("models", "models"),
        ("settings/is_adult", "age"),
    ] {
        desktop_routes = desktop_routes.route(
            &format!("/backend-api/{path}"),
            get(virtual_data::read).layer(Extension(key)),
        );
    }
    desktop_routes = desktop_routes
        .route(
            "/backend-api/amphora/u18_graduation_unlink_setting_notices",
            get(handlers::family_notices::read),
        )
        .route(
            "/backend-api/amphora/u18_graduation_unlink_setting_notices/dismiss",
            post(handlers::family_notices::dismiss),
        )
        .route(
            "/backend-api/wham/remote/control/server/enroll",
            post(handlers::remote_control::enroll),
        )
        .route(
            "/backend-api/wham/remote/control/server/refresh",
            post(handlers::remote_control::refresh),
        )
        .route(
            "/backend-api/profiles/me/page",
            get(handlers::desktop_profile::read).patch(handlers::desktop_profile::update_page),
        )
        .route(
            "/backend-api/profiles/{username}/page",
            get(handlers::desktop_profile::by_username),
        )
        .route(
            "/backend-api/profiles/me",
            patch(handlers::desktop_profile::update_profile),
        )
        .route(
            "/backend-api/wham/profiles/me",
            patch(handlers::desktop_profile::update_wham),
        )
        .route(
            "/backend-api/referrals/invite/tracking",
            get(virtual_data::referral_tracking),
        )
        .route(
            "/backend-api/connectors/directory/list",
            get(chatgpt::forward).layer(Extension(ChatgptEndpoint::ConnectorDirectory)),
        )
        .route(
            "/backend-api/aura/site_status",
            get(chatgpt::forward).layer(Extension(ChatgptEndpoint::SiteStatus)),
        )
        .route(
            "/backend-api/sentinel/heartbeat",
            post(virtual_data::heartbeat),
        )
        .route(
            "/backend-api/celsius/ws/user",
            get(handlers::virtual_events::connection),
        )
        .route(
            "/backend-api/notifications/settings",
            patch(virtual_data::notifications_patch),
        )
        .route(
            "/backend-api/amphora/notifications/{notification_id}/reacted",
            post(virtual_data::notification_reacted),
        )
        .route(
            "/backend-api/accounts/{account_id}/spend-controls/current-user/monthly-usage",
            get(virtual_data::monthly_usage),
        )
        .route(
            "/backend-api/conversation/init",
            post(virtual_data::conversation_init),
        )
        .route(
            "/backend-api/f/conversation/prepare",
            post(handlers::virtual_conversations::forward)
                .layer(Extension("/backend-api/f/conversation/prepare")),
        )
        .route(
            "/backend-api/f/conversation",
            post(handlers::virtual_conversations::forward)
                .layer(Extension("/backend-api/f/conversation")),
        )
        .route(
            "/backend-api/f/conversation/resume",
            post(handlers::virtual_conversations::forward)
                .layer(Extension("/backend-api/f/conversation/resume")),
        )
        .route(
            "/backend-api/stop_conversation",
            post(handlers::virtual_conversations::forward)
                .layer(Extension("/backend-api/stop_conversation")),
        );
    for (path, endpoint) in [
        ("/backend-api/wham/sites/access", D::Sites),
        ("/backend-api/wham/onboarding/context", D::Onboarding),
        ("/backend-api/automations", D::Automations),
        ("/backend-api/conversations", D::Conversations),
        ("/backend-api/beacons/home", D::Beacons),
        ("/backend-api/accounts/verified_access", D::VerifiedAccess),
        ("/backend-api/wham/browser/settings", D::BrowserSettings),
        (
            "/backend-api/wham/analytics/daily-code-review-metrics",
            D::CodeReviewMetrics,
        ),
    ] {
        desktop_routes = desktop_routes.route(path, get(desktop::read).layer(Extension(endpoint)));
    }
    use super::handlers::desktop_usage::{self, UsageEndpoint as U};
    for (path, endpoint) in [
        (
            "/backend-api/wham/analytics/daily-workspace-usage-counts",
            U::WorkspaceCounts,
        ),
        (
            "/backend-api/wham/usage/daily-token-usage-breakdown",
            U::Tokens,
        ),
        ("/backend-api/wham/usage/credit-usage-events", U::Credits),
        (
            "/backend-api/wham/analytics/daily-plugin-usage-metrics",
            U::Plugins,
        ),
        ("/backend-api/wham/usage/plan_limit_history", U::PlanHistory),
        (
            "/backend-api/wham/analytics/daily-skill-usage-metrics",
            U::Skills,
        ),
    ] {
        desktop_routes =
            desktop_routes.route(path, get(desktop_usage::read).layer(Extension(endpoint)));
    }
    let client = Router::new()
        .nest("/v1", codex_routes())
        .merge(desktop_routes)
        .route(
            "/backend-api/wham/browser/settings",
            patch(desktop::update_browser).layer(DefaultBodyLimit::max(256 * 1024)),
        )
        .route(
            "/backend-api/wham/onboarding/desktop/complete",
            post(desktop::complete_onboarding),
        )
        .route(
            "/backend-api/o11y/v1/traces",
            post(chatgpt::forward).layer(Extension(ChatgptEndpoint::Traces)),
        )
        .route("/backend-api/settings/voices", any(realtime::unavailable))
        .nest(
            "/backend-api/codex",
            codex_routes().route("/{call_id}", any(realtime::unavailable)),
        )
        .nest("/backend-api/wham", backend_routes())
        .route(
            "/backend-api/accounts/check/v4-2023-04-27",
            get(chatgpt::forward).layer(Extension(ChatgptEndpoint::AccountsCheck)),
        )
        .route(
            "/backend-api/accounts/optimized/check",
            get(chatgpt::optimized_account_check),
        )
        .route("/backend-api/me", get(chatgpt::virtual_profile))
        .route("/backend-api/settings/user", get(chatgpt::virtual_settings))
        .route(
            "/backend-api/tpp/models/",
            get(handlers::virtual_data::read).layer(Extension("models")),
        )
        .route(
            "/backend-api/tpp/models",
            get(handlers::virtual_data::read).layer(Extension("models")),
        )
        .route(
            "/backend-api/subscriptions/auto_top_up/settings",
            get(chatgpt::virtual_auto_top_up),
        )
        .route(
            "/backend-api/subscriptions/credits/discount-offer",
            get(chatgpt::virtual_discount_offer),
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
            "/backend-api/plugins/featured",
            get(chatgpt::forward).layer(Extension(ChatgptEndpoint::FeaturedPlugins)),
        )
        .route(
            "/backend-api/ps/plugins/list",
            get(chatgpt::forward).layer(Extension(ChatgptEndpoint::Plugins)),
        )
        .route(
            "/backend-api/ps/plugins/installed",
            get(chatgpt::forward).layer(Extension(ChatgptEndpoint::InstalledPlugins)),
        )
        .route(
            "/backend-api/ps/plugins/suggested/codex",
            get(chatgpt::forward).layer(Extension(ChatgptEndpoint::SuggestedPlugins)),
        )
        .route(
            "/backend-api/ps/plugins/{plugin_id}",
            get(chatgpt::plugin_detail),
        )
        .route(
            "/backend-api/automations/save",
            post(handlers::virtual_operations::unavailable_operation)
                .layer(Extension("automation_save")),
        )
        .route(
            "/backend-api/automations/set_status",
            post(handlers::virtual_operations::unavailable_operation)
                .layer(Extension("automation_set_status")),
        )
        .route(
            "/backend-api/automations/remove",
            post(handlers::virtual_operations::unavailable_operation)
                .layer(Extension("automation_remove")),
        )
        .route(
            "/backend-api/ps/plugins/{plugin_id}/install",
            post(handlers::virtual_operations::unavailable_operation)
                .layer(Extension("plugin_install")),
        )
        .route(
            "/backend-api/ps/plugins/{plugin_id}/uninstall",
            post(handlers::virtual_operations::unavailable_operation)
                .layer(Extension("plugin_uninstall")),
        )
        .route(
            "/backend-api/conversation/{conversation_id}",
            get(handlers::virtual_operations::conversation)
                .patch(handlers::virtual_operations::conversation),
        )
        .route(
            "/backend-api/pins/{item_type}/{item_id}",
            post(handlers::virtual_operations::pin).delete(handlers::virtual_operations::pin),
        )
        .route(
            "/backend-api/amphora/{amphora_id}/members",
            get(handlers::virtual_data::family_members),
        )
        .route("/backend-api/ps/mcp", get(chatgpt::mcp).post(chatgpt::mcp))
        .route("/backend-api/ps/apps/batch", post(chatgpt::apps_batch))
        .route(
            "/backend-api/wham/statsig/bootstrap",
            post(chatgpt::forward).layer(Extension(ChatgptEndpoint::StatsigBootstrap)),
        )
        .route(
            "/backend-api/wham/analytics-events/events",
            post(chatgpt::forward).layer(Extension(ChatgptEndpoint::AnalyticsEvents)),
        )
        .route(
            "/backend-api/codex/analytics-events/events",
            post(chatgpt::forward).layer(Extension(ChatgptEndpoint::AnalyticsEvents)),
        )
        .route(
            "/v1/responses/input_tokens",
            post(codex::forward).layer(Extension(Endpoint::InputTokens)),
        )
        .route(
            "/backend-api/wham/environments",
            get(handlers::virtual_data::cloud_environments),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            oauth::require_oauth,
        ))
        .route(
            "/backend-api/celsius/ws/user/socket",
            get(handlers::virtual_events::socket),
        )
        .route(
            "/backend-api/wham/remote/control/server",
            get(handlers::remote_control::socket),
        )
        .route(
            "/v1/initialize",
            post(handlers::desktop_support::initialize)
                .options(handlers::desktop_support::preflight),
        )
        .route(
            "/mcp-app.html",
            get(handlers::desktop_support::public_resource),
        )
        .route(
            "/assets/{file}",
            get(handlers::desktop_support::public_resource),
        )
        .route(
            "/codex-app-prod/windows-store-update.json",
            get(handlers::desktop_support::public_resource),
        )
        .fallback(handlers::missing::endpoint)
        .method_not_allowed_fallback(handlers::missing::endpoint)
        .route(
            "/backend-api/ps/mcp/.well-known/oauth-protected-resource",
            get(chatgpt::mcp_metadata),
        )
        .route(
            "/oauth/token",
            post(oauth::token).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/api/accounts/deviceauth/usercode",
            post(handlers::oauth_device::user_code).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/api/accounts/deviceauth/token",
            post(handlers::oauth_device::poll).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/codex/desktop-auth",
            get(handlers::oauth_authorize::desktop_authorize),
        )
        .route("/codex/device", get(handlers::oauth_device::page))
        .route(
            "/oauth/authorize",
            get(handlers::oauth_authorize::authorize).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/oauth/revoke",
            post(oauth::revoke).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .layer(DefaultBodyLimit::max(codex2api_upstream::MAX_REQUEST_BYTES));
    Router::new()
        .route("/healthz", get(system::healthz))
        .route("/version", get(system::version))
        .nest(oauth::PREFIX, client)
        .with_state(state)
}

/// Relative to /v1 (or /backend-api/codex). GET inference routes upgrade to WebSocket.
fn codex_routes() -> Router<OAuthState> {
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
        // Voice calls are intentionally unavailable, including old client aliases.
        .route("/realtime", any(realtime::unavailable))
        .route("/realtime/calls/{call_id}", any(realtime::unavailable))
        .route("/realtime/calls", any(realtime::unavailable))
        .route("/live", any(realtime::unavailable))
        .route("/live/{call_id}", any(realtime::unavailable))
}

/// Installed Desktop and pinned CLI routes relative to /backend-api/wham.
fn backend_routes() -> Router<OAuthState> {
    Router::new()
        .route("/realtime/calls/{call_id}", any(realtime::unavailable))
        .route("/realtime/calls", any(realtime::unavailable))
        .route(
            "/settings/configs/user-preferences",
            get(handlers::virtual_operations::cloud_preferences_schema),
        )
        // SupplierAccount, configuration and messages: read-only official endpoints.
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
            get(backend::forward)
                .patch(handlers::virtual_operations::cloud_preferences_patch)
                .layer(Extension(BackendEndpoint::Settings)),
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
            "/tasks/{task_id}/turns",
            get(backend::forward).layer(Extension(BackendEndpoint::TaskTurns)),
        )
        .route(
            "/tasks/{task_id}/turns/{turn_id}",
            get(backend::forward).layer(Extension(BackendEndpoint::TaskTurn)),
        )
        .route(
            "/tasks/{task_id}/turns/{turn_id}/logs",
            get(backend::forward).layer(Extension(BackendEndpoint::TaskLogs)),
        )
        .route(
            "/tasks/{task_id}/cancel",
            post(backend::forward).layer(Extension(BackendEndpoint::CancelTask)),
        )
        .route(
            "/tasks/{task_id}/archive",
            post(backend::forward).layer(Extension(BackendEndpoint::ArchiveTask)),
        )
        .route(
            "/tasks/{task_id}/turns/{turn_id}/sibling_turns",
            get(backend::forward).layer(Extension(BackendEndpoint::SiblingTurns)),
        )
}
