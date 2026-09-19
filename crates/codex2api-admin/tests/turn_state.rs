use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use codex2api_storage::{
    AccountStatus, NewAccount, Storage, TurnStateObservation, TurnStateSettings,
};
use tower::ServiceExt;

#[tokio::test]
async fn turn_state_page_is_account_scoped_authenticated_and_never_displays_token() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("admin.sqlite"))
        .await
        .unwrap();
    for id in ["a", "b"] {
        let mut new = NewAccount::pending_identity(
            id,
            "codex_cli_rs",
            "test",
            "test",
            "test",
            "test",
            "",
            "{}",
        );
        new.id = Some(id.into());
        new.chatgpt_account_id = Some(id.into());
        new.status = AccountStatus::Active;
        storage.create_account(new).await.unwrap();
    }
    let app = codex2api_admin::router(codex2api_admin::AdminState::new(storage.clone()).unwrap());
    for (method, path) in [
        ("GET", "/admin/accounts/a?tab=turn-state"),
        ("POST", "/admin/accounts/a/turn-state"),
        ("POST", "/admin/accounts/a/turn-state/clear"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()["location"], "/admin/login");
    }
    let session = storage
        .login_admin("admin", "admin", std::time::Duration::from_secs(600))
        .await
        .unwrap()
        .unwrap();
    let cookie = format!("{}={}", codex2api_admin::SESSION_COOKIE, session.id);
    let get = || {
        Request::get("/admin/accounts/a?tab=turn-state")
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap()
    };
    let response = app.clone().oneshot(get()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let html = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(html.contains("状态复用（实验）"));
    assert!(html.contains("WebSocket 不参与"));
    let csrf = html
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let post = |path: &str, csrf: &str, ttl: &str| {
        Request::post(path)
            .header("cookie", &cookie)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(
                url::form_urlencoded::Serializer::new(String::new())
                    .extend_pairs([
                        ("csrf", csrf),
                        ("enabled", "on"),
                        ("models", "gpt-6-astra"),
                        ("ttl", ttl),
                    ])
                    .finish(),
            ))
            .unwrap()
    };
    for path in [
        "/admin/accounts/a/turn-state",
        "/admin/accounts/a/turn-state/clear",
    ] {
        assert_eq!(
            app.clone()
                .oneshot(post(path, "wrong", "3600"))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        app.clone()
            .oneshot(post("/admin/accounts/a/turn-state", csrf, "119"))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        storage.turn_state_settings("a").await.unwrap().0,
        TurnStateSettings::default()
    );
    assert_eq!(
        app.clone()
            .oneshot(post("/admin/accounts/a/turn-state", csrf, "3600"))
            .await
            .unwrap()
            .status(),
        StatusCode::SEE_OTHER
    );
    assert!(storage.turn_state_settings("a").await.unwrap().0.enabled);
    assert!(!storage.turn_state_settings("b").await.unwrap().0.enabled);
    let (_, revision) = storage.turn_state_settings("a").await.unwrap();
    storage
        .ensure_turn_state_entry("a", "gpt-6-astra", "a", &revision)
        .await
        .unwrap();
    let entry = storage
        .turn_state_entry("a", "gpt-6-astra", "a", &revision)
        .await
        .unwrap()
        .unwrap();
    let time = chrono::Utc::now().timestamp();
    let settings = storage.turn_state_settings("a").await.unwrap().0;
    storage
        .record_turn_state_observation(
            &entry,
            &settings,
            TurnStateObservation {
                from_client: false,
                token: Some("NEVER_DISPLAY_THIS_SECRET"),
                issued_at: time,
                now: time,
                status: 200,
                result: "accepted",
                length: 292,
                blocks: 10,
                injected: false,
            },
        )
        .await
        .unwrap();
    storage
        .record_turn_state_observation(
            &entry,
            &settings,
            TurnStateObservation {
                from_client: true,
                token: None,
                issued_at: 0,
                now: time,
                status: 0,
                result: "missing_header",
                length: 0,
                blocks: 0,
                injected: true,
            },
        )
        .await
        .unwrap();
    let response = app.clone().oneshot(get()).await.unwrap();
    let html = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(html.contains("缓存可用"));
    assert!(html.contains("累计缓存使用次数</dt><dd>1"));
    assert!(!html.contains("NEVER_DISPLAY_THIS_SECRET"));
    assert!(html.contains("正常上游响应"));
    assert!(html.contains("合格的客户端 state 原样透传"));
    assert!(html.contains("才使用有效缓存替换"));
    assert!(html.contains("不延长旧 state 的有效期"));
    assert!(!html.contains("探针"));
    assert!(!html.contains("name=\"renew\""));
    assert!(!html.contains("name=\"cooldown\""));
    storage
        .record_turn_state_observation(
            &entry,
            &settings,
            TurnStateObservation {
                from_client: false,
                token: None,
                issued_at: 0,
                now: time,
                status: 200,
                result: "missing_header",
                length: 0,
                blocks: 0,
                injected: false,
            },
        )
        .await
        .unwrap();
    let response = app.clone().oneshot(get()).await.unwrap();
    let html = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(html.contains("未携带 state 头；HTTP 200"));
    assert!(html.contains("缓存可用"));
    assert_eq!(
        app.clone()
            .oneshot(post("/admin/accounts/a/turn-state/clear", csrf, "3600"))
            .await
            .unwrap()
            .status(),
        StatusCode::SEE_OTHER
    );
    assert!(storage.turn_state_entries("a").await.unwrap().is_empty());
}
