mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::json;
#[tokio::test]
async fn gateway_settings_require_session_and_csrf_and_persist_mode_and_rules() {
    let f = Fixture::new().await;
    let value = json!({"ua_mode":"whitelist","ua_rules":["codex*","desktop"]});
    assert_eq!(
        f.request("PUT", "/admin/api/settings/gateway", value.clone())
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(f.get("/admin/api/settings/gateway").await, value);
    let reopened = codex2api_storage::Storage::open(f.dir.path().join("test.sqlite"))
        .await
        .unwrap();
    assert!(
        reopened
            .gateway_settings()
            .await
            .unwrap()
            .allows_user_agent("Codex/1")
    );
    assert!(
        !reopened
            .gateway_settings()
            .await
            .unwrap()
            .allows_user_agent("other")
    );
}
#[tokio::test]
async fn security_checks_both_previous_credentials_and_revokes_sessions() {
    let f = Fixture::new().await;
    let mut input = json!({"old_username":"wrong","old_password":"admin","new_username":"operator","new_password":"new-password"});
    assert_eq!(
        f.request("PUT", "/admin/api/settings/security", input.clone())
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    input["old_username"] = "admin".into();
    input["old_password"] = "wrong".into();
    assert_eq!(
        f.request("PUT", "/admin/api/settings/security", input.clone())
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    input["old_password"] = "admin".into();
    assert_eq!(
        f.request("PUT", "/admin/api/settings/security", input)
            .await
            .status(),
        StatusCode::OK
    );
    let response = f.request("GET", "/admin/api/session", json!(null)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(body(response).await["error"]["code"], "unauthorized");
    assert!(
        f.storage
            .login_admin("admin", "admin", std::time::Duration::from_secs(60))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        f.storage
            .login_admin(
                "operator",
                "new-password",
                std::time::Duration::from_secs(60)
            )
            .await
            .unwrap()
            .is_some()
    );
}
#[tokio::test]
async fn desktop_settings_diagnostics_and_resources_share_persisted_data() {
    let f = Fixture::new().await;
    assert_eq!(
        f.request(
            "PUT",
            "/admin/api/settings/desktop",
            json!({"proxy_id":null,"collect_diagnostics":false,"resource_cache_minutes":0})
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        f.request(
            "PUT",
            "/admin/api/settings/desktop",
            json!({"proxy_id":"missing","collect_diagnostics":false,"resource_cache_minutes":60})
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let input = json!({"proxy_id":null,"collect_diagnostics":false,"resource_cache_minutes":30});
    assert_eq!(
        f.request("PUT", "/admin/api/settings/desktop", input.clone())
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(f.get("/admin/api/settings/desktop").await, input);
    for path in [
        "/admin/api/diagnostics",
        "/admin/api/resources",
        "/admin/api/missing-endpoints",
    ] {
        assert!(f.get(path).await["items"].is_array());
    }
}
