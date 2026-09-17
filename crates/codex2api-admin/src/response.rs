//! Shared admin error pages, redirects and URL encoding.
use crate::views as html;
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use codex2api_storage::StorageError;

pub(crate) fn not_found_account(id: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Html(html::error_page("账户不存在", &format!("找不到账户 {id}"))),
    )
        .into_response()
}

pub(crate) fn storage_error(title: &str, err: StorageError) -> Response {
    tracing::error!(%err, "{title}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Html(html::error_page(title, &err.to_string())),
    )
        .into_response()
}

pub(crate) fn redirect_with_cookie(path: &str, cookie: String) -> Response {
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, path.to_string()),
            (header::SET_COOKIE, cookie),
        ],
    )
        .into_response()
}

pub(crate) fn encode_query(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
