use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use codex2api_accounts::{AuthDotJson, TokenData};
use codex2api_storage::{QuotaSnapshot, Storage, UsageRecord};
use std::{sync::Arc, time::Duration};
use tower::ServiceExt;

#[tokio::test]
async fn deletion_requires_csrf_unbinds_proxy_accounts_and_cleans_account_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deletions.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let state = codex2api_admin::AdminState::new(storage.clone()).unwrap();
    let first = state.accounts.create_pending().await.unwrap().account;
    let second = state.accounts.create_pending().await.unwrap().account;
    let other = state.accounts.create_pending().await.unwrap().account;
    let proxy = storage
        .create_outbound_proxy("delete", "http://127.0.0.1:1080")
        .await
        .unwrap();
    let other_proxy = storage
        .create_outbound_proxy("keep", "socks5h://127.0.0.1:1081")
        .await
        .unwrap();
    for id in [&first.id, &second.id] {
        storage
            .set_account_proxy(id, Some(&proxy.id))
            .await
            .unwrap();
    }
    storage
        .set_account_proxy(&other.id, Some(&other_proxy.id))
        .await
        .unwrap();
    let auth = AuthDotJson::chatgpt(
        TokenData {
            id_token: "id-fixture".into(),
            access_token: "access-fixture".into(),
            refresh_token: "refresh-fixture".into(),
            account_id: Some(first.id.clone()),
        },
        Some(chrono::Utc::now()),
    );
    state
        .accounts
        .save_auth_for_account(&first.id, &auth)
        .await
        .unwrap();
    let first_upstream = state.upstream.get(&first.id).await.unwrap();
    let first_http = state.auth.account_http(&first.id).await.unwrap();
    let second_http = state.auth.account_http(&second.id).await.unwrap();
    let other_http = state.auth.account_http(&other.id).await.unwrap();
    let first_key = storage
        .create_proxy_api_key(&first.id, Some("first-key"))
        .await
        .unwrap();
    let second_key = storage
        .create_proxy_api_key(&second.id, Some("second-key"))
        .await
        .unwrap();
    for (id, pending) in [(&first.id, "first-pending"), (&second.id, "second-pending")] {
        storage
            .insert_oauth_pending(
                pending,
                "verifier",
                "http://localhost/callback",
                Some(id),
                Duration::from_secs(60),
            )
            .await
            .unwrap();
    }
    storage
        .store_account_quota(
            &first.id,
            &QuotaSnapshot {
                value: serde_json::json!({}),
                observed_at: chrono::Utc::now(),
            },
        )
        .await
        .unwrap();
    storage
        .insert_usage(&UsageRecord {
            id: "history".into(),
            account_id: first.id.clone(),
            account_name: "first account".into(),
            api_key_id: first_key.record.id.clone(),
            api_key_name: "first-key".into(),
            status: "completed".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let app = codex2api_admin::router(state.clone());
    let account_delete = format!("/admin/accounts/{}/delete", first.id);
    let proxy_delete = format!("/admin/proxies/{}/delete", proxy.id);
    for url in [&account_delete, &proxy_delete] {
        let response = app
            .clone()
            .oneshot(
                Request::post(url)
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("csrf=bad"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
    }
    let session = storage
        .login_admin("admin", "admin", Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let cookie = format!("{}={}", codex2api_admin::SESSION_COOKIE, session.id);
    let response = app
        .clone()
        .oneshot(
            Request::get("/admin")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    assert!(html.contains(&account_delete));
    let csrf = html
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let post = |url: &str, token: &str| {
        Request::post(url)
            .header("cookie", &cookie)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(format!("csrf={token}")))
            .unwrap()
    };
    for url in [&account_delete, &proxy_delete] {
        let response = app
            .clone()
            .oneshot(
                Request::get(url)
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        let response = app.clone().oneshot(post(url, "bad")).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    assert!(storage.get_account(&first.id).await.unwrap().is_some());
    assert_eq!(
        storage
            .require_account(&first.id)
            .await
            .unwrap()
            .proxy_id
            .as_deref(),
        Some(proxy.id.as_str())
    );
    let response = app
        .clone()
        .oneshot(
            Request::get("/admin/proxies")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        std::str::from_utf8(&body)
            .unwrap()
            .contains(&format!("data-proxy-delete=\"{proxy_delete}\""))
    );
    let response = app
        .clone()
        .oneshot(post(&proxy_delete, csrf))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let confirmation: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(confirmation["confirmation_required"], true);
    assert_eq!(confirmation["account_count"], 2);
    assert!(storage.require_outbound_proxy(&proxy.id).await.is_ok());
    assert_eq!(
        storage
            .require_account(&first.id)
            .await
            .unwrap()
            .proxy_id
            .as_deref(),
        Some(proxy.id.as_str())
    );
    let response = app
        .clone()
        .oneshot(
            Request::post(&proxy_delete)
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("csrf={csrf}&confirm_unbind=true")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(storage.require_outbound_proxy(&proxy.id).await.is_err());
    for id in [&first.id, &second.id] {
        assert!(
            storage
                .require_account(id)
                .await
                .unwrap()
                .proxy_id
                .is_none()
        );
        assert!(
            state
                .auth
                .account_http(id)
                .await
                .unwrap()
                .proxy_url()
                .is_none()
        );
    }
    assert!(!Arc::ptr_eq(
        &first_http,
        &state.auth.account_http(&first.id).await.unwrap()
    ));
    assert!(!Arc::ptr_eq(
        &second_http,
        &state.auth.account_http(&second.id).await.unwrap()
    ));
    assert!(!Arc::ptr_eq(
        &first_upstream,
        &state.upstream.get(&first.id).await.unwrap()
    ));
    assert!(Arc::ptr_eq(
        &other_http,
        &state.auth.account_http(&other.id).await.unwrap()
    ));
    assert_eq!(
        storage
            .require_account(&other.id)
            .await
            .unwrap()
            .proxy_id
            .as_deref(),
        Some(other_proxy.id.as_str())
    );
    assert!(
        storage
            .lookup_proxy_api_key(&first_key.token)
            .await
            .unwrap()
            .is_some()
    );
    let response = app
        .clone()
        .oneshot(post(&proxy_delete, csrf))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let unbound = storage
        .create_outbound_proxy("unbound", "http://127.0.0.1:1082")
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(post(&format!("/admin/proxies/{}/delete", unbound.id), csrf))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(storage.require_outbound_proxy(&unbound.id).await.is_err());
    let response = app.oneshot(post(&account_delete, csrf)).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(storage.get_account(&first.id).await.unwrap().is_none());
    assert!(
        storage
            .load_account_tokens(&first.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .lookup_proxy_api_key(&first_key.token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .get_account_quota(&first.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .get_oauth_pending("first-pending")
            .await
            .unwrap()
            .is_none()
    );
    assert!(state.auth.account_http(&first.id).await.is_err());
    assert!(state.upstream.get(&first.id).await.is_err());
    assert!(
        storage
            .lookup_proxy_api_key(&second_key.token)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        storage
            .get_oauth_pending("second-pending")
            .await
            .unwrap()
            .is_some()
    );
    let history = storage.query_usage(&Default::default()).await.unwrap();
    assert_eq!(history.total, 1);
    assert_eq!(history.records[0].account_name, "first account");
    storage.close().await;
    let reopened = Storage::open(&path).await.unwrap();
    assert!(reopened.get_account(&first.id).await.unwrap().is_none());
    assert!(
        reopened
            .require_account(&second.id)
            .await
            .unwrap()
            .proxy_id
            .is_none()
    );
    assert_eq!(reopened.list_outbound_proxies().await.unwrap().len(), 1);
    reopened.close().await;
}
