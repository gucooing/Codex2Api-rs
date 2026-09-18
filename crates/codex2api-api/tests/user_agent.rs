use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use codex2api_storage::{GatewaySettings, Storage, UaMode, UsageFilter};
use tower::ServiceExt;

#[tokio::test]
async fn policy_covers_billable_http_websocket_and_aliases_before_account_use() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("ua.sqlite")).await.unwrap();
    let accounts = codex2api_accounts::AccountStore::open(storage.clone());
    let account = accounts.create_pending().await.unwrap().account;
    let key = storage
        .create_proxy_api_key(&account.id, None)
        .await
        .unwrap();
    let auth = codex2api_auth::AuthService::new(accounts.clone()).unwrap();
    let app = codex2api_api::router(codex2api_api::ApiState::new(
        storage.clone(),
        accounts,
        codex2api_upstream::UpstreamPool::new(auth),
    ));
    storage
        .save_gateway_settings(&GatewaySettings::from_lines(
            UaMode::Blacklist,
            "blocked*client\notherbot",
        ))
        .await
        .unwrap();
    for prefix in ["/v1", "/backend-api/codex"] {
        for (method, suffix) in [
            ("POST", "/responses"),
            ("GET", "/responses"),
            ("POST", "/guardian"),
            ("GET", "/guardian"),
            ("POST", "/guardian-classifier"),
            ("GET", "/guardian-classifier"),
            ("POST", "/alpha/search"),
            ("POST", "/images/generations"),
            ("POST", "/images/edits"),
            ("POST", "/memories/trace_summarize"),
            ("GET", "/realtime"),
            ("POST", "/realtime/calls"),
            ("POST", "/live"),
            ("GET", "/live"),
            ("GET", "/live/call-1"),
        ] {
            let path = format!("{prefix}{suffix}");
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(&path)
                        .header("authorization", format!("Bearer {}", key.token))
                        .header("user-agent", "BLOCKED desktop CLIENT/1.0")
                        .body(Body::from("not-json"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::TOO_MANY_REQUESTS,
                "{method} {path}"
            );
            let body: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                    .unwrap();
            assert_eq!(body["error"]["code"], "user_agent_blocked");
        }
    }
    assert!(
        storage
            .get_proxy_api_key(&key.record.id)
            .await
            .unwrap()
            .unwrap()
            .last_used_at
            .is_none()
    );
    assert!(
        storage
            .get_account(&account.id)
            .await
            .unwrap()
            .unwrap()
            .last_used_at
            .is_none()
    );
    assert_eq!(
        storage
            .query_usage(&UsageFilter::default())
            .await
            .unwrap()
            .total,
        0
    );

    // Saving another mode takes effect in the already-created router.
    for (mode, rules, ua, status) in [
        (
            UaMode::Blacklist,
            "blocked",
            Some("allowed"),
            StatusCode::UNAUTHORIZED,
        ),
        (UaMode::Blacklist, "blocked", None, StatusCode::UNAUTHORIZED),
        (
            UaMode::Whitelist,
            "codex*cli\ntrusted",
            Some("CODEX desktop CLI/1"),
            StatusCode::UNAUTHORIZED,
        ),
        (
            UaMode::Whitelist,
            "codex*cli\ntrusted",
            Some("other"),
            StatusCode::TOO_MANY_REQUESTS,
        ),
        (
            UaMode::Whitelist,
            "codex*cli",
            None,
            StatusCode::TOO_MANY_REQUESTS,
        ),
        (
            UaMode::Whitelist,
            "",
            Some("codex_cli"),
            StatusCode::TOO_MANY_REQUESTS,
        ),
        (
            UaMode::Blacklist,
            "",
            Some("blocked"),
            StatusCode::UNAUTHORIZED,
        ),
    ] {
        storage
            .save_gateway_settings(&GatewaySettings::from_lines(mode, rules))
            .await
            .unwrap();
        let mut request = Request::post("/v1/responses");
        if let Some(ua) = ua {
            request = request.header("user-agent", ua);
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{mode:?} {rules:?} {ua:?}");
    }

    // Read-only metadata and admin/account APIs are outside this policy.
    storage
        .save_gateway_settings(&GatewaySettings::from_lines(UaMode::Blacklist, "*"))
        .await
        .unwrap();
    for (path, status) in [
        ("/v1/models", StatusCode::UNAUTHORIZED),
        ("/backend-api/codex/models", StatusCode::UNAUTHORIZED),
        ("/v1/usage", StatusCode::UNAUTHORIZED),
        ("/backend-api/wham/accounts/check", StatusCode::UNAUTHORIZED),
        ("/healthz", StatusCode::OK),
        ("/version", StatusCode::OK),
    ] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{path}");
    }
    storage.close().await;
}
