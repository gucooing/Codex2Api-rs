mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::json;
#[tokio::test]
async fn oauth_setup_is_randomized_and_callback_stays_admin_only() {
    let f = Fixture::new().await;
    let setup = f.get("/admin/api/suppliers/oauth/setup").await;
    assert!(setup["fingerprint"]["os_type"].is_string());
    assert!(setup["fingerprint"].get("installation_id").is_none());
    assert!(setup.get("refresh_token").is_none());
    assert_eq!(f.storage.list_accounts().await.unwrap().len(), 0);
    let mut input = json!({"method":"callback","fingerprint":setup["fingerprint"]});
    input["fingerprint"]["timezone"] = "invalid".into();
    assert_eq!(
        f.request("POST", "/admin/api/suppliers/oauth/start", input)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(f.storage.list_accounts().await.unwrap().len(), 0);
    for path in [
        "/auth/callback",
        "/admin/oauth/callback",
        "/admin/oauth/start",
    ] {
        assert_eq!(
            f.request("POST", path, json!({})).await.status(),
            StatusCode::NOT_FOUND
        );
    }
    let r=f.request("POST","/admin/api/suppliers/oauth/callback",json!({"state":"missing","callback_url":"http://localhost/callback?code=bad&state=missing"})).await;
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        f.request(
            "POST",
            "/admin/api/suppliers/oauth/cancel",
            json!({"state":"missing"})
        )
        .await
        .status(),
        StatusCode::OK
    );
}
