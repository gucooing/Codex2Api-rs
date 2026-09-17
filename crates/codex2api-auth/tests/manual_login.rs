use axum::{
    Json, Router,
    body::Bytes,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use base64::Engine;
use codex2api_accounts::{AccountIdentity, AccountStore, HostRuntime, new_installation_id};
use codex2api_auth::{AuthService, LoginFlow, OAuthConfig};
use codex2api_storage::Storage;
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

type Requests = Arc<Mutex<Vec<(String, HeaderMap, serde_json::Value)>>>;

async fn mock_issuer() -> (String, Requests, tokio::task::JoinHandle<()>) {
    let requests: Requests = Arc::default();
    let capture = requests.clone();
    let polling = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route("/api/accounts/deviceauth/usercode", post(move |headers: HeaderMap, Json(body): Json<serde_json::Value>| {
            capture.lock().unwrap().push(("usercode".into(), headers, body));
            async { Json(json!({"device_auth_id":"device-id", "usercode":"ABCD-EFGH", "interval":"1"})) }
        }))
        .route("/api/accounts/deviceauth/token", post({ let capture = requests.clone(); move |headers: HeaderMap, Json(body): Json<serde_json::Value>| {
            capture.lock().unwrap().push(("poll".into(), headers, body));
            let count = polling.fetch_add(1, Ordering::SeqCst);
            async move {
                if count == 0 { (StatusCode::FORBIDDEN, Json(json!({"pending":true}))) }
                else { (StatusCode::OK, Json(json!({"authorization_code":"device-code", "code_verifier":"device-verifier", "code_challenge":"challenge"}))) }
            }
        }}))
        .route("/oauth/token", post({ let capture = requests.clone(); move |headers: HeaderMap, body: Bytes| { let capture = capture.clone(); async move {
            if headers.get("content-type").is_some_and(|value| value.as_bytes().starts_with(b"application/json")) {
                let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
                capture.lock().unwrap().push(("refresh".into(), headers, request.clone()));
                if request["refresh_token"] == "invalid-refresh" { return Json(json!({})); }
                let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(&json!({
                    "email":"refresh@example.com", "https://api.openai.com/auth":{"chatgpt_account_id":"refresh-account","chatgpt_plan_type":"plus"}
                })).unwrap());
                return Json(json!({"id_token":format!("header.{payload}.signature"),"access_token":"refreshed-access","refresh_token":"rotated-refresh"}));
            }
            let params: HashMap<String, String> = url::form_urlencoded::parse(&body).into_owned().collect();
            capture.lock().unwrap().push(("exchange".into(), headers, json!(params)));
                if params.get("grant_type").is_some_and(|grant| grant.contains("token-exchange")) {
                    return Json(json!({"access_token":"api-key-fixture"}));
                }
                let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(&json!({
                    "email":"fixture@example.com", "https://api.openai.com/auth":{"chatgpt_account_id":"account-fixture","chatgpt_plan_type":"pro"}
                })).unwrap());
                Json(json!({"id_token":format!("header.{payload}.signature"),"access_token":"access-fixture","refresh_token":"refresh-fixture"}))
        }}}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (issuer, requests, server)
}

#[tokio::test]
async fn manual_callback_validates_full_url_and_works_with_occupied_callback_port() {
    let (issuer, requests, server) = mock_issuer().await;
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("manual.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let proxy = storage
        .create_outbound_proxy("callback-proxy", &issuer)
        .await
        .unwrap();
    let issuer = "http://official.invalid".to_string();
    let config = OAuthConfig {
        issuer: issuer.clone(),
        token_url: format!("{issuer}/oauth/token"),
        callback_port: occupied.local_addr().unwrap().port(),
        ..Default::default()
    };
    let auth =
        AuthService::with_config(AccountStore::open(storage.clone()), config.clone()).unwrap();
    let pending = auth
        .begin_manual_login_with_proxy(None, false, Some(&proxy.id))
        .await
        .unwrap();
    let account = storage
        .require_account(pending.account_id.as_ref().unwrap())
        .await
        .unwrap();
    assert!(matches!(
        auth.login_flow(&pending).unwrap(),
        LoginFlow::Callback { .. }
    ));
    storage.close().await;
    let storage = Storage::open(&path).await.unwrap();
    let auth = AuthService::with_config(AccountStore::open(storage.clone()), config).unwrap();
    let valid = format!(
        "{}?code=browser-code&state={}",
        pending.redirect_uri, pending.state
    );
    for invalid in [
        valid.replace(&pending.state, "wrong-state"),
        valid.replace("localhost", "example.com"),
        format!("{valid}&state={}", pending.state),
        format!("{valid}#fragment"),
        "browser-code".to_string(),
    ] {
        assert!(
            auth.complete_manual_callback(&pending.state, &invalid)
                .await
                .is_err()
        );
    }
    assert!(requests.lock().unwrap().is_empty());
    let done = auth
        .complete_manual_callback(&pending.state, &valid)
        .await
        .unwrap();
    assert_eq!(done.account.id, account.id);
    assert_eq!(done.account.proxy_id.as_deref(), Some(proxy.id.as_str()));
    assert_eq!(done.account.installation_id, account.installation_id);
    assert_eq!(
        done.account.http_fingerprint_json,
        account.http_fingerprint_json
    );
    assert!(
        storage
            .get_oauth_pending(&pending.state)
            .await
            .unwrap()
            .is_none()
    );
    let count = requests.lock().unwrap().len();
    assert!(
        auth.complete_manual_callback(&pending.state, &valid)
            .await
            .is_err()
    );
    assert_eq!(requests.lock().unwrap().len(), count);
    let request = requests.lock().unwrap()[0].clone();
    assert_eq!(request.2["redirect_uri"], pending.redirect_uri);
    assert_eq!(request.2["code_verifier"], pending.code_verifier);
    assert!(!request.1.contains_key("originator"));
    assert!(!request.1.contains_key("user-agent"));
    let relogin = auth.begin_relogin(&account.id).await.unwrap();
    let link = format!(
        "{}?code=relogin-code&state={}",
        relogin.redirect_uri, relogin.state
    );
    let again = auth
        .complete_manual_callback(&relogin.state, &link)
        .await
        .unwrap();
    assert!(again.reused_existing);
    assert_eq!(again.account.installation_id, account.installation_id);
    let replacement = storage
        .create_outbound_proxy("replacement", &proxy.url)
        .await
        .unwrap();
    let duplicate = auth
        .begin_manual_login_with_proxy(None, false, Some(&replacement.id))
        .await
        .unwrap();
    let link = format!(
        "{}?code=duplicate-code&state={}",
        duplicate.redirect_uri, duplicate.state
    );
    let merged = auth
        .complete_manual_callback(&duplicate.state, &link)
        .await
        .unwrap();
    assert_eq!(merged.account.id, account.id);
    assert_eq!(
        merged.account.proxy_id.as_deref(),
        Some(replacement.id.as_str())
    );
    assert_eq!(merged.account.installation_id, account.installation_id);
    assert_eq!(
        merged.account.http_fingerprint_json,
        account.http_fingerprint_json
    );
    let expired = storage
        .insert_oauth_pending(
            "expired",
            "verifier",
            &pending.redirect_uri,
            Some(&account.id),
            Duration::ZERO,
        )
        .await
        .unwrap();
    assert!(
        auth.complete_manual_callback(&expired.state, &valid)
            .await
            .is_err()
    );
    storage.close().await;
    server.abort();
}

#[tokio::test]
async fn refresh_token_login_uses_the_draft_identity_proxy_and_persists_only_the_result() {
    let (issuer, requests, server) = mock_issuer().await;
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("refresh.sqlite"))
        .await
        .unwrap();
    let proxy = storage
        .create_outbound_proxy("refresh-proxy", &issuer)
        .await
        .unwrap();
    let config = OAuthConfig {
        issuer: issuer.clone(),
        token_url: format!("{issuer}/oauth/token"),
        ..Default::default()
    };
    let auth = AuthService::with_config(AccountStore::open(storage.clone()), config).unwrap();
    let identity = AccountIdentity::new(
        uuid::Uuid::new_v4().to_string(),
        new_installation_id(),
        HostRuntime {
            originator: codex2api_version::DEFAULT_ORIGINATOR.into(),
            user_agent: codex2api_version::official_user_agent(
                "Ubuntu", "24.4.0", "aarch64", "tmux",
            ),
            os_type: "Ubuntu".into(),
            os_version: "24.4.0".into(),
            arch: "aarch64".into(),
        },
    );
    assert!(storage.list_accounts().await.unwrap().is_empty());
    assert!(
        auth.login_with_refresh_token(identity.clone(), Some(&proxy.id), "invalid-refresh")
            .await
            .is_err()
    );
    assert!(storage.list_accounts().await.unwrap().is_empty());
    let done = auth
        .login_with_refresh_token(identity.clone(), Some(&proxy.id), "initial-refresh")
        .await
        .unwrap();
    assert_eq!(
        done.account.status,
        codex2api_storage::AccountStatus::Active
    );
    assert_eq!(done.account.installation_id, identity.installation_id);
    assert_eq!(done.account.os_type, "Ubuntu");
    assert_eq!(done.account.arch, "aarch64");
    assert_eq!(done.account.proxy_id.as_deref(), Some(proxy.id.as_str()));
    assert_eq!(done.account.email.as_deref(), Some("refresh@example.com"));
    assert_eq!(
        done.tokens.access_token.as_deref(),
        Some("refreshed-access")
    );
    assert_eq!(
        done.tokens.refresh_token.as_deref(),
        Some("rotated-refresh")
    );
    assert_eq!(storage.list_accounts().await.unwrap().len(), 1);
    let captured = requests.lock().unwrap().clone();
    let refresh = captured
        .iter()
        .find(|request| request.0 == "refresh" && request.2["refresh_token"] == "initial-refresh")
        .unwrap();
    assert_eq!(refresh.2["refresh_token"], "initial-refresh");
    assert_eq!(
        refresh.1["originator"],
        codex2api_version::DEFAULT_ORIGINATOR
    );
    assert_eq!(
        refresh.1["user-agent"],
        identity.official_user_agent().as_str()
    );
    storage.close().await;
    server.abort();
}

fn draft_identity() -> AccountIdentity {
    AccountIdentity::new(
        uuid::Uuid::new_v4().to_string(),
        new_installation_id(),
        HostRuntime::generate(),
    )
}

#[tokio::test]
async fn callback_draft_is_not_saved_until_success_and_deduplicates_accounts() {
    let (issuer, requests, server) = mock_issuer().await;
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("draft.sqlite"))
        .await
        .unwrap();
    let proxy = storage
        .create_outbound_proxy("draft-proxy", &issuer)
        .await
        .unwrap();
    let auth = AuthService::with_config(
        AccountStore::open(storage.clone()),
        OAuthConfig {
            issuer: "http://official.invalid".into(),
            token_url: "http://official.invalid/oauth/token".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let identity = draft_identity();
    let pending = auth
        .begin_manual_login_with_identity(identity.clone(), false, Some(&proxy.id))
        .await
        .unwrap();
    assert!(pending.account_id.is_none());
    assert!(storage.list_accounts().await.unwrap().is_empty());
    assert!(
        storage
            .get_oauth_pending(&pending.state)
            .await
            .unwrap()
            .is_none()
    );
    let invalid = format!("{}?code=fixture&state=wrong", pending.redirect_uri);
    assert!(
        auth.complete_manual_callback(&pending.state, &invalid)
            .await
            .is_err()
    );
    assert!(requests.lock().unwrap().is_empty());
    let callback = format!(
        "{}?code=fixture&state={}",
        pending.redirect_uri, pending.state
    );
    let (first, second) = tokio::join!(
        auth.complete_manual_callback(&pending.state, &callback),
        auth.complete_manual_callback(&pending.state, &callback),
    );
    assert!(first.is_ok() != second.is_ok());
    let done = first.or(second).unwrap();
    assert_eq!(storage.list_accounts().await.unwrap().len(), 1);
    assert_eq!(done.account.installation_id, identity.installation_id);
    assert_eq!(
        done.account.http_fingerprint_json,
        identity.fingerprint_json().unwrap()
    );
    assert_eq!(done.account.proxy_id.as_deref(), Some(proxy.id.as_str()));
    assert_eq!(done.tokens.access_token.as_deref(), Some("access-fixture"));
    let again = auth
        .begin_manual_login_with_identity(draft_identity(), false, Some(&proxy.id))
        .await
        .unwrap();
    let callback = format!("{}?code=again&state={}", again.redirect_uri, again.state);
    let existing = auth
        .complete_manual_callback(&again.state, &callback)
        .await
        .unwrap();
    assert!(existing.reused_existing);
    assert_eq!(existing.account.id, done.account.id);
    assert_eq!(existing.account.installation_id, identity.installation_id);
    assert_eq!(
        existing.account.http_fingerprint_json,
        done.account.http_fingerprint_json
    );
    assert_eq!(storage.list_accounts().await.unwrap().len(), 1);
    let abandoned = auth
        .begin_manual_login_with_identity(draft_identity(), false, None)
        .await
        .unwrap();
    auth.cancel_draft_login(&abandoned.state).await;
    assert!(
        auth.pending_login(&abandoned.state)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        auth.complete_manual_callback(&abandoned.state, &callback)
            .await
            .is_err()
    );
    storage.close().await;
    server.abort();
}

#[tokio::test]
async fn draft_device_enforces_poll_interval_and_saves_only_after_authorization() {
    let (issuer, requests, server) = mock_issuer().await;
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("device-draft.sqlite"))
        .await
        .unwrap();
    let auth = AuthService::with_config(
        AccountStore::open(storage.clone()),
        OAuthConfig {
            token_url: format!("{issuer}/oauth/token"),
            issuer,
            ..Default::default()
        },
    )
    .unwrap();
    let identity = draft_identity();
    let pending = auth
        .begin_manual_login_with_identity(identity.clone(), true, None)
        .await
        .unwrap();
    assert!(storage.list_accounts().await.unwrap().is_empty());
    assert!(
        storage
            .get_oauth_pending(&pending.state)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        auth.poll_device_login(&pending.state)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        auth.poll_device_login(&pending.state)
            .await
            .unwrap()
            .is_none()
    );
    assert!(storage.list_accounts().await.unwrap().is_empty());
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.0 == "poll")
            .count(),
        1
    );
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let done = auth
        .poll_device_login(&pending.state)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(done.account.installation_id, identity.installation_id);
    assert_eq!(storage.list_accounts().await.unwrap().len(), 1);
    assert!(auth.poll_device_login(&pending.state).await.is_err());
    storage.close().await;
    server.abort();
}

