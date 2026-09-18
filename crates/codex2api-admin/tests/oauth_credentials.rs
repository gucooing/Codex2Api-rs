use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use codex2api_storage::{AccountStatus, AccountTokens, AccountUpdate, Storage};
use tower::ServiceExt;

#[tokio::test]
async fn oauth_management_requires_admin_csrf_and_never_embeds_refresh_tokens() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("oauth-admin.sqlite"))
        .await
        .unwrap();
    let state = codex2api_admin::AdminState::new(storage.clone()).unwrap();
    let account = state.accounts.create_pending().await.unwrap().account;
    let pending = state.accounts.create_pending().await.unwrap().account;
    storage
        .update_account(
            &account.id,
            AccountUpdate {
                status: Some(AccountStatus::Active),
                chatgpt_account_id: Some("official-workspace".into()),
                email: Some("fixture@example.test".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    storage
        .upsert_account_tokens(AccountTokens {
            account_id: account.id.clone(),
            access_token: Some("official-secret-access".into()),
            refresh_token: Some("official-secret-refresh".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let app = codex2api_admin::router(state);
    let root = "/admin/oauth/credentials";
    for (method, path) in [
        ("GET", root),
        ("POST", root),
        ("POST", "/admin/oauth/credentials/id/copy"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()["location"], "/admin/login");
    }
    let session = storage
        .login_admin("admin", "admin", std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let cookie = format!("{}={}", codex2api_admin::SESSION_COOKIE, session.id);
    let get = || {
        Request::get(root)
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap()
    };
    let response = app.clone().oneshot(get()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let html = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(html.contains("class=\"main-tab active\" href=\"/admin/oauth/credentials\""));
    assert!(html.contains("/api/oauth/chatgpt/oauth/token"));
    assert!(!html.contains(&format!("<option value=\"{}\">", pending.id)));
    let csrf = html
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let post = |path: &str, fields: &[(&str, &str)]| {
        Request::post(path)
            .header("cookie", &cookie)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(
                url::form_urlencoded::Serializer::new(String::new())
                    .extend_pairs(fields.iter().copied())
                    .finish(),
            ))
            .unwrap()
    };
    let response = app
        .clone()
        .oneshot(post(
            root,
            &[
                ("csrf", "bad"),
                ("name", "client"),
                ("account_id", &account.id),
            ],
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(storage.list_oauth_credentials().await.unwrap().is_empty());
    for id in [&pending.id, "missing"] {
        let response = app
            .clone()
            .oneshot(post(
                root,
                &[("csrf", csrf), ("name", "client"), ("account_id", id)],
            ))
            .await
            .unwrap();
        assert!(
            response.headers()["location"]
                .to_str()
                .unwrap()
                .contains("?err=")
        );
    }
    let response = app
        .clone()
        .oneshot(post(
            root,
            &[
                ("csrf", csrf),
                ("name", "<client>"),
                ("account_id", &account.id),
            ],
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let credential = storage.list_oauth_credentials().await.unwrap().remove(0);
    let rt = storage
        .oauth_refresh_token(&credential.id)
        .await
        .unwrap()
        .unwrap();
    let other_rt = storage
        .create_oauth_credential(&account.id, "client-two")
        .await
        .unwrap();
    let device =
        codex2api_storage::OAuthDeviceIdentity::new(Some("device-one"), "<Desktop Client>");
    storage
        .register_oauth_access(
            &credential,
            &rt,
            "device-access",
            chrono::Utc::now().timestamp() + 3600,
            &device,
        )
        .await
        .unwrap();
    storage
        .touch_oauth_access(&codex2api_storage::hash_api_key("device-access"))
        .await
        .unwrap();
    let response = app.clone().oneshot(get()).await.unwrap();
    let html = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    let detail = format!("/admin/oauth/accounts/{}", account.id);
    assert!(html.contains(&format!("href=\"{detail}\"")));
    assert!(html.contains(&format!("action=\"{detail}/delete\"")));
    assert!(html.contains("<td>2</td><td>1</td>"));
    assert!(!html.contains(&rt));
    let response = app
        .clone()
        .oneshot(Request::get(&detail).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
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
    let html = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(html.contains("&lt;client&gt;"));
    assert!(html.contains("client-two"));
    assert!(html.contains("device-one"));
    assert!(html.contains("&lt;Desktop Client&gt;"));
    assert!(html.contains("最近登录"));
    assert!(html.contains("最近使用"));
    assert!(html.contains("data-oauth-time"));
    assert!(html.contains("data-key-copy="));
    assert!(!html.contains(&rt));
    assert!(!html.contains("official-secret"));
    assert!(!html.contains(&codex2api_storage::hash_api_key(&rt)));
    for action in ["copy", "pause", "enable", "delete"] {
        let path = format!("{root}/{}/{action}", credential.id);
        let response = app
            .clone()
            .oneshot(post(&path, &[("csrf", "bad")]))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let response = app
            .clone()
            .oneshot(post(&path, &[("csrf", csrf)]))
            .await
            .unwrap();
        if action == "copy" {
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["cache-control"], "no-store");
            let value: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                    .unwrap();
            assert_eq!(value["token"], rt);
        } else {
            assert_eq!(response.status(), StatusCode::SEE_OTHER);
            assert_eq!(
                storage.lookup_oauth_refresh(&rt).await.unwrap().is_some(),
                action == "enable"
            );
        }
    }
    assert_eq!(storage.list_oauth_credentials().await.unwrap().len(), 1);
    assert_eq!(
        storage.list_oauth_credentials().await.unwrap()[0].id,
        other_rt.id
    );
    assert_eq!(
        storage
            .load_account_tokens(&account.id)
            .await
            .unwrap()
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("official-secret-refresh")
    );
    let key = storage
        .create_proxy_api_key(&account.id, Some("keep-key"))
        .await
        .unwrap();
    let third = storage
        .create_oauth_credential(&account.id, "third-rt")
        .await
        .unwrap();
    let third_token = storage
        .oauth_refresh_token(&third.id)
        .await
        .unwrap()
        .unwrap();
    storage
        .register_oauth_access(
            &third,
            &third_token,
            "third-access",
            chrono::Utc::now().timestamp() + 3600,
            &device,
        )
        .await
        .unwrap();
    let deletion = format!("{detail}/delete");
    let response = app
        .clone()
        .oneshot(post(&deletion, &[("csrf", "wrong")]))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        storage
            .oauth_credentials_for_account(&account.id)
            .await
            .unwrap()
            .len(),
        2
    );
    let response = app
        .clone()
        .oneshot(post(&deletion, &[("csrf", csrf)]))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        storage
            .oauth_credentials_for_account(&account.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        storage
            .oauth_devices_for_account(&account.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        storage
            .lookup_oauth_refresh(&third_token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .lookup_oauth_access_hash(&codex2api_storage::hash_api_key("third-access"))
            .await
            .unwrap()
            .is_none()
    );
    assert!(storage.get_account(&account.id).await.unwrap().is_some());
    assert!(
        storage
            .lookup_proxy_api_key(&key.token)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        storage
            .load_account_tokens(&account.id)
            .await
            .unwrap()
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("official-secret-refresh")
    );
    storage.close().await;
}
