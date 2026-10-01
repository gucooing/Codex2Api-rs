use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use codex2api_storage::{Storage, VirtualAccount};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const ROOT: &str = "/api/oauth/chatgpt";

async fn request(
    app: &Router,
    path: &str,
    body: Option<Value>,
    cookie: Option<&str>,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method(if body.is_some() { "POST" } else { "GET" })
        .uri(format!("{ROOT}{path}"))
        .header("host", "service.example.test")
        .header("content-type", "application/json");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    app.clone()
        .oneshot(
            builder
                .body(
                    body.map(|body| Body::from(body.to_string()))
                        .unwrap_or_else(Body::empty),
                )
                .unwrap(),
        )
        .await
        .unwrap()
}
async fn payload(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}

#[tokio::test]
async fn device_login_preserves_pkce_single_use_and_existing_device_revocation() {
    let temporary = tempfile::tempdir().unwrap();
    let storage = Storage::open(temporary.path().join("device.sqlite"))
        .await
        .unwrap();
    let account = VirtualAccount {
        provider_id: "chatgpt".into(),
        id: uuid::Uuid::new_v4().to_string(),
        username: "alice".into(),
        password_hash: codex2api_storage::hash_password("fixture-password").unwrap(),
        name: "Alice".into(),
        email: "alice@example.test".into(),
        plan_type: "free".into(),
        plan_id: "free".into(),
        subscription_expires_at: None,
        enabled: true,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    storage.save_virtual_account(&account).await.unwrap();
    let suppliers = codex2api_accounts::SupplierAccountStore::open(storage.clone());
    let auth = codex2api_auth::AuthService::new(suppliers.clone()).unwrap();
    let app = codex2api_api::router(codex2api_api::ApiState::new(
        storage.clone(),
        suppliers,
        codex2api_upstream::UpstreamPool::new(auth),
    ));
    let issued = request(
        &app,
        "/api/accounts/deviceauth/usercode",
        Some(json!({"client_id":codex2api_version::OAUTH_CLIENT_ID})),
        None,
    )
    .await;
    assert_eq!(issued.status(), StatusCode::OK);
    let issued = payload(issued).await;
    assert_eq!(issued["interval"], "5"); // Official reader deserializes a string.
    let poll = json!({"device_auth_id":issued["device_auth_id"],"user_code":issued["user_code"]});
    assert_eq!(
        request(
            &app,
            "/api/accounts/deviceauth/token",
            Some(poll.clone()),
            None
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let bootstrap = request(&app, "/oauth/device/bootstrap", None, None).await;
    let cookie = bootstrap.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let flow = payload(bootstrap).await;
    let mut approval = json!({"request_id":flow["request_id"],"csrf_token":"x".repeat(64),"user_code":issued["user_code"],"username":"alice","password":"fixture-password"});
    assert_eq!(
        request(
            &app,
            "/oauth/device/approve",
            Some(approval.clone()),
            Some(&cookie)
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    approval["csrf_token"] = flow["csrf_token"].clone();
    assert_eq!(
        request(
            &app,
            "/oauth/device/approve",
            Some(approval.clone()),
            Some(&cookie)
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        request(&app, "/oauth/device/approve", Some(approval), Some(&cookie))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    sqlx::query("UPDATE virtual_device_authorizations SET last_poll_at=0")
        .execute(storage.pool())
        .await
        .unwrap();
    let response = request(&app, "/api/accounts/deviceauth/token", Some(poll), None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let code = payload(response).await;
    assert_eq!(
        code["code_challenge"],
        URL_SAFE_NO_PAD.encode(Sha256::digest(
            code["code_verifier"].as_str().unwrap().as_bytes()
        ))
    );
    let exchange = json!({"grant_type":"authorization_code","client_id":codex2api_version::OAUTH_CLIENT_ID,"redirect_uri":"http://service.example.test/api/oauth/chatgpt/deviceauth/callback","code":code["authorization_code"],"code_verifier":code["code_verifier"]});
    let response = request(&app, "/oauth/token", Some(exchange.clone()), None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let tokens = payload(response).await;
    assert_eq!(
        request(&app, "/oauth/token", Some(exchange), None)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let devices = storage.virtual_devices(&account.id).await.unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].virtual_account_id, account.id);
    assert_eq!(request(&app,"/oauth/revoke",Some(json!({"token":tokens["refresh_token"],"token_type_hint":"refresh_token","client_id":codex2api_version::OAUTH_CLIENT_ID})),None).await.status(),StatusCode::OK);
    assert!(
        storage
            .virtual_devices(&account.id)
            .await
            .unwrap()
            .is_empty()
    );
    storage.close().await;
}