#[tokio::test]
async fn device_code_uses_official_requests_persists_pending_state_and_enforces_poll_interval() {
    let (issuer, requests, server) = mock_issuer().await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let proxy = storage
        .create_outbound_proxy("device-proxy", &issuer)
        .await
        .unwrap();
    let issuer = "http://official.invalid".to_string();
    let config = OAuthConfig {
        issuer: issuer.clone(),
        token_url: format!("{issuer}/oauth/token"),
        ..Default::default()
    };
    let auth =
        AuthService::with_config(AccountStore::open(storage.clone()), config.clone()).unwrap();
    let pending = auth
        .begin_manual_login_with_proxy(None, true, Some(&proxy.id))
        .await
        .unwrap();
    let LoginFlow::Device {
        verification_url,
        user_code,
        interval,
        ..
    } = auth.login_flow(&pending).unwrap()
    else {
        panic!("device flow expected")
    };
    assert_eq!(verification_url, format!("{issuer}/codex/device"));
    assert_eq!(user_code, "ABCD-EFGH");
    assert_eq!(interval, 1);
    assert_eq!(
        pending.redirect_uri,
        format!("{issuer}/deviceauth/callback")
    );
    assert!(
        auth.complete_manual_callback(&pending.state, "http://localhost:1455/auth/callback")
            .await
            .is_err()
    );
    storage.close().await;
    let storage = Storage::open(&path).await.unwrap();
    let auth = AuthService::with_config(AccountStore::open(storage.clone()), config).unwrap();
    assert!(
        auth.poll_device_login(&pending.state)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        auth.poll_device_login(&pending.state)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.0 == "poll")
            .count(),
        1
    );
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let done = auth
        .poll_device_login(&pending.state)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(done.account.id, pending.account_id.unwrap());
    assert_eq!(done.account.proxy_id.as_deref(), Some(proxy.id.as_str()));
    assert_eq!(done.tokens.access_token.as_deref(), Some("access-fixture"));
    assert!(done.auth.openai_api_key.is_none());
    assert!(auth.poll_device_login(&pending.state).await.is_err());
    let captured = requests.lock().unwrap().clone();
    assert_eq!(
        captured[0].2,
        json!({"client_id":codex2api_version::OAUTH_CLIENT_ID})
    );
    assert_eq!(
        captured[1].2,
        json!({"device_auth_id":"device-id","user_code":"ABCD-EFGH"})
    );
    let exchanges: Vec<_> = captured
        .iter()
        .filter(|request| request.0 == "exchange")
        .collect();
    assert_eq!(exchanges.len(), 1);
    assert_eq!(
        exchanges[0].2["redirect_uri"],
        format!("{issuer}/deviceauth/callback")
    );
    assert_eq!(exchanges[0].2["code_verifier"], "device-verifier");
    for request in &captured {
        assert!(!request.1.contains_key("originator"));
        assert!(!request.1.contains_key("user-agent"));
        assert!(!request.1.contains_key("authorization"));
    }
    storage.close().await;
    server.abort();
}
