//! Static Next.js export embedded at compile time. No Node.js server or runtime files.
use axum::{
    Router,
    extract::OriginalUri,
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
include!(concat!(env!("OUT_DIR"), "/assets.rs"));
pub fn router() -> Router {
    Router::new()
        .route("/admin", get(|| async { Redirect::permanent("/admin/") }))
        .route("/admin/", get(serve))
        .route("/admin/{*path}", get(serve))
}
async fn serve(OriginalUri(uri): OriginalUri) -> Response {
    let Some(path) = uri.path().strip_prefix("/admin/") else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if path == "api" || path.starts_with("api/") || path.contains("..") || path.contains('\\') {
        return StatusCode::NOT_FOUND.into_response();
    }
    if !path.is_empty()
        && !path.ends_with('/')
        && ASSETS
            .iter()
            .any(|(name, _)| *name == format!("{path}/index.html"))
    {
        let mut target = format!("/admin/{path}/");
        if let Some(query) = uri.query() {
            target.push('?');
            target.push_str(query);
        }
        return Redirect::permanent(&target).into_response();
    }
    let path = if path.is_empty() {
        "index.html".into()
    } else if path.ends_with('/') {
        format!("{path}index.html")
    } else {
        path.into()
    };
    let Some((_, body)) = ASSETS.iter().find(|(name, _)| *name == path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    };
    let cache = if path.starts_with("_next/static/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-store"
    };
    (
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, cache),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::X_FRAME_OPTIONS, "DENY"),
            (header::REFERRER_POLICY, "same-origin"),
        ],
        *body,
    )
        .into_response()
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    #[tokio::test]
    async fn static_export_is_embedded_and_api_misses_never_return_html() {
        for path in ["/admin/", "/admin/login/", "/admin/consumers/"] {
            let r = router()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(r.status(), StatusCode::OK, "{path}");
            assert_eq!(r.headers()[header::CACHE_CONTROL], "no-store");
            let bytes = to_bytes(r.into_body(), usize::MAX).await.unwrap();
            assert!(String::from_utf8_lossy(&bytes).contains("<html"));
        }
        for path in [
            "/admin/api/missing",
            "/admin/missing/",
            "/admin/../data.sqlite",
        ] {
            assert_eq!(
                router()
                    .oneshot(Request::get(path).body(Body::empty()).unwrap())
                    .await
                    .unwrap()
                    .status(),
                StatusCode::NOT_FOUND
            );
        }
        for page in [
            "usage",
            "suppliers",
            "consumers",
            "plans",
            "models",
            "proxies",
            "settings",
        ] {
            let path = format!("/admin/{page}/__next.{page}.__PAGE__.txt?_rsc=review");
            let response = router()
                .oneshot(Request::get(&path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert_eq!(
                response.headers()[header::CONTENT_TYPE],
                "text/plain; charset=utf-8"
            );
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            assert!(!bytes.is_empty());
            assert!(!String::from_utf8_lossy(&bytes).contains("<html"));
        }
        let asset = ASSETS
            .iter()
            .find(|(name, _)| name.starts_with("_next/static/") && name.ends_with(".js"))
            .unwrap();
        let r = router()
            .oneshot(
                Request::get(format!("/admin/{}", asset.0))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            r.headers()[header::CACHE_CONTROL],
            "public, max-age=31536000, immutable"
        );
        assert_eq!(
            to_bytes(r.into_body(), usize::MAX).await.unwrap().as_ref(),
            asset.1
        );
    }
}
