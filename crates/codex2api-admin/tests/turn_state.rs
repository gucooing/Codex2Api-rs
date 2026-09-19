use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use codex2api_storage::{
    AccountStatus, NewAccount, Storage, TurnStateProbeResult, TurnStateSettings,
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
    assert!(html.contains("WebSocket 不参与实验"));
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
                        ("renew", "600"),
                        ("cooldown", "300"),
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
            .oneshot(post("/admin/accounts/a/turn-state", csrf, "120"))
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
    let lease = storage
        .claim_turn_state_probe(&entry, time, 300)
        .await
        .unwrap()
        .unwrap();
    storage
        .finish_turn_state_probe(
            &entry,
            &lease,
            TurnStateProbeResult {
                token: Some("NEVER_DISPLAY_THIS_SECRET"),
                issued_at: time,
                expires_at: time + 3570,
                refresh_at: time + 3000,
                now: time,
                status: 200,
                result: "accepted",
                next_probe_at: time + 300,
            },
        )
        .await
        .unwrap();
    storage.record_turn_state_use(&entry, true).await.unwrap();
    let response = app.clone().oneshot(get()).await.unwrap();
    let html = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(html.contains("缓存可用"));
    assert!(html.contains("累计注入次数</dt><dd>1"));
    assert!(!html.contains("NEVER_DISPLAY_THIS_SECRET"));
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
