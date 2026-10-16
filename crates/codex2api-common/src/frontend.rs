//! Static asset serving shared by the two embedded websites.
use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};

pub fn serve_assets(
    uri: axum::http::Uri,
    prefix: &str,
    assets: &'static [(&str, &[u8])],
) -> Response {
    let Some(path) = uri.path().strip_prefix(prefix) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if path == "api" || path.starts_with("api/") || path.contains("..") || path.contains('\\') {
        return StatusCode::NOT_FOUND.into_response();
    }
    if !path.is_empty()
        && !path.ends_with('/')
        && assets
            .iter()
            .any(|(name, _)| *name == format!("{path}/index.html"))
    {
        let mut target = format!("{prefix}{path}/");
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
    let Some((_, body)) = assets.iter().find(|(name, _)| *name == path) else {
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
