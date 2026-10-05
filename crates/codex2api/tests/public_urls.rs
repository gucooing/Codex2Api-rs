use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    response::Response,
};
use codex2api_storage::Storage;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn send(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> Response {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:8080")
        .header("x-forwarded-host", "untrusted.example")
        .header("x-forwarded-proto", "http")
        .header("forwarded", "host=untrusted.example;proto=https")
        .header("content-type", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    app.clone()
        .oneshot(
            request
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn payload(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}

fn query(client: &str, callback: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("response_type", "code"),
            ("client_id", client),
            ("redirect_uri", callback),
            ("state", "state+must/stay=exact"),
            ("nonce", "fixture-nonce"),
            ("code_challenge", &"A".repeat(43)),
            ("code_challenge_method", "S256"),
            ("scope", "openid profile email offline_access"),
        ])
        .finish()
}

fn apps(storage: Storage) -> codex2api::ApplicationRouters {
    codex2api::routers(
        storage,
        "http://127.0.0.1:8080",
        "http://127.0.0.1:8082",
        "http://127.0.0.1:8081",
    )
    .unwrap()
}

#[tokio::test]
async fn administrator_addresses_apply_to_existing_routers_and_survive_restart() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("public-urls.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let routers = apps(storage.clone());
    let login = send(
        &routers.admin,
        "POST",
        "/admin/api/login",
        json!({"username":"admin","password":"admin"}),
        &[],
    )
    .await;
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let login = payload(login).await;
    let admin_headers = [
        ("cookie", cookie.as_str()),
        ("x-csrf-token", login["csrf_token"].as_str().unwrap()),
    ];
    let initial = payload(
        send(
            &routers.admin,
            "GET",
            "/admin/api/settings/public-urls",
            Value::Null,
            &admin_headers,
        )
        .await,
    )
    .await;
    assert_eq!(
        initial,
        json!({"api_url":"http://127.0.0.1:8080", "user_url":"http://127.0.0.1:8082", "admin_url":"http://127.0.0.1:8081", "revision":0})
    );

    let grok_query = query(
        codex2api_version::grok::CLIENT_ID,
        "http://127.0.0.1:43210/callback",
    );
    let codex_query = query(
        codex2api_version::OAUTH_CLIENT_ID,
        "http://localhost:1455/auth/callback",
    );
    for (revision, api_url, user_url, admin_url) in [
        (
            0,
            "https://api.example.test:9443",
            "http://users.example.test:9082",
            "https://admin.example.test:9441",
        ),
        (
            1,
            "http://api.example.test:9080",
            "https://users.example.test:9442",
            "http://admin.example.test:9081",
        ),
    ] {
        let settings = json!({"api_url":api_url,"user_url":user_url,"admin_url":admin_url,"revision":revision});
        let saved = send(
            &routers.admin,
            "PUT",
            "/admin/api/settings/public-urls",
            settings.clone(),
            &admin_headers,
        )
        .await;
        assert_eq!(saved.status(), StatusCode::OK);
        let saved = payload(saved).await;
        assert_eq!(saved["revision"], revision + 1);
        assert_eq!(
            payload(
                send(
                    &routers.admin,
                    "GET",
                    "/admin/api/settings/public-urls",
                    Value::Null,
                    &admin_headers
                )
                .await
            )
            .await,
            saved
        );
        assert_eq!(
            send(
                &routers.admin,
                "PUT",
                "/admin/api/settings/public-urls",
                settings,
                &admin_headers
            )
            .await
            .status(),
            StatusCode::CONFLICT
        );

        for root in ["/api/oauth/grok", "/grok"] {
            let discovery = send(
                &routers.api,
                "GET",
                &format!("{root}/.well-known/openid-configuration"),
                Value::Null,
                &[],
            )
            .await;
            assert_eq!(discovery.status(), StatusCode::OK);
            assert_eq!(discovery.headers()["cache-control"], "no-store");
            let discovery = payload(discovery).await;
            for (field, suffix) in [
                ("issuer", ""),
                ("authorization_endpoint", "/oauth2/authorize"),
                ("token_endpoint", "/oauth2/token"),
                ("revocation_endpoint", "/oauth2/revoke"),
                ("device_authorization_endpoint", "/oauth2/device/code"),
                ("userinfo_endpoint", "/oauth2/userinfo"),
                ("jwks_uri", "/.well-known/jwks.json"),
            ] {
                assert_eq!(discovery[field], format!("{api_url}{root}{suffix}"));
            }
            let redirect = send(
                &routers.api,
                "GET",
                &format!("{root}/oauth2/authorize?{grok_query}"),
                Value::Null,
                &[],
            )
            .await;
            assert_eq!(redirect.status(), StatusCode::SEE_OTHER);
            assert_eq!(
                redirect.headers()["location"],
                format!("{user_url}/user/authorize/?{grok_query}")
            );
            let device = payload(
                send(
                    &routers.api,
                    "POST",
                    &format!("{root}/oauth2/device/code"),
                    json!({"client_id":codex2api_version::grok::CLIENT_ID}),
                    &[],
                )
                .await,
            )
            .await;
            assert_eq!(
                device["verification_uri"],
                format!("{user_url}/user/device/?provider=grok")
            );
            assert_eq!(
                device["verification_uri_complete"],
                format!(
                    "{user_url}/user/device/?provider=grok&user_code={}",
                    device["user_code"].as_str().unwrap()
                )
            );
        }
        let redirect = send(
            &routers.api,
            "GET",
            &format!("/api/oauth/chatgpt/oauth/authorize?{codex_query}"),
            Value::Null,
            &[],
        )
        .await;
        assert_eq!(redirect.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            redirect.headers()["location"],
            format!("{user_url}/user/authorize/?{codex_query}")
        );
        let desktop = url::form_urlencoded::Serializer::new(String::new())
            .append_pair(
                "authorize_url",
                &format!("{api_url}/api/oauth/chatgpt/oauth/authorize?{codex_query}"),
            )
            .finish();
        let redirect = send(
            &routers.api,
            "GET",
            &format!("/codex/desktop-auth?{desktop}"),
            Value::Null,
            &[],
        )
        .await;
        assert_eq!(redirect.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            redirect.headers()["location"],
            format!("{user_url}/user/authorize/?{codex_query}")
        );
        let redirect = send(
            &routers.api,
            "GET",
            "/api/oauth/chatgpt/codex/device",
            Value::Null,
            &[],
        )
        .await;
        assert_eq!(
            redirect.headers()["location"],
            format!("{user_url}/user/device/")
        );
        let metadata = payload(
            send(
                &routers.api,
                "GET",
                "/api/oauth/chatgpt/backend-api/ps/mcp/.well-known/oauth-protected-resource",
                Value::Null,
                &[],
            )
            .await,
        )
        .await;
        assert_eq!(
            metadata["resource"],
            format!("{api_url}/api/oauth/chatgpt/backend-api/ps/mcp")
        );

        let bootstrap = send(
            &routers.user,
            "GET",
            &format!("/user/api/oauth/authorize/bootstrap?{grok_query}"),
            Value::Null,
            &[],
        )
        .await;
        assert_eq!(bootstrap.status(), StatusCode::OK);
        assert_eq!(
            bootstrap.headers()["set-cookie"]
                .to_str()
                .unwrap()
                .contains("; Secure"),
            user_url.starts_with("https://")
        );
        for (origin, expected) in [
            (user_url, StatusCode::UNAUTHORIZED),
            ("https://untrusted.example", StatusCode::FORBIDDEN),
        ] {
            let response = send(
                &routers.user,
                "POST",
                "/user/api/login",
                json!({"username":"missing","password":"fixture-password"}),
                &[("origin", origin)],
            )
            .await;
            assert_eq!(response.status(), expected);
        }
        let login = send(
            &routers.admin,
            "POST",
            "/admin/api/login",
            json!({"username":"admin","password":"admin"}),
            &[],
        )
        .await;
        assert_eq!(login.status(), StatusCode::OK);
        assert_eq!(
            login.headers()["set-cookie"]
                .to_str()
                .unwrap()
                .contains("; Secure"),
            admin_url.starts_with("https://")
        );
    }
    drop(routers);
    storage.close().await;
    let reopened = Storage::open(path).await.unwrap();
    let routers = apps(reopened);
    let discovery = payload(
        send(
            &routers.api,
            "GET",
            "/api/oauth/grok/.well-known/openid-configuration",
            Value::Null,
            &[],
        )
        .await,
    )
    .await;
    assert_eq!(
        discovery["issuer"],
        "http://api.example.test:9080/api/oauth/grok"
    );
}

#[tokio::test]
async fn public_addresses_require_admin_csrf_and_atomic_valid_settings() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("validation.sqlite"))
        .await
        .unwrap();
    let routers = apps(storage.clone());
    let settings = json!({"api_url":"https://api.example.test/","user_url":"http://user.example.test:8082/","admin_url":"https://admin.example.test/","revision":0});
    assert_eq!(
        send(
            &routers.admin,
            "PUT",
            "/admin/api/settings/public-urls",
            settings.clone(),
            &[]
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let login = send(
        &routers.admin,
        "POST",
        "/admin/api/login",
        json!({"username":"admin","password":"admin"}),
        &[],
    )
    .await;
    let cookie = login.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let login = payload(login).await;
    let headers = [
        ("cookie", cookie.as_str()),
        ("x-csrf-token", login["csrf_token"].as_str().unwrap()),
    ];
    assert_eq!(
        send(
            &routers.admin,
            "PUT",
            "/admin/api/settings/public-urls",
            settings.clone(),
            &headers[..1]
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    for field in ["api_url", "user_url", "admin_url"] {
        for invalid in [
            "",
            "api.example.test",
            "//api.example.test",
            "ftp://api.example.test",
            "https://api.example.test/user",
            "https://name:secret@api.example.test",
            "https://api.example.test?",
            "https://api.example.test#",
            "https://api.example.test\n",
            "https://api.example.test\\user",
        ] {
            let mut bad = settings.clone();
            // Surrounding whitespace is intentionally trimmed, so use an embedded newline.
            bad[field] = if invalid.ends_with('\n') {
                "https://api.exa\nmple.test"
            } else {
                invalid
            }
            .into();
            assert_eq!(
                send(
                    &routers.admin,
                    "PUT",
                    "/admin/api/settings/public-urls",
                    bad,
                    &headers
                )
                .await
                .status(),
                StatusCode::BAD_REQUEST,
                "{field}: {invalid:?}"
            );
            assert!(storage.public_url_settings().await.unwrap().is_none());
        }
    }
    let saved = payload(
        send(
            &routers.admin,
            "PUT",
            "/admin/api/settings/public-urls",
            settings,
            &headers,
        )
        .await,
    )
    .await;
    assert_eq!(saved["api_url"], "https://api.example.test");
    assert_eq!(saved["user_url"], "http://user.example.test:8082");
    assert_eq!(saved["admin_url"], "https://admin.example.test");
}
