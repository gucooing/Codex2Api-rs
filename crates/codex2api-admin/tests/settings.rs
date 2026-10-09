mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::json;
#[tokio::test]
async fn gateway_settings_require_session_and_csrf_and_persist_mode_and_rules() {
    let f = Fixture::new().await;
    let mut value = json!({"ua_mode":"whitelist","ua_rules":["codex*","desktop"]});
    assert_eq!(
        f.request("PUT", "/admin/api/settings/gateway", value.clone())
            .await
            .status(),
        StatusCode::OK
    );
    value["default_rpm"] = 20.into();
    assert_eq!(f.get("/admin/api/settings/gateway").await, value);
    value["default_rpm"] = 0.into();
    assert_eq!(
        f.request("PUT", "/admin/api/settings/gateway", value.clone())
            .await
            .status(),
        StatusCode::OK
    );
    let reopened = codex2api_storage::Storage::open(f.dir.path().join("test.sqlite"))
        .await
        .unwrap();
    assert_eq!(reopened.gateway_settings().await.unwrap().default_rpm, 0);
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
async fn desktop_settings_and_resources_share_persisted_data() {
    let f = Fixture::new().await;
    assert_eq!(
        f.request(
            "PUT",
            "/admin/api/settings/desktop",
            json!({"proxy_id":null,"resource_cache_minutes":0})
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        f.request(
            "PUT",
            "/admin/api/settings/desktop",
            json!({"proxy_id":"missing","resource_cache_minutes":60})
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let input = json!({"proxy_id":null,"resource_cache_minutes":30});
    assert_eq!(
        f.request("PUT", "/admin/api/settings/desktop", input.clone())
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(f.get("/admin/api/settings/desktop").await, input);
    assert_eq!(
        f.request("GET", "/admin/api/diagnostics", json!(null))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.request(
            "PUT",
            "/admin/api/settings/desktop",
            json!({"collect_diagnostics":true,"resource_cache_minutes":30})
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    for path in ["/admin/api/resources", "/admin/api/missing-endpoints"] {
        assert!(f.get(path).await["items"].is_array());
    }
}

#[tokio::test]
async fn admin_cookie_security_uses_configured_origin_for_login_logout_and_password_changes() {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let f = Fixture::new().await;
    for secure in [true, false] {
        let app = codex2api_admin::router(f.state.clone().with_secure_cookies(secure));
        for action in ["logout", "settings/security"] {
            let response = app
                .clone()
                .oneshot(
                    Request::post("/admin/api/login")
                        .header("content-type", "application/json")
                        .header("x-forwarded-proto", if secure { "http" } else { "https" })
                        .header(
                            "forwarded",
                            if secure { "proto=http" } else { "proto=https" },
                        )
                        .body(Body::from(
                            json!({"username":"admin","password":"admin"}).to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let cookie = response.headers()["set-cookie"]
                .to_str()
                .unwrap()
                .to_owned();
            assert_eq!(
                cookie.split(';').any(|part| part.trim() == "Secure"),
                secure
            );
            let attributes = cookie.split(';').map(str::trim).collect::<Vec<_>>();
            assert!(attributes.contains(&"HttpOnly"));
            assert!(attributes.contains(&"Path=/admin"));
            assert!(attributes.contains(&"SameSite=Lax"));
            let session = body(response).await;
            let input = if action == "logout" {
                json!({})
            } else {
                json!({"old_username":"admin","old_password":"admin","new_username":"admin","new_password":"admin"})
            };
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(if action == "logout" { "POST" } else { "PUT" })
                        .uri(format!("/admin/api/{action}"))
                        .header("content-type", "application/json")
                        .header("cookie", cookie.split(';').next().unwrap())
                        .header("x-csrf-token", session["csrf_token"].as_str().unwrap())
                        .body(Body::from(input.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let cleared = response.headers()["set-cookie"].to_str().unwrap();
            assert!(cleared.contains("Max-Age=0"));
            assert_eq!(
                cleared.split(';').any(|part| part.trim() == "Secure"),
                secure
            );
        }
    }
}
