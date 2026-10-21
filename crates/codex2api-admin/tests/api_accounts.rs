mod common;
use axum::http::StatusCode;
use codex2api_common::providers::api as channel;
use codex2api_suppliers::api::ApiCredentials;
use common::*;
use serde_json::json;

#[tokio::test]
async fn supplier_key_requires_admin_session_and_stays_out_of_general_views() {
    let f = Fixture::new().await;
    let secret = "sk-api-supplier-fixture";
    let (account, _) = f
        .state
        .upstream
        .api()
        .auth()
        .create(
            "API fixture",
            ApiCredentials {
                api_key: secret.into(),
                base_url: "https://supplier.example.test/v1".into(),
                wire: channel::WIRE_CHAT_COMPLETIONS.into(),
                auth_scheme: channel::default_auth_scheme(channel::WIRE_CHAT_COMPLETIONS).into(),
                retry_statuses: vec![500, 502],
                retry_count: channel::DEFAULT_RETRY_COUNT,
            },
            None,
            &["fixture-model".into()],
        )
        .await
        .unwrap();
    let path = format!("/admin/api/suppliers/api/accounts/{}/key", account.id);
    assert_eq!(
        f.with_auth("GET", &path, json!(null), None, None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let response = f
        .request(
            "POST",
            "/admin/api/users",
            json!({
                "username":"key-reader", "password":"fixture-password", "name":"Key reader",
                "email":"reader@example.test", "enabled":true, "revision":null
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let user_id = body(response).await["id"].as_str().unwrap().to_owned();
    let user = f.storage.user(&user_id).await.unwrap().unwrap();
    let user_token = f
        .storage
        .create_user_session(&user, "fixture-csrf")
        .await
        .unwrap()
        .unwrap();
    let wrong_audience = format!("c2a_admin_session={user_token}");
    assert_eq!(
        f.with_auth("GET", &path, json!(null), Some(&wrong_audience), None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let response = f.request("GET", &path, json!(null)).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(body(response).await["api_key"], secret);
    for path in [
        "/admin/api/suppliers".to_owned(),
        format!("/admin/api/suppliers/{}", account.id),
    ] {
        assert!(!f.get(&path).await.to_string().contains(secret));
    }

    let other = f.state.accounts.create_pending().await.unwrap().account;
    let wrong_provider = format!("/admin/api/suppliers/api/accounts/{}/key", other.id);
    assert_eq!(
        f.request("GET", &wrong_provider, json!(null))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
}
