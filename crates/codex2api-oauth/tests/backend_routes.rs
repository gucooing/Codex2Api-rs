use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use tower::ServiceExt;

#[tokio::test]
async fn all_backend_routes_require_a_virtual_account_token() {
    let temp = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(temp.path().join("test-pg"))
        .await
        .unwrap();
    let accounts = codex2api_suppliers::SupplierAccountStore::open(storage.clone());
    let auth = codex2api_suppliers::AuthService::new(accounts.clone()).unwrap();
    let state = codex2api_oauth::OAuthState::new(
        storage.clone(),
        accounts,
        codex2api_upstream::UpstreamPool::new(auth),
    );
    let app = codex2api_oauth::router(state);
    // Pin the public contract independently of upstream endpoint enums.
    for (method, suffix) in [
        (Method::GET, "/usage"),
        (Method::GET, "/accounts/check"),
        (Method::GET, "/profiles/me"),
        (Method::GET, "/config/bundle"),
        (Method::GET, "/settings/user"),
        (Method::GET, "/workspace-messages"),
        (Method::GET, "/rate-limit-reset-credits"),
        (Method::POST, "/rate-limit-reset-credits/consume"),
        (Method::GET, "/tasks/list"),
        (Method::POST, "/tasks"),
        (Method::GET, "/tasks/task-1"),
        (Method::GET, "/tasks/task-1/turns/turn-1/sibling_turns"),
        (Method::POST, "/usage/thread_usage/query"),
        (Method::POST, "/usage/thread-estimates/query"),
        (Method::POST, "/realtime/calls"),
    ] {
        let path = format!("/v1/chatgpt/backend-api/wham{suffix}");
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(&path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
    for prefix in ["/v1/chatgpt/v1", "/v1/chatgpt/backend-api/codex"] {
        for suffix in [
            "/responses",
            "/guardian",
            "/guardian-classifier",
            "/alpha/search",
            "/images/generations",
            "/images/edits",
            "/memories/trace_summarize",
            "/realtime/calls",
            "/live",
        ] {
            let path = format!("{prefix}{suffix}");
            let response = app
                .clone()
                .oneshot(Request::post(&path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
        }
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("{prefix}/models"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        for suffix in [
            "/responses",
            "/guardian",
            "/guardian-classifier",
            "/realtime",
            "/live",
            "/live/call-1",
        ] {
            let path = format!("{prefix}{suffix}");
            let response = app
                .clone()
                .oneshot(Request::get(&path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            // Authentication precedes WebSocket upgrade validation.
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
        }
    }
    for path in ["/healthz", "/version"] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
    }
    storage.close().await;
}
