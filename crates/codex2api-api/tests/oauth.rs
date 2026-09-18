use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use codex2api_accounts::{AccountStore, AuthDotJson, TokenData};
use codex2api_storage::{
    AccountStatus, AccountUpdate, GatewaySettings, Storage, UaMode, hash_api_key,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const ROOT: &str = "/api/oauth/chatgpt";

async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

fn claims(token: &str) -> Value {
    serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(token.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap()
}

async fn refresh(app: &Router, rt: &str) -> axum::response::Response {
    app.clone().oneshot(Request::post(format!("{ROOT}/oauth/token"))
        .header("content-type", "application/json")
        .header("x-codex-installation-id", "test-device")
        .header("user-agent", "ThirdParty/1.0")
        .body(Body::from(json!({"grant_type":"refresh_token","refresh_token":rt,"client_id":codex2api_version::OAUTH_CLIENT_ID}).to_string())).unwrap()).await.unwrap()
}

#[tokio::test]
async fn refresh_issues_isolated_credentials_and_protects_all_prefixed_routes() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("oauth.sqlite"))
        .await
        .unwrap();
    let accounts = AccountStore::open(storage.clone());
    let account = accounts.create_pending().await.unwrap().account;
    storage
        .update_account(
            &account.id,
            AccountUpdate {
                status: Some(AccountStatus::Active),
                chatgpt_account_id: Some("workspace-fixture".into()),
                chatgpt_user_id: Some("user-fixture".into()),
                email: Some("fixture@example.test".into()),
                plan_type: Some("plus".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    accounts
        .save_auth_for_account(
            &account.id,
            &AuthDotJson::chatgpt(
                TokenData {
                    id_token: "official-id-secret".into(),
                    access_token: "official-access-secret".into(),
                    refresh_token: "official-refresh-secret".into(),
                    account_id: Some("workspace-fixture".into()),
                },
                None,
            ),
        )
        .await
        .unwrap();
    let credential = storage
        .create_oauth_credential(&account.id, "third-party")
        .await
        .unwrap();
    let rt = storage
        .oauth_refresh_token(&credential.id)
        .await
        .unwrap()
        .unwrap();
    let auth = codex2api_auth::AuthService::new(accounts.clone()).unwrap();
    let app = codex2api_api::router(codex2api_api::ApiState::new(
        storage.clone(),
        accounts,
        codex2api_upstream::UpstreamPool::new(auth),
    ));
    let response = refresh(&app, &rt).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["pragma"], "no-cache");
    let tokens = json_body(response).await;
    let access = tokens["access_token"].as_str().unwrap();
    assert_eq!(tokens["refresh_token"], rt);
    assert_eq!(tokens["token_type"], "Bearer");
    assert_eq!(tokens["expires_in"], 3600);
    assert!(!tokens.to_string().contains("official-"));
    for token in [access, tokens["id_token"].as_str().unwrap()] {
        let claims = claims(token);
        assert_eq!(
            claims["https://api.openai.com/auth"]["chatgpt_account_id"],
            "workspace-fixture"
        );
        assert_eq!(
            claims["https://api.openai.com/auth"]["chatgpt_plan_type"],
            "plus"
        );
        assert_eq!(claims["email"], "fixture@example.test");
        assert_eq!(claims["iss"], "codex2api");
    }
    assert_eq!(
        storage
            .lookup_oauth_access_hash(&hash_api_key(access))
            .await
            .unwrap()
            .unwrap()
            .account_id,
        account.id
    );
    let devices = storage
        .oauth_devices_for_account(&account.id)
        .await
        .unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].installation_id.as_deref(), Some("test-device"));
    assert_eq!(devices[0].user_agent, "ThirdParty/1.0");
    assert!(devices[0].last_used_at.is_none());
    let form = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("grant_type", "refresh_token"),
            ("refresh_token", rt.as_str()),
        ])
        .finish();
    let response = app
        .clone()
        .oneshot(
            Request::post(format!("{ROOT}/oauth/token"))
                .header(
                    "content-type",
                    "application/x-www-form-urlencoded; charset=UTF-8",
                )
                .body(Body::from(form))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let second = json_body(response).await;
    assert_ne!(second["access_token"], tokens["access_token"]);
    assert_eq!(second["refresh_token"], rt);
    assert!(
        storage
            .lookup_oauth_access_hash(&hash_api_key(access))
            .await
            .unwrap()
            .is_some()
    );

    let api_key = storage
        .create_proxy_api_key(&account.id, None)
        .await
        .unwrap();
    for bearer in [
        &rt,
        tokens["id_token"].as_str().unwrap(),
        &format!("{access}tampered"),
        &api_key.token,
    ] {
        for (method, path) in [
            ("POST", "/backend-api/codex/responses"),
            ("GET", "/backend-api/codex/responses"),
            ("GET", "/backend-api/codex/models"),
            ("GET", "/backend-api/wham/usage"),
            ("GET", "/backend-api/wham/accounts/check"),
            ("POST", "/backend-api/codex/images/generations"),
            ("POST", "/backend-api/codex/alpha/search"),
            ("POST", "/backend-api/codex/responses/compact"),
            ("POST", "/backend-api/codex/responses/response-id/cancel"),
            ("POST", "/v1/responses/input_tokens"),
            ("GET", "/backend-api/accounts/check/v4-2023-04-27"),
            ("GET", "/backend-api/subscriptions"),
            (
                "PATCH",
                "/backend-api/settings/account_user_setting?feature=training_allowed&value=false",
            ),
            ("GET", "/backend-api/codex/rtc_test"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(format!("{ROOT}{path}"))
                        .header("authorization", format!("Bearer {bearer}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "{method} {path}"
            );
            assert_eq!(json_body(response).await["error"]["code"], "invalid_token");
        }
    }
    // Existing API-key surface cannot be used to bypass OAuth scope.
    let response = app
        .clone()
        .oneshot(
            Request::post(format!("{ROOT}/v1/responses/input_tokens"))
                .header("authorization", format!("Bearer {access}"))
                .body(Body::from("invalid-json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        storage
            .query_usage(&Default::default())
            .await
            .unwrap()
            .total,
        0
    );
    for (method, path) in [
        (
            "GET",
            "/backend-api/subscriptions?account_id=another-workspace",
        ),
        (
            "PATCH",
            "/backend-api/settings/account_user_setting?feature=other&value=false",
        ),
        ("POST", "/backend-api/codex/responses/%2e%2e/models"),
        ("POST", "/backend-api/codex/responses/input_tokens/extra"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(format!("{ROOT}{path}"))
                    .header("authorization", format!("Bearer {access}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
    }
    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/responses")
                .header("authorization", format!("Bearer {access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = app
        .clone()
        .oneshot(
            Request::get(format!("{ROOT}/backend-api/wham/usage"))
                .header("authorization", format!("Bearer {access}"))
                .header("chatgpt-account-id", "another-workspace")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "account_mismatch"
    );
    storage
        .save_gateway_settings(&GatewaySettings::from_lines(UaMode::Blacklist, "blocked"))
        .await
        .unwrap();
    for method in ["GET", "POST"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(format!("{ROOT}/backend-api/codex/responses"))
                    .header("authorization", format!("Bearer {access}"))
                    .header("user-agent", "blocked")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }
    assert_eq!(
        refresh(&app, "invalid").await.status(),
        StatusCode::BAD_REQUEST
    );
    storage
        .set_oauth_paused(&credential.id, true)
        .await
        .unwrap();
    assert_eq!(
        json_body(refresh(&app, &rt).await).await["error"],
        "invalid_grant"
    );
    storage
        .set_oauth_paused(&credential.id, false)
        .await
        .unwrap();
    let (one, two) = tokio::join!(refresh(&app, &rt), refresh(&app, &rt));
    assert_eq!(one.status(), StatusCode::OK);
    assert_eq!(two.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(
            Request::post(format!("{ROOT}/oauth/revoke"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"token":rt}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(refresh(&app, &rt).await.status(), StatusCode::BAD_REQUEST);
    assert!(
        storage
            .lookup_oauth_access_hash(&hash_api_key(access))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        storage
            .load_account_tokens(&account.id)
            .await
            .unwrap()
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("official-refresh-secret")
    );
    storage.close().await;
}

#[tokio::test]
async fn malformed_token_requests_return_oauth_errors_without_secrets() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("errors.sqlite"))
        .await
        .unwrap();
    let accounts = AccountStore::open(storage.clone());
    let auth = codex2api_auth::AuthService::new(accounts.clone()).unwrap();
    let app = codex2api_api::router(codex2api_api::ApiState::new(
        storage.clone(),
        accounts,
        codex2api_upstream::UpstreamPool::new(auth),
    ));
    for (content_type, body, code) in [
        ("application/json", "{}", "invalid_request"),
        (
            "application/json",
            r#"{"grant_type":"password","refresh_token":"secret"}"#,
            "unsupported_grant_type",
        ),
        (
            "application/json",
            r#"{"grant_type":"refresh_token","refresh_token":"secret","client_id":"wrong"}"#,
            "invalid_client",
        ),
        (
            "application/x-www-form-urlencoded",
            "grant_type=refresh_token&refresh_token=secret&refresh_token=other",
            "invalid_request",
        ),
        ("text/plain", "secret", "invalid_request"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::post(format!("{ROOT}/oauth/token"))
                    .header("content-type", content_type)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(response.status().is_client_error());
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body = json_body(response).await;
        assert_eq!(body["error"], code);
        assert!(!body.to_string().contains("secret"));
    }
    let response = app
        .oneshot(
            Request::post(format!("{ROOT}/oauth/token"))
                .header("content-type", "application/json")
                .body(Body::from("x".repeat(17000)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    storage.close().await;
}
