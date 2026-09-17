use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use codex2api_storage::Storage;
use tower::ServiceExt;

#[tokio::test]
async fn proxy_configuration_requires_login_and_csrf_and_hides_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("proxies.sqlite"))
        .await
        .unwrap();
    let app = codex2api_admin::router(codex2api_admin::AdminState::new(storage.clone()).unwrap());
    assert_eq!(
        app.clone()
            .oneshot(Request::get("/admin/proxies").body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status(),
        StatusCode::SEE_OTHER
    );
    let session = storage
        .login_admin("admin", "admin", std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let cookie = format!("{}={}", codex2api_admin::SESSION_COOKIE, session.id);
    let page = app
        .clone()
        .oneshot(
            Request::get("/admin/proxies")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(page.into_body(), usize::MAX).await.unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    assert!(html.contains("data-add-proxy"));
    assert!(html.contains("<dialog"));
    for field in ["protocol", "host", "port", "username", "password"] {
        assert!(html.contains(&format!("name=\"{field}\"")));
    }
    assert!(!html.contains("name=\"url\""));
    let csrf = html
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    for (token, status) in [
        ("wrong", StatusCode::FORBIDDEN),
        (csrf, StatusCode::SEE_OTHER),
    ] {
        let form = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs([
                ("csrf", token),
                ("name", "<proxy>"),
                ("protocol", "https"),
                ("host", "localhost"),
                ("port", "8080"),
                ("username", "user"),
                ("password", "private-password"),
            ])
            .finish();
        let response = app
            .clone()
            .oneshot(
                Request::post("/admin/proxies")
                    .header("cookie", &cookie)
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(form))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        if token == "wrong" {
            assert!(storage.list_outbound_proxies().await.unwrap().is_empty());
        }
    }
    assert_eq!(storage.list_outbound_proxies().await.unwrap().len(), 1);
    let proxy = storage.list_outbound_proxies().await.unwrap().remove(0);
    assert_eq!(proxy.url, "https://user:private-password@localhost:8080/");
    let response = app
        .clone()
        .oneshot(
            Request::get("/admin/proxies")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    assert!(html.contains("&lt;proxy&gt;"));
    assert!(html.contains("localhost:8080"));
    assert!(html.contains("测试连接"));
    assert!(html.contains("质量检测"));
    assert!(html.contains("data-proxy-edit="));
    assert!(!html.contains("private-password"));
    for (path, status) in [
        (
            format!("/admin/proxies/{}/test", proxy.id),
            StatusCode::FORBIDDEN,
        ),
        (
            format!("/admin/proxies/{}/quality", proxy.id),
            StatusCode::FORBIDDEN,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::post(path)
                    .header("cookie", &cookie)
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("csrf=bad"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
    }
    let response = app
        .clone()
        .oneshot(
            Request::get("/admin/proxies?protocol=socks5")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        !std::str::from_utf8(&body)
            .unwrap()
            .contains("&lt;proxy&gt;")
    );
    let response = app
        .oneshot(
            Request::get("/admin/proxies?keyword=LOCALHOST&protocol=https")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        std::str::from_utf8(&body)
            .unwrap()
            .contains("&lt;proxy&gt;")
    );
    storage.close().await;
}

#[tokio::test]
async fn edit_proxy_prefills_saves_and_refreshes_bound_account_transport() {
    use codex2api_storage::{ProxyConnectionCheck, ProxyQualityCheck, StorageError};
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("edit.sqlite")).await.unwrap();
    let state = codex2api_admin::AdminState::new(storage.clone()).unwrap();
    let account = state.accounts.create_pending().await.unwrap().account;
    let proxy = storage
        .create_outbound_proxy("before", "http://user%25:pass%40@127.0.0.1:1080")
        .await
        .unwrap();
    storage
        .set_account_proxy(&account.id, Some(&proxy.id))
        .await
        .unwrap();
    let original_client = state.auth.account_http(&account.id).await.unwrap();
    let connection = ProxyConnectionCheck {
        ok: true,
        latency_ms: 123,
        country: Some("US".into()),
        timezone: Some("America/Los_Angeles".into()),
        ..Default::default()
    };
    let quality = ProxyQualityCheck {
        ok: true,
        latency_ms: 234,
        http_status: Some(401),
        ..Default::default()
    };
    storage
        .save_proxy_connection_check(&proxy, &connection)
        .await
        .unwrap();
    storage
        .save_proxy_quality_check(&proxy, &quality)
        .await
        .unwrap();
    let app = codex2api_admin::router(state.clone());
    let path = format!("/admin/proxies/{}/edit", proxy.id);
    let response = app
        .clone()
        .oneshot(Request::get(&path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let session = storage
        .login_admin("admin", "admin", std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let cookie = format!("{}={}", codex2api_admin::SESSION_COOKIE, session.id);
    let get = |url: &str| {
        Request::get(url)
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap()
    };
    let page = app.clone().oneshot(get("/admin/proxies")).await.unwrap();
    let body = to_bytes(page.into_body(), usize::MAX).await.unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    let csrf = html
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    assert!(!html.contains("延迟为完整请求耗时"));
    let response = app.clone().oneshot(get(&path)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let values: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(values["name"], "before");
    assert_eq!(values["protocol"], "http");
    assert_eq!(values["host"], "127.0.0.1");
    assert_eq!(values["port"], 1080);
    assert_eq!(values["username"], "user%");
    assert_eq!(values["password"], "pass@");
    assert_eq!(values["account_count"], 1);
    for (token, protocol, status) in [
        ("bad", "http", StatusCode::FORBIDDEN),
        (csrf, "http", StatusCode::SEE_OTHER),
        (csrf, "socks5h", StatusCode::SEE_OTHER),
    ] {
        let form = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs([
                ("csrf", token),
                ("name", "after"),
                ("protocol", protocol),
                ("host", "127.0.0.1"),
                ("port", "1080"),
                ("username", "user%"),
                ("password", "pass@"),
            ])
            .finish();
        let response = app
            .clone()
            .oneshot(
                Request::post(&path)
                    .header("cookie", &cookie)
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(form))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        let updated = storage.require_outbound_proxy(&proxy.id).await.unwrap();
        if token == "bad" {
            assert_eq!(updated.name, "before");
        } else {
            assert_eq!(updated.name, "after");
        }
        if protocol == "http" {
            assert_eq!(updated.connection_latency_ms, Some(123));
            assert_eq!(updated.timezone.as_deref(), Some("America/Los_Angeles"));
            assert_eq!(updated.quality_latency_ms, Some(234));
        } else {
            assert!(updated.connection_ok.is_none() && updated.quality_ok.is_none());
            assert!(updated.country.is_none());
            assert!(updated.timezone.is_none());
            assert_eq!(updated.created_at, proxy.created_at);
        }
    }
    assert!(matches!(
        storage
            .save_proxy_connection_check(&proxy, &connection)
            .await,
        Err(StorageError::ProxyChanged)
    ));
    assert!(matches!(
        storage.save_proxy_quality_check(&proxy, &quality).await,
        Err(StorageError::ProxyChanged)
    ));
    let current_client = state.auth.account_http(&account.id).await.unwrap();
    assert!(!Arc::ptr_eq(&current_client, &original_client));
    assert_eq!(
        current_client.proxy_url(),
        Some("socks5h://user%25:pass%40@127.0.0.1:1080")
    );
    let bound = storage.require_account(&account.id).await.unwrap();
    assert_eq!(bound.proxy_id.as_deref(), Some(proxy.id.as_str()));
    assert_eq!(bound.installation_id, account.installation_id);
    assert_eq!(storage.list_outbound_proxies().await.unwrap().len(), 1);
    storage.close().await;
}
