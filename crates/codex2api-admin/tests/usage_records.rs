use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use codex2api_storage::{Storage, UsageRecord};
use tower::ServiceExt;

#[tokio::test]
async fn usage_list_requires_login_filters_history_and_renders_reasoning_and_units() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("usage.sqlite"))
        .await
        .unwrap();
    for (id, account, key, model, effort, status) in [
        ("one", "Alice", "Key One", "gpt-test", "xhigh", "completed"),
        ("two", "Bob", "Key Two", "other", "low", "failed"),
        ("three", "Carol", "Key Three", "other", "low", "in_progress"),
        ("four", "Dave", "Key Four", "other", "low", "incomplete"),
        ("five", "Eve", "Key Five", "other", "low", "interrupted"),
    ] {
        let mut record = UsageRecord {
            id: id.into(),
            account_id: id.into(),
            account_name: account.into(),
            api_key_id: id.into(),
            api_key_name: key.into(),
            endpoint: "/v1/responses".into(),
            transport: "http".into(),
            model: Some(model.into()),
            reasoning_effort: Some(effort.into()),
            service_tier: (id == "one").then(|| "priority".into()),
            requested_at_ms: 1_789_516_800_000,
            status: "in_progress".into(),
            ..Default::default()
        };
        storage.insert_usage(&record).await.unwrap();
        record.status = status.into();
        record.actual_model = Some("actual-model".into());
        record.input_tokens = Some(12500);
        record.output_tokens = Some(1000);
        record.first_byte_ms = Some(1234);
        record.total_ms = Some(7654);
        storage.finish_usage(&record).await.unwrap();
    }
    let state = codex2api_admin::AdminState::new(storage.clone()).unwrap();
    let account = state.accounts.create_pending().await.unwrap();
    storage
        .update_account(
            &account.account.id,
            codex2api_storage::AccountUpdate {
                display_name: Some("Alice".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let app = codex2api_admin::router(state);
    let response = app
        .clone()
        .oneshot(Request::get("/admin/usage").body(Body::empty()).unwrap())
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
    let response = app
        .clone()
        .oneshot(
            Request::get("/admin/usage?account=lic&api_key=one&model=GPT&status=completed&page=1")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    assert!(!html.contains("共 1 条记录"));
    assert!(!html.contains("<h2>用量管理</h2>"));
    assert!(!html.contains("记录客户端消耗类调用"));
    assert!(html.contains("xhigh"));
    assert!(html.contains("xhigh · fast"));
    assert!(html.contains("<th>模型 / 接口</th><th>推理强度</th>"));
    assert!(html.contains("<th>来源</th>"));
    assert!(html.contains("<label>来源<select"));
    assert!(!html.contains("<th>API Key</th>"));
    assert!(!html.contains("<th>真实模型</th>"));
    assert!(
        html.contains(r#"<strong class="record-model-changed">gpt-test-&gt;actual-model</strong>"#)
    );
    assert!(html.contains("12.5K"));
    assert!(html.contains("1.23s"));
    assert!(html.contains("7.65s"));
    assert!(html.contains("<label>账户<input"));
    assert!(html.contains(
        r#"<datalist id="usage-account-options"><option value="Alice"></option></datalist>"#
    ));
    assert!(html.contains(r#"<option value="completed" selected>完成</option>"#));
    assert!(!html.contains(r#"<option value="one""#));
    let rows = html
        .split("<tbody>")
        .nth(1)
        .unwrap()
        .split("</tbody>")
        .next()
        .unwrap();
    assert!(rows.contains("Alice"));
    assert!(rows.contains("Key One"));
    assert!(!rows.contains("Bob"));
    for (status, expected_id) in [
        ("completed", "one"),
        ("failed", "two"),
        ("in_progress", "three"),
        ("incomplete", "four"),
        ("interrupted", "five"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("/admin/usage?status={status}"))
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let html = std::str::from_utf8(&body).unwrap();
        let rows = html
            .split("<tbody>")
            .nth(1)
            .unwrap()
            .split("</tbody>")
            .next()
            .unwrap();
        assert_eq!(rows.matches("<tr>").count(), 1);
        assert!(rows.contains(&format!(r#"class="record-account" title="{expected_id}""#)));
    }
    let response = app
        .clone()
        .oneshot(
            Request::get("/admin/usage?status=invalid")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response = app
        .oneshot(
            Request::get("/admin/usage?from=2026-09-17T00:00&until=2026-09-16T00:00")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    storage.close().await;
}
