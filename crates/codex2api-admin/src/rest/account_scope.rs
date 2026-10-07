use axum::{
    extract::{MatchedPath, Path, Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use codex2api_storage::{AccountScope, Storage};
use std::collections::HashMap;

/// Installed once on the admin API. New routes inherit their namespace's scope.
pub(super) async fn enforce(
    State(storage): State<Storage>,
    matched: MatchedPath,
    Path(parameters): Path<HashMap<String, String>>,
    request: Request,
    next: Next,
) -> Response {
    let namespace = matched
        .as_str()
        .trim_start_matches("/admin/api/")
        .trim_start_matches('/')
        .split('/')
        .next();
    let scope = match namespace {
        Some("users") => Some(AccountScope::User),
        Some("consumers") => Some(AccountScope::VirtualAccount),
        Some("subscriptions") => Some(AccountScope::UserPlatform),
        _ => None,
    };
    let id = parameters
        .get("id")
        .or_else(|| parameters.get("user_id"))
        .or_else(|| parameters.get("account_id"));
    if let (Some(scope), Some(id)) = (scope, id) {
        if let Err(error) = storage.require_account_scope(id, scope).await {
            return super::error::ApiError::from(error).into_response();
        }
    }
    next.run(request).await
}
