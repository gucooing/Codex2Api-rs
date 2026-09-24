mod common;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use common::*;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn supplier_detail_reads_cached_official_username() {
    let f = Fixture::new().await;
    let account = f.state.accounts.create_pending().await.unwrap().account;
    f.storage
        .store_supplier_info(
            &account.id,
            codex2api_storage::SupplierInfoSection::Usage,
            &codex2api_storage::QuotaSnapshot {
                value: json!({"profile":{"username":"cached-user"}}),
                observed_at: chrono::Utc::now(),
            },
        )
        .await
        .unwrap();
    let value = f.get(&format!("/admin/api/suppliers/{}", account.id)).await;
    assert_eq!(value["username"], "cached-user");
}
#[tokio::test]
async fn cookie_sessions_require_csrf_for_every_administration_mutation() {
    let f = Fixture::new().await;
    for (method, path) in [
        ("POST", "/admin/api/logout"),
        ("POST", "/admin/api/suppliers/fake/status"),
        ("POST", "/admin/api/suppliers/fake/recover"),
        ("DELETE", "/admin/api/suppliers/fake"),
        ("PUT", "/admin/api/suppliers/fake/fingerprint"),
        ("POST", "/admin/api/suppliers/oauth/start"),
        ("POST", "/admin/api/suppliers/oauth/callback"),
        ("POST", "/admin/api/suppliers/oauth/poll"),
        ("POST", "/admin/api/suppliers/oauth/cancel"),
        ("POST", "/admin/api/suppliers/fake/relogin"),
        ("POST", "/admin/api/suppliers/fake/credits/consume"),
        ("POST", "/admin/api/consumers"),
        ("PUT", "/admin/api/consumers/fake"),
        ("DELETE", "/admin/api/consumers/fake"),
        ("PUT", "/admin/api/consumers/fake/config/profile"),
        ("PUT", "/admin/api/consumers/fake/routing"),
        ("POST", "/admin/api/consumers/fake/devices/fake/revoke"),
        ("POST", "/admin/api/plans"),
        ("PUT", "/admin/api/plans/fake"),
        ("DELETE", "/admin/api/plans/fake"),
        ("POST", "/admin/api/models"),
        ("POST", "/admin/api/models/status"),
        ("POST", "/admin/api/models/delete"),
        ("POST", "/admin/api/proxies"),
        ("PUT", "/admin/api/proxies/fake"),
        ("DELETE", "/admin/api/proxies/fake"),
        ("POST", "/admin/api/proxies/fake/check/test"),
        ("PUT", "/admin/api/settings/gateway"),
        ("PUT", "/admin/api/settings/security"),
        ("PUT", "/admin/api/settings/desktop"),
    ] {
        assert_eq!(
            f.with_auth(method, path, json!({}), None, None)
                .await
                .status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path}"
        );
        for token in [None, Some("wrong")] {
            assert_eq!(
                f.with_auth(method, path, json!({}), Some(&f.cookie), token)
                    .await
                    .status(),
                StatusCode::FORBIDDEN,
                "{method} {path}"
            );
        }
    }
    for path in [
        "/admin/api/suppliers",
        "/admin/api/consumers",
        "/admin/api/plans",
        "/admin/api/models",
        "/admin/api/proxies",
        "/admin/api/usage",
        "/admin/api/overview",
        "/admin/api/resources",
    ] {
        let r = f
            .app
            .clone()
            .oneshot(
                Request::get(path)
                    .header("authorization", "Bearer consumer-oauth-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED, "{path}");
        assert_eq!(r.headers()["cache-control"], "no-store");
    }
    let logout = f.request("POST", "/admin/api/logout", json!({})).await;
    assert_eq!(logout.status(), StatusCode::OK);
    assert!(
        logout.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    let response = f.request("GET", "/admin/api/session", json!(null)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(body(response).await["error"]["code"], "unauthorized");
}

#[tokio::test]
async fn session_storage_failure_returns_503_without_invalidating_the_session() {
    let f = Fixture::new().await;
    let session_id = f.cookie.split_once('=').unwrap().1.to_owned();
    for cookie in [None, Some("c2a_admin_session=missing-session")] {
        let response = f
            .with_auth("GET", "/admin/api/session", json!(null), cookie, None)
            .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(body(response).await["error"]["code"], "unauthorized");
    }
    f.storage.close().await;
    for (method, path) in [
        ("GET", "/admin/api/session"),
        ("GET", "/admin/api/settings/gateway"),
        ("POST", "/admin/api/logout"),
    ] {
        let response = f.request(method, path, json!(null)).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE, "{path}");
        assert!(response.headers().get("set-cookie").is_none());
        assert_eq!(body(response).await["error"]["code"], "session_unavailable");
    }
    let reopened = codex2api_storage::Storage::open(f.dir.path().join("test.sqlite"))
        .await
        .unwrap();
    assert!(
        reopened
            .get_admin_session(&session_id)
            .await
            .unwrap()
            .is_some()
    );
    reopened.close().await;
}

#[tokio::test]
async fn expired_session_returns_401_for_session_reads_and_protected_routes() {
    let f = Fixture::new().await;
    let expired = f
        .storage
        .create_admin_session(std::time::Duration::ZERO)
        .await
        .unwrap();
    let cookie = format!("c2a_admin_session={}", expired.id);
    for path in ["/admin/api/session", "/admin/api/settings/gateway"] {
        let response = f
            .with_auth("GET", path, json!(null), Some(&cookie), None)
            .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(body(response).await["error"]["code"], "unauthorized");
    }
}
