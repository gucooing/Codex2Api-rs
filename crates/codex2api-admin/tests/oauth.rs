use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use codex2api_accounts::AccountStore;
use codex2api_auth::{AuthService, OAuthConfig};
use codex2api_storage::Storage;
use tower::ServiceExt;

#[tokio::test]
async fn oauth_chooser_requires_confirmation_and_manual_callback_has_no_public_listener_route() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("oauth.sqlite"))
        .await
        .unwrap();
    let proxy = storage
        .create_outbound_proxy("login-proxy", "http://127.0.0.1:1080")
        .await
        .unwrap();
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let accounts = AccountStore::open(storage.clone());
    let auth = AuthService::with_config(
        accounts.clone(),
        OAuthConfig {
            callback_port: occupied.local_addr().unwrap().port(),
            ..Default::default()
        },
    )
    .unwrap();
    let app = codex2api_admin::router(codex2api_admin::AdminState::from_parts(
        storage.clone(),
        accounts.clone(),
        auth.clone(),
    ));
    let session = storage
        .login_admin("admin", "admin", std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let cookie = format!("{}={}", codex2api_admin::SESSION_COOKIE, session.id);
    let get = |path: &str| {
        Request::get(path)
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap()
    };
    let post = |path: &str, body: String| {
        Request::post(path)
            .header("cookie", &cookie)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(body))
            .unwrap()
    };
    let response = app.clone().oneshot(get("/admin")).await.unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    assert!(html.contains("data-account-wizard-open"));
    assert!(html.contains("id=\"account-wizard-dialog\""));
    assert!(storage.list_accounts().await.unwrap().is_empty());
    let response = app
        .clone()
        .oneshot(get("/admin/oauth/setup"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let setup = std::str::from_utf8(&body).unwrap();
    assert!(setup.contains("data-wizard-step=\"1\""));
    assert!(setup.contains("data-wizard-step=\"2\""));
    assert!(setup.contains("data-wizard-step=\"3\""));
    assert!(setup.contains("name=\"method\" value=\"callback\""));
    assert!(setup.contains("name=\"method\" value=\"device\""));
    assert!(setup.contains("name=\"method\" value=\"refresh_token\""));
    assert!(setup.contains("name=\"proxy_id\""));
    assert!(setup.contains("login-proxy"));
    assert!(storage.list_accounts().await.unwrap().is_empty());
    let value = |name: &str| {
        setup
            .split(&format!("name=\"{name}\" value=\""))
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .to_string()
    };
    let csrf = value("csrf");
    let installation_id = value("installation_id");
    let os_type = value("os_type");
    let os_version = value("os_version");
    let arch = value("arch");
    let terminal = value("terminal");
    let start_form = |csrf_value: &str, proxy_id: &str| {
        url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs([
                ("csrf", csrf_value),
                ("method", "callback"),
                ("installation_id", installation_id.as_str()),
                ("os_type", os_type.as_str()),
                ("os_version", os_version.as_str()),
                ("arch", arch.as_str()),
                ("terminal", terminal.as_str()),
                ("proxy_id", proxy_id),
                ("timezone", "Asia/Taipei"),
            ])
            .finish()
    };
    let response = app
        .clone()
        .oneshot(post("/admin/oauth/start", start_form("bad", "")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(storage.list_accounts().await.unwrap().is_empty());
    let response = app
        .clone()
        .oneshot(post("/admin/oauth/start", start_form(&csrf, "missing")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(storage.list_accounts().await.unwrap().is_empty());
    let response = app
        .clone()
        .oneshot(post("/admin/oauth/start", start_form(&csrf, &proxy.id)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let state_id = result["state"].as_str().unwrap();
    assert_eq!(result["method"], "callback");
    assert!(result.get("code_verifier").is_none());
    assert!(storage.list_accounts().await.unwrap().is_empty());
    assert!(storage.get_oauth_pending(state_id).await.unwrap().is_none());
    let pending = auth.pending_login(state_id).await.unwrap().unwrap();
    let location = format!("/admin/oauth?state={state_id}");
    let response = app.clone().oneshot(get(&location)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    assert!(html.contains("data-callback-form"));
    assert!(html.contains("完整回调链接"));
    assert!(!html.contains(&pending.code_verifier));
    let callback = format!(
        "{}?code=private-code-fixture&state=wrong-state",
        pending.redirect_uri
    );
    let form = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("csrf", csrf.as_str()),
            ("state", state_id),
            ("callback_url", callback.as_str()),
        ])
        .finish();
    let response = app
        .clone()
        .oneshot(post("/admin/oauth/callback", form))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        !std::str::from_utf8(&body)
            .unwrap()
            .contains("private-code-fixture")
    );
    assert!(auth.pending_login(state_id).await.unwrap().is_some());
    assert!(storage.list_accounts().await.unwrap().is_empty());
    let response = app
        .clone()
        .oneshot(post(
            "/admin/oauth/cancel",
            format!("csrf=bad&state={state_id}"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(auth.pending_login(state_id).await.unwrap().is_some());
    let response = app
        .clone()
        .oneshot(post(
            "/admin/oauth/cancel",
            format!("csrf={csrf}&state={state_id}"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(auth.pending_login(state_id).await.unwrap().is_none());
    assert!(storage.list_accounts().await.unwrap().is_empty());
    let response = app
        .clone()
        .oneshot(get("/auth/callback?code=fixture&state=fixture"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    // Existing accounts still use the re-login flow and retain their identity.
    let account = accounts.create_pending().await.unwrap();
    let id = account.account.id;
    storage
        .set_account_proxy(&id, Some(&proxy.id))
        .await
        .unwrap();
    let original = storage.require_account(&id).await.unwrap();
    assert_eq!(original.proxy_id.as_deref(), Some(proxy.id.as_str()));
    let response = app
        .clone()
        .oneshot(get(&format!("/admin/accounts/{id}")))
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        std::str::from_utf8(&body)
            .unwrap()
            .contains("data-oauth-dialog-open")
    );
    let response = app
        .oneshot(post(
            &format!("/admin/accounts/{id}/relogin"),
            format!("csrf={csrf}&method=callback"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(storage.list_accounts().await.unwrap().len(), 1);
    assert_eq!(
        storage
            .require_account(&id)
            .await
            .unwrap()
            .proxy_id
            .as_deref(),
        Some(proxy.id.as_str())
    );
    assert_eq!(
        storage.require_account(&id).await.unwrap().installation_id,
        original.installation_id
    );
    storage.close().await;
}
