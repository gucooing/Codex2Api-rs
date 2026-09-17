use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn official_pages_require_login_and_mutations_require_the_session_csrf_token() {
    let temp = tempfile::tempdir().unwrap();
    let storage = codex2api_storage::Storage::open(temp.path().join("test.sqlite"))
        .await
        .unwrap();
    storage.ensure_default_admin().await.unwrap();
    let state = codex2api_admin::AdminState::new(storage.clone()).unwrap();
    let account = state.accounts.create_pending().await.unwrap();
    let path = format!("/admin/accounts/{}/official", account.account.id);
    let app = codex2api_admin::router(state);
    let response = app
        .clone()
        .oneshot(Request::get(&path).body(Body::empty()).unwrap())
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
            Request::get(format!("/admin/accounts/{}", account.account.id))
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = std::str::from_utf8(&body).unwrap();
    assert!(body.contains("每周限额"));
    assert!(body.contains("5 小时限额"));
    assert!(body.contains("查看用量明细"));
    assert!(!body.contains("<h2>代理 API Key</h2>"));
    for title in ["账户信息", "指纹", "用量明细（官方数据）", "账户详细信息"]
    {
        assert!(body.contains(title));
    }
    assert!(body.contains("账户尚未完成授权"));
    assert!(!body.contains("?tab=keys"));
    assert!(!body.contains("aria-valuenow"));
    for tab in ["usage", "account", "credits"] {
        let response = app
            .clone()
            .oneshot(
                Request::get(format!(
                    "{path}?tab={tab}&thread_id=old-thread&turn_ids=old-turn"
                ))
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let target = response.headers()["location"].to_str().unwrap();
        assert!(target.starts_with(&format!("/admin/accounts/{}?tab=", account.account.id)));
        let response = app
            .clone()
            .oneshot(
                Request::get(target)
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{tab}");
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&body).unwrap();
        assert!(body.contains("账户尚未完成授权")); // No real upstream calls for a pending fixture account.
        assert!(!body.contains("工作区消息"));
        assert!(!body.contains("云任务"));
        assert!(!body.contains("create_task"));
        assert!(!body.contains("配置与设置"));
        assert!(!body.contains("查询线程与回合"));
        assert!(!body.contains("线程用量明细"));
        assert!(!body.contains("回合费用估算"));
        if tab == "usage" {
            assert!(!body.contains("<dt>内部 ID</dt>"));
            assert!(!body.contains("<h2>代理 API Key</h2>"));
        }
        if tab == "account" {
            assert!(body.contains("<dt>内部 ID</dt>"));
            assert!(body.contains("Installation ID"));
        }
    }
    let issued = storage
        .create_proxy_api_key(&account.account.id, Some("test-key"))
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::get("/admin/keys")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = std::str::from_utf8(&body).unwrap();
    assert!(body.contains("<h2>API Key</h2>"));
    assert!(body.contains("test-key"));
    assert!(body.contains(&issued.record.key_prefix));
    assert!(!body.contains(&issued.token));
    assert!(!body.contains(&issued.record.key_hash));
    assert!(!body.contains("套餐限额"));
    assert!(!body.contains("<dt>内部 ID</dt>"));
    assert!(!body.contains("账户尚未完成授权"));
    let csrf = body
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::post(format!("/admin/keys/{}/delete", issued.record.id))
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("csrf={csrf}")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        response.headers()["location"]
            .to_str()
            .unwrap()
            .contains("/admin/keys?ok=")
    );
    for tab in ["tasks", "messages", "config"] {
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("{path}?tab={tab}"))
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{tab}");
    }
    let response = app
        .oneshot(
            Request::post(&path)
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "csrf=invalid&action=consume&redeem_request_id=anything",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    storage.close().await;
}
