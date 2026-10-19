use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    response::Response,
};
use codex2api_storage::{Storage, User, UserKind};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn send(
    app: &Router,
    method: &str,
    path: &str,
    value: Value,
    headers: &[(&str, &str)],
) -> Response {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    app.clone()
        .oneshot(
            request
                .body(if value.is_null() {
                    Body::empty()
                } else {
                    Body::from(value.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap()
}
async fn body(response: Response) -> Value {
    serde_json::from_slice(
        &to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}
async fn user(storage: &Storage, id: &str) -> String {
    let user = User {
        id: id.into(),
        kind: UserKind::Regular,
        username: id.into(),
        name: id.into(),
        email: format!("{id}@example.test"),
        password_hash: codex2api_storage::hash_password("test-password").unwrap(),
        enabled: true,
        wallet_cents: 0,
        revision: 1,
        created_at: "2026-10-12T00:00:00Z".into(),
    };
    assert!(storage.save_user(&user, None).await.unwrap());
    storage
        .create_user_session(&user, "user-csrf")
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn api_management_keys_and_four_listeners_keep_their_ownership_boundaries() {
    let directory = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(directory.path().join("metered-pg"))
        .await
        .unwrap();
    let apps = codex2api::routers(
        storage.clone(),
        "http://localhost:8080",
        "http://localhost:8082",
        "http://localhost:8081",
        "https://models.example.test",
    )
    .unwrap();
    for (app, paths) in [
        (
            &apps.api,
            vec![
                "/admin/api/api-groups",
                "/user/api/api-keys",
                "/v1/chatgpt/oauth/token",
            ],
        ),
        (
            &apps.oauth,
            vec!["/admin/api/api-groups", "/user/api/api-keys"],
        ),
        (
            &apps.user,
            vec![
                "/admin/api/api-groups",
                "/v1/chat/completions",
                "/v1/images/generations",
            ],
        ),
        (
            &apps.admin,
            vec![
                "/user/api/api-keys",
                "/v1/chat/completions",
                "/v1/images/generations",
            ],
        ),
    ] {
        for path in paths {
            assert!(
                matches!(
                    send(app, "GET", path, Value::Null, &[]).await.status(),
                    StatusCode::NOT_FOUND | StatusCode::UNAUTHORIZED
                ),
                "{path}"
            );
        }
    }
    assert_eq!(
        send(&apps.api, "GET", "/healthz", Value::Null, &[])
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(&apps.api, "GET", "/v1/models", Value::Null, &[])
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let login = send(
        &apps.admin,
        "POST",
        "/admin/api/login",
        json!({"username":"admin","password":"admin"}),
        &[],
    )
    .await;
    assert_eq!(login.status(), StatusCode::OK);
    let admin_cookie = login.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let login = body(login).await;
    let admin = [
        ("cookie", admin_cookie.as_str()),
        ("x-csrf-token", login["csrf_token"].as_str().unwrap()),
    ];
    let alice = user(&storage, "api-alice").await;
    let bob = user(&storage, "api-bob").await;
    let alice_auth = format!("Bearer {alice}");
    let bob_auth = format!("Bearer {bob}");
    let alice_headers = [
        ("authorization", alice_auth.as_str()),
        ("x-csrf-token", "user-csrf"),
    ];
    let bob_headers = [
        ("authorization", bob_auth.as_str()),
        ("x-csrf-token", "user-csrf"),
    ];
    let mut groups = vec![];
    for provider in ["chatgpt", "grok"] {
        let tag = format!("api-tag-{provider}");
        storage
            .save_supplier_tag(&tag, provider, "Private supplier pool")
            .await
            .unwrap();
        let input = json!({"name":format!("{provider} API"),"description":"Public API description","enabled":true,"tag_ids":[tag],"models":[{"model":"gpt-6-astra","multiplier":null,"concurrency_limit":0,"rpm_limit":0}],"multiplier":"1.25","concurrency_limit":2,"rpm_limit":30});
        assert_eq!(
            send(
                &apps.admin,
                "POST",
                "/admin/api/api-groups",
                input.clone(),
                &admin[..1]
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
        let response = send(&apps.admin, "POST", "/admin/api/api-groups", input, &admin).await;
        assert_eq!(response.status(), StatusCode::OK);
        groups.push(body(response).await);
    }
    let listed = body(
        send(
            &apps.admin,
            "GET",
            "/admin/api/api-groups?search=grok",
            Value::Null,
            &admin,
        )
        .await,
    )
    .await;
    assert_eq!(listed["total"], 1);
    assert_eq!(listed["items"][0]["name"], "grok API");
    let public = body(
        send(
            &apps.user,
            "GET",
            "/user/api/api-groups",
            Value::Null,
            &alice_headers,
        )
        .await,
    )
    .await;
    assert_eq!(public["api_base_url"], "https://models.example.test/v1");
    assert_eq!(public["items"].as_array().unwrap().len(), 2);
    for group in public["items"].as_array().unwrap() {
        assert!(group.get("tag_ids").is_none());
        assert_eq!(group["multiplier"], "1.25");
    }
    let group_id = groups[0]["id"].as_str().unwrap();
    assert_eq!(
        send(
            &apps.user,
            "POST",
            "/user/api/api-keys",
            json!({"name":"Alice API","group_id":group_id}),
            &alice_headers[..1]
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let created = send(
        &apps.user,
        "POST",
        "/user/api/api-keys",
        json!({"name":"Alice API","group_id":group_id}),
        &alice_headers,
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let created = body(created).await;
    let id = created["key"]["id"].as_str().unwrap();
    let secret = created["secret"].as_str().unwrap();
    let key_auth = format!("Bearer {secret}");
    let listed = body(
        send(
            &apps.user,
            "GET",
            "/user/api/api-keys",
            Value::Null,
            &alice_headers,
        )
        .await,
    )
    .await;
    assert_eq!(listed["total"], 1);
    assert_eq!(listed["items"][0]["active_requests"], 0);
    assert!(!listed.to_string().contains(secret));
    for name in ["token_hash", "user_id", "username"] {
        assert!(listed["items"][0].get(name).is_none());
    }
    let bob_list = body(
        send(
            &apps.user,
            "GET",
            "/user/api/api-keys?user_id=api-alice",
            Value::Null,
            &bob_headers,
        )
        .await,
    )
    .await;
    assert_eq!(bob_list["total"], 0);
    assert_ne!(
        send(
            &apps.user,
            "POST",
            &format!("/user/api/api-keys/{id}/revoke"),
            json!({"revision":1}),
            &bob_headers
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            &apps.api,
            "GET",
            "/v1/models",
            Value::Null,
            &[("authorization", key_auth.as_str())]
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(&apps.api, "GET", "/v1/models", Value::Null, &alice_headers)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &apps.user,
            "GET",
            "/user/api/api-keys",
            Value::Null,
            &[("authorization", key_auth.as_str())]
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &apps.oauth,
            "POST",
            "/v1/chatgpt/backend-api/codex/responses",
            json!({"model":"any","input":"hello"}),
            &[("authorization", key_auth.as_str())]
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let inspected = body(
        send(
            &apps.admin,
            "GET",
            "/admin/api/api-keys?user_id=api-alice",
            Value::Null,
            &admin,
        )
        .await,
    )
    .await;
    assert_eq!(inspected["items"][0]["username"], "api-alice");
    assert_eq!(inspected["items"][0]["active_requests"], 0);
    assert!(!inspected.to_string().contains(secret));
    assert_eq!(
        send(
            &apps.user,
            "POST",
            &format!("/user/api/api-keys/{id}/revoke"),
            json!({"revision":1}),
            &alice_headers
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            &apps.api,
            "GET",
            "/v1/models",
            Value::Null,
            &[("authorization", key_auth.as_str())]
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let delete = format!("/admin/api/api-groups/{group_id}");
    assert_eq!(
        send(
            &apps.admin,
            "DELETE",
            &delete,
            json!({"revision":999}),
            &admin
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(
            &apps.admin,
            "DELETE",
            &delete,
            json!({"revision":1}),
            &admin
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_ne!(
        send(
            &apps.user,
            "POST",
            "/user/api/api-keys",
            json!({"name":"Deleted group","group_id":group_id}),
            &alice_headers
        )
        .await
        .status(),
        StatusCode::OK
    );
    storage.close().await;
}
