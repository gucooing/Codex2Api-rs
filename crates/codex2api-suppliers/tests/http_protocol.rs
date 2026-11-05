use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::{Json, Router, body::Bytes, http::HeaderMap, routing::post};
use codex2api_suppliers::transport::AccountHttpClients;
use codex2api_suppliers::{AccountIdentity, HostRuntime, SupplierAccountStore};
use codex2api_suppliers::{
    AuthService, OAuthConfig, exchange_code_for_tokens, refresh_tokens, revoke_tokens,
};
use serde_json::json;

#[tokio::test]
async fn oauth_exchange_refresh_and_revoke_use_the_right_identity() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let app = Router::new().route(
        "/oauth/token",
        post(move |headers: HeaderMap, body: Bytes| {
            let tx = tx.clone();
            async move {
                tx.send((headers, body)).await.unwrap();
                Json(json!({"id_token":"id", "access_token":"access", "refresh_token":"refresh"}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    let endpoint = "http://official.invalid/oauth/token".to_string();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let cfg = OAuthConfig {
        token_url: endpoint.clone(),
        revoke_url: endpoint,
        ..OAuthConfig::default()
    };
    let identity = AccountIdentity::new("account", "installation", HostRuntime::generate());
    let http = AccountHttpClients::with_proxy(&identity, Some(&proxy)).unwrap();

    exchange_code_for_tokens(
        &http.raw,
        &cfg,
        "http://localhost:1455/auth/callback",
        "verifier +&",
        "code +&",
    )
    .await
    .unwrap();
    let (headers, body) = rx.recv().await.unwrap();
    assert!(!headers.contains_key("originator"));
    assert!(!headers.contains_key("user-agent"));
    assert_eq!(headers["content-type"], "application/x-www-form-urlencoded");
    assert_eq!(
        std::str::from_utf8(&body).unwrap(),
        format!(
            "grant_type=authorization_code&client_id={}&code=code+%2B%26&redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback&code_verifier=verifier+%2B%26",
            codex2api_common::version::OAUTH_CLIENT_ID,
        )
    );
    let form: std::collections::HashMap<_, _> =
        url::form_urlencoded::parse(&body).into_owned().collect();
    assert_eq!(form["grant_type"], "authorization_code");
    assert_eq!(form["code_verifier"], "verifier +&");

    codex2api_suppliers::oauth::obtain_api_key(&http.raw, &cfg, "id")
        .await
        .unwrap();
    let (headers, body) = rx.recv().await.unwrap();
    assert!(!headers.contains_key("user-agent"));
    let form: std::collections::HashMap<_, _> =
        url::form_urlencoded::parse(&body).into_owned().collect();
    assert_eq!(
        form["grant_type"],
        "urn:ietf:params:oauth:grant-type:token-exchange"
    );
    assert_eq!(form["requested_token"], "openai-api-key");

    refresh_tokens(
        &http.authenticated,
        &cfg,
        "refresh",
        codex2api_suppliers::oauth::RefreshReason::Forced,
    )
    .await
    .unwrap();
    let (headers, body) = rx.recv().await.unwrap();
    assert_eq!(headers["user-agent"], identity.official_user_agent());
    assert_eq!(headers["originator"], "codex_cli_rs");
    assert!(!headers.contains_key("authorization"));
    assert!(!headers.contains_key("version"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        json!({
            "client_id":codex2api_common::version::OAUTH_CLIENT_ID, "grant_type":"refresh_token", "refresh_token":"refresh", "refresh_reason":"forced"
        })
    );

    revoke_tokens(&http.authenticated, &cfg, Some("refresh"), Some("access"))
        .await
        .unwrap();
    let (headers, body) = rx.recv().await.unwrap();
    assert_eq!(headers["user-agent"], identity.official_user_agent());
    assert_eq!(headers["originator"], "codex_cli_rs");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        json!({
            "client_id":codex2api_common::version::OAUTH_CLIENT_ID, "token_type_hint":"refresh_token", "token":"refresh"
        })
    );
    server.abort();
}

#[tokio::test]
async fn proactive_refresh_and_concurrent_401s_do_not_reuse_rotated_tokens() {
    let requests = Arc::new(AtomicUsize::new(0));
    let reasons = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let observed_reasons = reasons.clone();
    let count = requests.clone();
    let app = Router::new().route("/oauth/token", post(move |Json(body): Json<serde_json::Value>| {
        let count = count.clone();
        let reasons = observed_reasons.clone();
        async move {
            reasons.lock().await.push(body["refresh_reason"].as_str().unwrap().to_owned());
            let n = count.fetch_add(1, Ordering::SeqCst) + 1;
            Json(json!({"access_token":format!("access-{n}"), "refresh_token":format!("refresh-{n}")}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/oauth/token", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let temp = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(temp.path().join("test-pg"))
        .await
        .unwrap();
    let accounts = SupplierAccountStore::open(storage.clone());
    let pending = accounts.create_pending().await.unwrap();
    let id = pending.account.id.clone();
    let mut tokens = codex2api_suppliers::persist::chatgpt_auth(
        "id".into(),
        "old-access".into(),
        "old-refresh".into(),
        Some("chatgpt-account".into()),
    );
    tokens.last_refresh = Some(chrono::Utc::now() - chrono::Duration::days(9));
    accounts.save_auth_for_account(&id, &tokens).await.unwrap();
    let auth = AuthService::with_config(
        accounts.clone(),
        OAuthConfig {
            token_url: endpoint,
            ..OAuthConfig::default()
        },
    )
    .unwrap();
    let updated = auth.refresh(&id, false).await.unwrap();
    assert_eq!(
        updated.auth.tokens.as_ref().unwrap().access_token,
        "access-1"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    auth.refresh(&id, false).await.unwrap();
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    let (a, b) = tokio::join!(
        auth.refresh_rejected_token(&id, "access-1"),
        auth.refresh_rejected_token(&id, "access-1")
    );
    assert_eq!(
        a.unwrap().auth.tokens.as_ref().unwrap().access_token,
        "access-2"
    );
    assert_eq!(
        b.unwrap().auth.tokens.as_ref().unwrap().access_token,
        "access-2"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 2);
    auth.refresh(&id, true).await.unwrap();
    assert_eq!(requests.load(Ordering::SeqCst), 3);
    assert_eq!(
        *reasons.lock().await,
        ["scheduled", "unauthorized_recovery", "forced"]
    );
    let clients = auth.account_http(&id).await.unwrap();
    assert!(Arc::ptr_eq(
        &clients,
        &auth.account_http(&id).await.unwrap()
    ));
    let second = accounts.create_pending().await.unwrap();
    assert!(!Arc::ptr_eq(
        &clients,
        &auth.account_http(&second.account.id).await.unwrap()
    ));
    let ctx = accounts.load_context(&id).await.unwrap();
    assert_eq!(
        ctx.identity.installation_id,
        pending.identity.installation_id
    );
    server.abort();
    storage.close().await;
}

#[tokio::test]
async fn a_late_refresh_success_or_rejection_cannot_replace_a_new_login() {
    for rejected in [false, true] {
        let received = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let (started, finish) = (received.clone(), release.clone());
        let app = Router::new().route("/oauth/token", post(move || {
            let (started, finish) = (started.clone(), finish.clone());
            async move {
                started.notify_one();
                finish.notified().await;
                if rejected {
                    (axum::http::StatusCode::BAD_REQUEST, Json(json!({"error":"invalid_grant"})))
                } else {
                    (axum::http::StatusCode::OK, Json(json!({"access_token":"late-access","refresh_token":"late-refresh"})))
                }
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/oauth/token", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let directory = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(directory.path().join("late-refresh"))
            .await
            .unwrap();
        let accounts = SupplierAccountStore::open(storage.clone());
        let pending = accounts.create_pending().await.unwrap();
        let id = pending.account.id.clone();
        let credentials = |access: &str, refresh: &str| {
            codex2api_suppliers::persist::chatgpt_auth(
                "fixture".into(),
                access.into(),
                refresh.into(),
                Some("workspace".into()),
            )
        };
        accounts
            .save_auth_for_account(&id, &credentials("old-access", "old-refresh"))
            .await
            .unwrap();
        let service = AuthService::with_config(
            accounts.clone(),
            OAuthConfig {
                token_url: endpoint,
                ..Default::default()
            },
        )
        .unwrap();
        let refresh = {
            let (service, id) = (service.clone(), id.clone());
            tokio::spawn(async move { service.refresh(&id, true).await })
        };
        tokio::time::timeout(std::time::Duration::from_secs(60), received.notified())
            .await
            .unwrap();
        accounts
            .save_auth_for_account(&id, &credentials("new-login-access", "new-login-refresh"))
            .await
            .unwrap();
        let revision = storage.supplier_auth_revision(&id).await.unwrap();
        release.notify_one();
        let result = refresh.await.unwrap();
        assert_eq!(result.is_err(), rejected);
        let current = service.credentials(&id).await.unwrap();
        let tokens = current.auth.tokens.as_ref().unwrap();
        assert_eq!(tokens.access_token, "new-login-access");
        assert_eq!(tokens.refresh_token, "new-login-refresh");
        assert_eq!(Some(current.revision), revision);
        assert!(
            !storage
                .supplier_health(&id)
                .await
                .unwrap()
                .authentication_invalid()
        );
        assert_eq!(
            accounts
                .load_context(&id)
                .await
                .unwrap()
                .identity
                .installation_id,
            pending.identity.installation_id
        );
        server.abort();
        storage.close().await;
    }
}
