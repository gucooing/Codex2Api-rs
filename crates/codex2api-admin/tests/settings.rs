use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use codex2api_storage::Storage;
use tower::ServiceExt;

#[tokio::test]
async fn settings_verify_both_old_credentials_require_csrf_and_expire_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let app = codex2api_admin::router(codex2api_admin::AdminState::new(storage.clone()).unwrap());
    for method in ["GET", "POST"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri("/admin/settings")
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
    let other_session = storage
        .login_admin("admin", "admin", std::time::Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let cookie = format!("{}={}", codex2api_admin::SESSION_COOKIE, session.id);
    let get = || {
        Request::get("/admin/settings")
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
    assert!(html.contains("data-admin-menu"));
    assert!(html.contains("href=\"/admin/settings\""));
    assert!(html.contains("action=\"/admin/logout\""));
    let csrf = html
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let post = |csrf: &str,
                old_username: &str,
                old_password: &str,
                new_username: &str,
                new_password: &str| {
        let form = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs([
                ("csrf", csrf),
                ("old_username", old_username),
                ("old_password", old_password),
                ("new_username", new_username),
                ("new_password", new_password),
            ])
            .finish();
        Request::post("/admin/settings")
            .header("cookie", &cookie)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(form))
            .unwrap()
    };
    for (token, username, password, new_username, new_password, status, message) in [
        (
            "wrong-csrf",
            "admin",
            "admin",
            "operator",
            "new-secret-fixture",
            StatusCode::FORBIDDEN,
            "请刷新页面后重试",
        ),
        (
            csrf,
            "wrong-user",
            "admin",
            "operator",
            "new-secret-fixture",
            StatusCode::BAD_REQUEST,
            "原用户名或原密码错误",
        ),
        (
            csrf,
            "admin",
            "wrong-password-fixture",
            "<operator>",
            "new-secret-fixture",
            StatusCode::BAD_REQUEST,
            "原用户名或原密码错误",
        ),
        (
            csrf,
            "admin",
            "",
            "operator",
            "new-secret-fixture",
            StatusCode::BAD_REQUEST,
            "原用户名或原密码错误",
        ),
        (
            csrf,
            "admin",
            "admin",
            "   ",
            "new-secret-fixture",
            StatusCode::BAD_REQUEST,
            "新用户名不能为空",
        ),
        (
            csrf,
            "admin",
            "admin",
            "admin",
            "",
            StatusCode::BAD_REQUEST,
            "请输入新的用户名或密码",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(post(token, username, password, new_username, new_password))
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let html = String::from_utf8(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(html.contains(message));
        assert!(!html.contains("wrong-password-fixture"));
        assert!(!html.contains("new-secret-fixture"));
        assert!(!html.contains("<operator>"));
        assert!(
            storage
                .verify_admin("admin", "admin")
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            storage
                .get_admin_session(&session.id)
                .await
                .unwrap()
                .is_some()
        );
    }
    let response = app
        .clone()
        .oneshot(post(
            csrf,
            "admin",
            "admin",
            "operator",
            "new-secret-fixture",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()["location"], "/admin/login?updated=true");
    assert!(
        response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    for id in [&session.id, &other_session.id] {
        assert!(storage.get_admin_session(id).await.unwrap().is_none());
    }
    assert!(
        storage
            .verify_admin("admin", "admin")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .verify_admin("operator", "admin")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .verify_admin("operator", "new-secret-fixture")
            .await
            .unwrap()
            .is_some()
    );
    let response = app.clone().oneshot(get()).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let response = app
        .oneshot(
            Request::get("/admin/login?updated=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(html.contains("请重新登录"));
    storage.close().await;
    let storage = Storage::open(&path).await.unwrap();
    assert!(
        storage
            .login_admin(
                "operator",
                "new-secret-fixture",
                std::time::Duration::from_secs(60)
            )
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        storage
            .login_admin("admin", "admin", std::time::Duration::from_secs(60))
            .await
            .unwrap()
            .is_none()
    );
    storage.close().await;
}
