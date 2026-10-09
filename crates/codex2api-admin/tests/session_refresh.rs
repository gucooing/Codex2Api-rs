mod common;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use codex2api_storage::TokenPurpose;
use common::{Fixture, body};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn refresh_recovers_expired_access_and_reuses_concurrent_rotation() {
    let f = Fixture::new().await;
    let access = f.cookie.split_once('=').unwrap().1;
    let session = f
        .storage
        .admin_session_from_jwt(access)
        .await
        .unwrap()
        .unwrap();
    let tokens = f.storage.admin_session_tokens(&session).await.unwrap();
    let mut claims = f
        .storage
        .verify_jwt(TokenPurpose::AdminSession, access)
        .await
        .unwrap();
    claims["iat"] = json!(chrono::Utc::now().timestamp() - 901);
    claims["exp"] = json!(chrono::Utc::now().timestamp() - 1);
    let expired = f
        .storage
        .sign_jwt(TokenPurpose::AdminSession, claims)
        .await
        .unwrap();
    let cookies = format!(
        "c2a_admin_session={expired}; c2a_admin_refresh={}",
        tokens.refresh_token
    );
    assert_eq!(
        f.with_auth(
            "GET",
            "/admin/api/session",
            json!(null),
            Some(&cookies),
            None
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.with_auth(
            "POST",
            "/admin/api/session/refresh",
            json!({}),
            Some(&cookies),
            None
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE admin_sessions SET refresh_issued_at=unixepoch()-31 WHERE id=?")
        .bind(&session.id)
        .execute(f.storage.pool())
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        f.with_auth(
            "POST",
            "/admin/api/session/refresh",
            json!({}),
            Some(&cookies),
            Some(&f.csrf)
        ),
        f.with_auth(
            "POST",
            "/admin/api/session/refresh",
            json!({}),
            Some(&cookies),
            Some(&f.csrf)
        ),
    );
    assert_eq!(a.status(), StatusCode::OK);
    assert_eq!(b.status(), StatusCode::OK);
    let get_cookie = |response: &axum::response::Response, name: &str| {
        response
            .headers()
            .get_all("set-cookie")
            .iter()
            .map(|h| h.to_str().unwrap())
            .find(|s| s.starts_with(name))
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned()
    };
    let refreshed = get_cookie(&a, "c2a_admin_session=");
    let refresh_a = get_cookie(&a, "c2a_admin_refresh=");
    assert_eq!(refresh_a, get_cookie(&b, "c2a_admin_refresh="));
    let current = body(
        f.with_auth(
            "GET",
            "/admin/api/session",
            json!(null),
            Some(&refreshed),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(current["csrf_token"], f.csrf);
    assert!(
        a.headers()
            .get_all("set-cookie")
            .iter()
            .any(|h| h.to_str().unwrap().contains("Max-Age=900; HttpOnly"))
    );
    assert_eq!(
        f.with_auth(
            "GET",
            "/admin/api/session",
            json!(null),
            Some(&format!("c2a_admin_session={}", tokens.refresh_token)),
            None
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.with_auth(
            "POST",
            "/admin/api/session/refresh",
            json!({}),
            Some(&format!("c2a_admin_refresh={access}")),
            Some(&f.csrf)
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let explicit_bearer = f
        .app
        .clone()
        .oneshot(
            Request::post("/admin/api/session/refresh")
                .header("cookie", &refresh_a)
                .header("authorization", "Bearer invalid")
                .header("x-csrf-token", &f.csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(explicit_bearer.status(), StatusCode::UNAUTHORIZED);
    let cross_site = f
        .app
        .clone()
        .oneshot(
            Request::post("/admin/api/session/refresh")
                .header("cookie", &refresh_a)
                .header("sec-fetch-site", "cross-site")
                .header("x-csrf-token", &f.csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cross_site.status(), StatusCode::FORBIDDEN);
    sqlx::query("UPDATE admin_sessions SET refresh_issued_at=unixepoch()-31 WHERE id=?")
        .bind(&session.id)
        .execute(f.storage.pool())
        .await
        .unwrap();
    assert_eq!(
        f.with_auth(
            "POST",
            "/admin/api/session/refresh",
            json!({}),
            Some(&cookies),
            Some(&f.csrf)
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let logout = f
        .with_auth(
            "POST",
            "/admin/api/logout",
            json!({}),
            Some(&refreshed),
            Some(&f.csrf),
        )
        .await;
    assert_eq!(logout.status(), StatusCode::OK);
    assert_eq!(
        logout
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter(|h| h.to_str().unwrap().contains("Max-Age=0"))
            .count(),
        3
    );
    assert_eq!(
        f.with_auth(
            "POST",
            "/admin/api/session/refresh",
            json!({}),
            Some(&refresh_a),
            Some(&f.csrf)
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert!(
        f.storage
            .renew_admin_session(&session)
            .await
            .unwrap()
            .is_none()
    );
}
