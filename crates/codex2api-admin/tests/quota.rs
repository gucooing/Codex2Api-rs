use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use codex2api_storage::{QuotaSnapshot, Storage};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn quota_routes_use_persisted_data_and_return_refresh_failure_reasons() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("quota.sqlite"))
        .await
        .unwrap();
    let state = codex2api_admin::AdminState::new(storage.clone()).unwrap();
    let id = state.accounts.create_pending().await.unwrap().account.id;
    let snapshot = QuotaSnapshot {
        value: json!({"rate_limit":{"primary_window":{"used_percent":58,"limit_window_seconds":604800,"reset_after_seconds":3600}}}),
        observed_at: chrono::Utc::now() - chrono::TimeDelta::minutes(5),
    };
    storage.store_account_quota(&id, &snapshot).await.unwrap();
    let path = format!("/admin/accounts/{id}/quota");
    let app = codex2api_admin::router(state);
    let response = app
        .clone()
        .oneshot(
            Request::get(format!("{path}?refresh=true"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()["location"], "/admin/login");
    let session = storage
        .login_admin("admin", "admin", std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let cookie = format!("{}={}", codex2api_admin::SESSION_COOKIE, session.id);
    for url in [&path, &format!("/admin/accounts/{id}")] {
        let response = app
            .clone()
            .oneshot(
                Request::get(url)
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let html = std::str::from_utf8(&body).unwrap();
        assert!(html.contains("42%"));
        assert!(html.contains(&format!(
            "data-reset-countdown=\"{}\"",
            snapshot.observed_at.timestamp() + 3600
        )));
        assert!(html.contains("55 分钟后重置"));
    }
    // Pending accounts fail locally, so forcing a refresh needs no official request.
    let response = app
        .oneshot(
            Request::get(format!("{path}?refresh=true"))
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let error: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(error["error"], "账户尚未完成授权");
    assert_eq!(
        storage.get_account_quota(&id).await.unwrap(),
        Some(snapshot)
    );
    storage.close().await;
}
