//! Application composition: both HTTP surfaces share one account/auth/upstream context.
//! Actual endpoint lists live in codex2api-api::routes and codex2api-admin::router.
//! main.rs owns environment settings, storage startup, listening and shutdown.

use anyhow::Result;
use axum::Router;
use codex2api_accounts::SupplierAccountStore;
use codex2api_auth::AuthService;
use codex2api_storage::Storage;
use codex2api_upstream::UpstreamPool;

pub fn router(storage: Storage) -> Result<Router> {
    router_with_public_base_url(storage, None)
}

pub fn router_with_public_base_url(
    storage: Storage,
    public_base_url: Option<&str>,
) -> Result<Router> {
    let accounts = SupplierAccountStore::open(storage.clone());
    let auth = AuthService::new(accounts.clone())?;
    let upstream = UpstreamPool::new(auth.clone());
    let mut api_state =
        codex2api_api::ApiState::new(storage.clone(), accounts.clone(), upstream.clone());
    if let Some(value) = public_base_url {
        api_state = api_state.with_public_base_url(value)?;
    }
    let admin_state =
        codex2api_admin::AdminState::from_parts(storage, accounts, auth).with_upstream(upstream);

    Ok(Router::new()
        .merge(codex2api_api::router(api_state))
        .merge(codex2api_admin::router(admin_state))
        .merge(codex2api_web::router()))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn composed_router_keeps_static_admin_and_consumer_surfaces_separate() {
        let directory = tempfile::tempdir().unwrap();
        let storage = codex2api_storage::Storage::open(directory.path().join("composition.sqlite"))
            .await
            .unwrap();
        let app = super::router(storage).unwrap();
        for path in ["/admin/", "/admin/login/", "/admin/authorize/"] {
            let response = app
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert!(
                response.headers()["content-type"]
                    .to_str()
                    .unwrap()
                    .starts_with("text/html")
            );
        }
        let response = app
            .clone()
            .oneshot(
                Request::get("/admin/login?next=consumers")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(
            response.headers()["location"],
            "/admin/login/?next=consumers"
        );
        let response = app
            .clone()
            .oneshot(
                Request::get("/admin/api/session")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(value["error"]["code"], "unauthorized");
        assert_eq!(
            app.clone()
                .oneshot(
                    Request::get("/admin/api/consumers")
                        .body(Body::empty())
                        .unwrap()
                )
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let missing = app
            .clone()
            .oneshot(
                Request::get("/admin/api/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        assert!(
            !missing
                .headers()
                .get("content-type")
                .is_some_and(|v| v.to_str().unwrap_or("").contains("text/html"))
        );
        let response = app
            .oneshot(
                Request::get("/api/oauth/chatgpt/oauth/authorize")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
