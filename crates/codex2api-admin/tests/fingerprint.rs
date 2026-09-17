use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use codex2api_accounts::{AuthDotJson, HttpFingerprint, TokenData};
use codex2api_storage::{AccountUpdate, Storage};
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn fingerprint_edits_are_validated_persisted_and_applied_to_only_this_account() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fingerprint.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let state = codex2api_admin::AdminState::new(storage.clone()).unwrap();
    let first = state.accounts.create_pending().await.unwrap();
    let second = state.accounts.create_pending().await.unwrap();
    let id = &first.account.id;
    let other_id = &second.account.id;
    let proxy = storage
        .create_outbound_proxy("test-proxy", "socks5h://127.0.0.1:1080")
        .await
        .unwrap();
    for account_id in [id, other_id] {
        let auth = AuthDotJson::chatgpt(
            TokenData {
                id_token: "id-token-fixture".into(),
                access_token: "access-token-fixture".into(),
                refresh_token: "refresh-token-fixture".into(),
                account_id: Some(account_id.clone()),
            },
            Some(chrono::Utc::now()),
        );
        state
            .accounts
            .save_auth_for_account(account_id, &auth)
            .await
            .unwrap();
    }
    let mut fingerprint = first.identity.http_fingerprint.clone();
    fingerprint.cookies = vec!["__cf_bm=stored-fixture".into()];
    storage
        .update_account(
            id,
            AccountUpdate {
                http_fingerprint_json: Some(fingerprint.to_json().unwrap()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let first_client = state.upstream.get(id).await.unwrap();
    let second_client = state.upstream.get(other_id).await.unwrap();
    let first_http = state.auth.account_http(id).await.unwrap();
    let second_http = state.auth.account_http(other_id).await.unwrap();
    let app = codex2api_admin::router(state.clone());
    let detail = format!("/admin/accounts/{id}?tab=fingerprint");
    let action = format!("/admin/accounts/{id}/fingerprint");
    let response = app
        .clone()
        .oneshot(Request::get(&detail).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()["location"], "/admin/login");
    let session = storage
        .login_admin("admin", "admin", std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let cookie = format!("{}={}", codex2api_admin::SESSION_COOKIE, session.id);
    let response = app
        .clone()
        .oneshot(
            Request::get(&detail)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = std::str::from_utf8(&body).unwrap();
    assert!(body.contains("保存指纹配置"));
    assert!(body.contains(r#"list="fingerprint-timezone-options""#));
    assert!(body.contains(r#"<option value="Asia/Taipei"></option>"#));
    assert!(body.contains(r#"<option value="America/Los_Angeles"></option>"#));
    assert!(body.contains("data-apply-proxy-timezone hidden"));
    assert!(body.contains(&first.account.installation_id));
    assert!(!body.contains("access-token-fixture"));
    assert!(!body.contains("refresh-token-fixture"));
    assert!(!body.contains("__cf_bm=stored-fixture"));
    let csrf = body
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let form = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("csrf", csrf),
            ("os_type", "Windows"),
            ("os_version", "10.0.26100"),
            ("arch", "aarch64"),
            ("terminal", "WindowsTerminal"),
            ("proxy_id", &proxy.id),
            ("timezone", "Asia/Taipei"),
        ])
        .finish();
    for (body, status) in [
        (form.replace(csrf, "invalid"), StatusCode::FORBIDDEN),
        (form.replace(&proxy.id, "missing"), StatusCode::BAD_REQUEST),
        (
            form.replace("Asia%2FTaipei", "Invalid%2FTimezone"),
            StatusCode::BAD_REQUEST,
        ),
        (
            form.replace("WindowsTerminal", "terminal%0D%0AX-Injected%3Ayes"),
            StatusCode::BAD_REQUEST,
        ),
        (
            format!("{form}&installation_id=changed"),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::post(&action)
                    .header("cookie", &cookie)
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(
            storage
                .require_account(id)
                .await
                .unwrap()
                .http_fingerprint_json,
            fingerprint.to_json().unwrap()
        );
    }
    let response = app
        .clone()
        .oneshot(
            Request::post(&action)
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(form))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        response.headers()["location"]
            .to_str()
            .unwrap()
            .contains("tab=fingerprint&ok=")
    );
    let updated = storage.require_account(id).await.unwrap();
    let expected_ua = "codex_cli_rs/0.154.0 (Windows 10.0.26100; aarch64) WindowsTerminal";
    assert_eq!(updated.user_agent, expected_ua);
    assert_eq!(updated.installation_id, first.account.installation_id);
    assert_eq!(updated.status, first.account.status);
    let response = app
        .clone()
        .oneshot(
            Request::get(&detail)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = std::str::from_utf8(&html).unwrap();
    assert!(html.contains("data-apply-proxy-timezone >应用代理时区"));
    assert!(!html.contains("data-apply-proxy-timezone hidden"));
    assert_eq!(updated.proxy_id.as_deref(), Some(proxy.id.as_str()));
    assert_eq!(
        HttpFingerprint::from_json(&updated.http_fingerprint_json)
            .unwrap()
            .timezone
            .as_deref(),
        Some("Asia/Taipei")
    );
    assert_eq!(
        HttpFingerprint::from_json(&updated.http_fingerprint_json)
            .unwrap()
            .cookies,
        fingerprint.cookies
    );
    assert_eq!(
        storage
            .load_account_tokens(id)
            .await
            .unwrap()
            .unwrap()
            .access_token
            .as_deref(),
        Some("access-token-fixture")
    );
    let client = state.upstream.get(id).await.unwrap();
    assert!(!Arc::ptr_eq(&client, &first_client));
    let headers = client.default_headers().unwrap();
    assert_eq!(headers["user-agent"], expected_ua);
    assert_eq!(headers["originator"], "codex_cli_rs");
    assert_eq!(
        client.identity().installation_id,
        first.account.installation_id
    );
    let http = state.auth.account_http(id).await.unwrap();
    assert_eq!(http.proxy_url(), Some(proxy.url.as_str()));
    assert!(second_http.proxy_url().is_none());
    assert!(!Arc::ptr_eq(&http, &first_http));
    assert!(Arc::ptr_eq(&http.cookies, &first_http.cookies));
    assert!(Arc::ptr_eq(&http.refresh_lock, &first_http.refresh_lock));
    assert!(Arc::ptr_eq(
        &state.upstream.get(other_id).await.unwrap(),
        &second_client
    ));
    assert!(Arc::ptr_eq(
        &state.auth.account_http(other_id).await.unwrap(),
        &second_http
    ));
    assert_eq!(
        storage
            .require_account(other_id)
            .await
            .unwrap()
            .http_fingerprint_json,
        second.account.http_fingerprint_json
    );
    storage.close().await;
    let reopened = Storage::open(&path).await.unwrap();
    assert_eq!(
        reopened
            .require_account(id)
            .await
            .unwrap()
            .http_fingerprint_json,
        updated.http_fingerprint_json
    );
    reopened.close().await;
}
