use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use codex2api_storage::{AccountStatus, Storage};
use tower::ServiceExt;

#[tokio::test]
async fn key_management_requires_csrf_and_keeps_plaintext_out_of_the_list() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("keys.sqlite")).await.unwrap();
    let state = codex2api_admin::AdminState::new(storage.clone()).unwrap();
    let id = state.accounts.create_pending().await.unwrap().account.id;
    storage
        .set_account_status(&id, AccountStatus::Active)
        .await
        .unwrap();
    let other_id = state.accounts.create_pending().await.unwrap().account.id;
    storage
        .set_account_status(&other_id, AccountStatus::Active)
        .await
        .unwrap();
    let pending_id = state.accounts.create_pending().await.unwrap().account.id;
    let existing = storage
        .create_proxy_api_key(&other_id, Some("existing-key"))
        .await
        .unwrap();
    let session = storage
        .login_admin("admin", "admin", std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let cookie = format!("{}={}", codex2api_admin::SESSION_COOKIE, session.id);
    let app = codex2api_admin::router(state);
    let path = "/admin/keys";
    let page = "/admin/keys";
    let response = app
        .clone()
        .oneshot(Request::get(page).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let response = app
        .clone()
        .oneshot(
            Request::get(page)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    assert!(html.contains("<dialog"));
    assert!(html.contains("data-key-cancel"));
    assert!(!html.contains("撤销"));
    assert!(html.contains("绑定账户"));
    assert!(html.contains("existing-key"));
    assert!(html.contains(&format!("<option value=\"{other_id}\">")));
    assert!(!html.contains(&format!("<option value=\"{pending_id}\">")));
    assert!(html.contains("class=\"main-tab active\" href=\"/admin/keys\""));
    let csrf = html
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    assert!(storage.list_proxy_api_keys(&id).await.unwrap().is_empty());
    let post = |path: &str, body: String| {
        Request::post(path)
            .header("cookie", &cookie)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(body))
            .unwrap()
    };
    let response = app
        .clone()
        .oneshot(post(path, format!("csrf=bad&name=test&account_id={id}")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(storage.list_proxy_api_keys(&id).await.unwrap().is_empty());
    let response = app
        .clone()
        .oneshot(post(path, format!("csrf={csrf}&name=test&account_id={id}")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let key = storage.list_proxy_api_keys(&id).await.unwrap().remove(0);
    let token = storage
        .proxy_api_key_token(&id, &key.id)
        .await
        .unwrap()
        .unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::get(page)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    assert!(!html.contains(&token));
    assert!(!html.contains(&key.key_hash));
    let bind_path = format!("{path}/{}/bind", key.id);
    let response = app
        .clone()
        .oneshot(post(&bind_path, format!("csrf=bad&account_id={other_id}")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    for target in [pending_id.as_str(), "missing-account"] {
        let response = app
            .clone()
            .oneshot(post(&bind_path, format!("csrf={csrf}&account_id={target}")))
            .await
            .unwrap();
        assert!(
            response.headers()["location"]
                .to_str()
                .unwrap()
                .contains("?err=")
        );
        assert_eq!(
            storage
                .get_proxy_api_key(&key.id)
                .await
                .unwrap()
                .unwrap()
                .account_id,
            id
        );
    }
    let response = app
        .clone()
        .oneshot(post(
            &bind_path,
            format!("csrf={csrf}&account_id={other_id}"),
        ))
        .await
        .unwrap();
    assert!(
        response.headers()["location"]
            .to_str()
            .unwrap()
            .contains("?ok=")
    );
    assert_eq!(
        storage
            .lookup_proxy_api_key(&token)
            .await
            .unwrap()
            .unwrap()
            .account_id,
        other_id
    );
    assert_eq!(
        storage
            .proxy_api_key_token(&other_id, &key.id)
            .await
            .unwrap(),
        Some(token.clone())
    );
    assert!(storage.list_proxy_api_keys(&id).await.unwrap().is_empty());
    for (action, available) in [("pause", false), ("enable", true)] {
        let response = app
            .clone()
            .oneshot(post(
                &format!("{path}/{}/{action}", key.id),
                format!("csrf={csrf}"),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            storage
                .lookup_proxy_api_key(&token)
                .await
                .unwrap()
                .is_some(),
            available
        );
    }
    let response = app
        .clone()
        .oneshot(post(
            &format!("{path}/{}/copy", key.id),
            format!("csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["token"],
        token
    );
    let response = app
        .clone()
        .oneshot(post(
            &format!("{path}/{}/delete", key.id),
            format!("csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(storage.list_proxy_api_keys(&id).await.unwrap().is_empty());
    assert!(storage.get_proxy_api_key(&key.id).await.unwrap().is_none());
    assert_eq!(storage.list_all_proxy_api_keys().await.unwrap().len(), 1);
    assert!(
        storage
            .lookup_proxy_api_key(&existing.token)
            .await
            .unwrap()
            .is_some()
    );
    let response = app
        .clone()
        .oneshot(
            Request::get(format!("/admin/accounts/{id}?tab=keys"))
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = app
        .oneshot(post(
            &format!("{path}/{}/copy", key.id),
            format!("csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    storage.close().await;
}
