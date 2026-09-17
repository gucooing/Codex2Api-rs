use crate::AdminState;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use std::time::Duration;

use axum::http::{HeaderMap, header};
use codex2api_storage::{AdminSession, Storage};

pub const SESSION_COOKIE: &str = "c2a_admin_session";

pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        if key == name && !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

pub fn session_id_from_headers(headers: &HeaderMap) -> Option<String> {
    cookie_value(headers, SESSION_COOKIE)
}

pub async fn load_session(storage: &Storage, headers: &HeaderMap) -> Option<AdminSession> {
    let id = session_id_from_headers(headers)?;
    storage.get_admin_session(&id).await.ok().flatten()
}

pub fn set_session_cookie(id: &str, ttl: Duration) -> String {
    format!(
        "{SESSION_COOKIE}={id}; HttpOnly; Path=/admin; SameSite=Lax; Max-Age={}",
        ttl.as_secs()
    )
}

pub fn clear_session_cookie() -> String {
    format!("{SESSION_COOKIE}=; HttpOnly; Path=/admin; SameSite=Lax; Max-Age=0")
}

pub(crate) async fn require_session(
    State(state): State<AdminState>,
    req: Request,
    next: Next,
) -> Response {
    if load_session(&state.storage, req.headers()).await.is_some() {
        next.run(req).await
    } else {
        Redirect::to("/admin/login").into_response()
    }
}
