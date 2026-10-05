//! Protocol entry points redirect to the independently hosted user consent surface.
use crate::ApiState;
use axum::{
    extract::{OriginalUri, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use codex2api_user::AuthorizationRequest;
use serde::Deserialize;

fn failure() -> Response {
    (StatusCode::BAD_REQUEST, "Invalid authorization request.").into_response()
}
fn redirect(target: &str) -> Response {
    let mut response = Redirect::to(target).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    response
}
pub async fn authorize(
    State(state): State<ApiState>,
    Query(request): Query<AuthorizationRequest>,
    OriginalUri(uri): OriginalUri,
) -> Response {
    if !request.valid() {
        return failure();
    }
    let user_url = match state.public_user_url().await {
        Ok(url) => url,
        Err(error) => return crate::ApiError::from(error).into_response(),
    };
    redirect(&format!(
        "{}/user/authorize/?{}",
        user_url,
        uri.query().unwrap_or_default()
    ))
}
#[derive(Deserialize)]
pub struct DesktopAuthorizationQuery {
    authorize_url: String,
}
pub async fn desktop_authorize(
    State(state): State<ApiState>,
    Query(query): Query<DesktopAuthorizationQuery>,
) -> Response {
    if query.authorize_url.len() > 16 * 1024 {
        return failure();
    }
    let Ok(target) = url::Url::parse(&query.authorize_url) else {
        return failure();
    };
    if !matches!(target.scheme(), "http" | "https")
        || target.path() != format!("{}/oauth/authorize", super::oauth::PREFIX)
        || !target.username().is_empty()
        || target.password().is_some()
        || target.fragment().is_some()
    {
        return failure();
    }
    let local = format!(
        "{}/oauth/authorize?{}",
        super::oauth::PREFIX,
        target.query().unwrap_or_default()
    );
    let Ok(uri) = local.parse::<axum::http::Uri>() else {
        return failure();
    };
    let Ok(Query(request)) = Query::<AuthorizationRequest>::try_from_uri(&uri) else {
        return failure();
    };
    if !request.valid() {
        return failure();
    }
    let user_url = match state.public_user_url().await {
        Ok(url) => url,
        Err(error) => return crate::ApiError::from(error).into_response(),
    };
    redirect(&format!(
        "{}/user/authorize/?{}",
        user_url,
        target.query().unwrap_or_default()
    ))
}
