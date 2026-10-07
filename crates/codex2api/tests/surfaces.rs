use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn independent_routers_do_not_register_other_surfaces_or_accept_user_tokens_as_admin() {
    let temp = tempfile::tempdir().unwrap();
    let storage = codex2api_storage::Storage::open(temp.path().join("surfaces.sqlite"))
        .await
        .unwrap();
    let apps = codex2api::routers(
        storage.clone(),
        "http://127.0.0.1:8080",
        "http://127.0.0.1:8082",
        "http://127.0.0.1:8081",
    )
    .unwrap();
    for (app, paths) in [
        (
            &apps.api,
            vec![
                "/admin/",
                "/admin/api/users",
                "/user/",
                "/user/api/session",
                "/api/oauth/chatgpt/oauth/authorize/bootstrap",
            ],
        ),
        (
            &apps.user,
            vec![
                "/admin/",
                "/admin/api/suppliers",
                "/v1/responses",
                "/api/oauth/chatgpt/oauth/token",
            ],
        ),
        (
            &apps.admin,
            vec![
                "/user/",
                "/user/api/session",
                "/v1/responses",
                "/admin/authorize/",
                "/admin/device/",
            ],
        ),
    ] {
        for path in paths {
            let response = app
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert!(
                matches!(
                    response.status(),
                    StatusCode::NOT_FOUND | StatusCode::UNAUTHORIZED
                ) || (path == "/api/oauth/chatgpt/oauth/authorize/bootstrap"
                    && response.status() == StatusCode::NOT_IMPLEMENTED),
                "{path}: {}",
                response.status()
            );
        }
    }
    #[cfg(not(feature = "dev-frontend"))]
    for (app, path) in [
        (&apps.admin, "/admin/"),
        (&apps.user, "/user/"),
        (&apps.user, "/user/authorize/"),
        (&apps.user, "/user/device/"),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
    let response = apps
        .admin
        .clone()
        .oneshot(
            Request::post("/admin/api/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username":"admin","password":"admin"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let admin_cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let admin_token = admin_cookie.split_once('=').unwrap().1;
    let user = codex2api_storage::User {
        kind: codex2api_storage::UserKind::Regular,
        id: "surface-user".into(),
        username: "surface-user".into(),
        password_hash: codex2api_storage::hash_password("user-password").unwrap(),
        name: "Surface user".into(),
        email: "surface@example.test".into(),
        enabled: true,
        wallet_cents: 0,
        revision: 1,
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    storage.save_user(&user, None).await.unwrap();
    let user_token = storage
        .create_user_session(&user, "csrf")
        .await
        .unwrap()
        .unwrap();
    let user_cookie = format!("c2a_user_session={user_token}");
    let platform = storage
        .user_platform_account(&user.id, "chatgpt")
        .await
        .unwrap()
        .unwrap();
    storage
        .create_virtual_device(&platform, "client-refresh", &Default::default())
        .await
        .unwrap()
        .unwrap();
    let response = apps.api.clone().oneshot(Request::post("/api/oauth/chatgpt/oauth/token")
        .header("content-type","application/json")
        .body(Body::from(json!({"grant_type":"refresh_token","refresh_token":"client-refresh","client_id":codex2api_version::OAUTH_CLIENT_ID}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let client_tokens: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 100000).await.unwrap()).unwrap();
    let access_token = client_tokens["access_token"].as_str().unwrap();
    let id_token = client_tokens["id_token"].as_str().unwrap();
    let refresh_token = client_tokens["refresh_token"].as_str().unwrap();
    for (app, path, own_cookie, own_token, foreign) in [
        (
            &apps.admin,
            "/admin/api/session",
            &admin_cookie,
            admin_token,
            vec![user_token.as_str(), access_token, id_token, refresh_token],
        ),
        (
            &apps.user,
            "/user/api/session",
            &user_cookie,
            user_token.as_str(),
            vec![admin_token, access_token, id_token, refresh_token],
        ),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(
                    Request::get(path)
                        .header("authorization", format!("Bearer {own_token}"))
                        .body(Body::empty())
                        .unwrap()
                )
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        for token in foreign {
            for cookie in [None, Some(own_cookie)] {
                let mut request =
                    Request::get(path).header("authorization", format!("Bearer {token}"));
                if let Some(cookie) = cookie {
                    request = request.header("cookie", cookie);
                }
                assert_eq!(
                    app.clone()
                        .oneshot(request.body(Body::empty()).unwrap())
                        .await
                        .unwrap()
                        .status(),
                    StatusCode::UNAUTHORIZED
                );
            }
        }
    }
    for token in [admin_token, user_token.as_str(), id_token, refresh_token] {
        let response = apps
            .api
            .clone()
            .oneshot(
                Request::get("/api/oauth/chatgpt/backend-api/wham/profiles/me")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    for path in [
        "/admin/api/session",
        "/admin/api/suppliers",
        "/admin/api/users",
        "/admin/api/subscriptions",
    ] {
        let response = apps
            .admin
            .clone()
            .oneshot(
                Request::get(path)
                    .header("authorization", "Bearer user-token")
                    .header("cookie", &admin_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let response = apps
        .user
        .oneshot(
            Request::post("/user/api/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username":"admin","password":"admin"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 10000).await.unwrap()).unwrap();
    assert_eq!(value["error"]["code"], "invalid_credentials");
    storage.close().await;
}
