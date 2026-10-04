use crate::{
    UserState,
    error::{Result, UserError},
};
use axum::{
    Json,
    extract::{Extension, Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use codex2api_storage::{User, UserSession};
use serde::Deserialize;
use serde_json::json;
pub(crate) const COOKIE: &str = "c2a_user_session";
// Missing users still incur the normal password-verification cost, without
// consulting administrator credentials. This value can never authenticate a user.
const MISSING_USER_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
pub(crate) fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .flat_map(|h| h.split(';'))
        .find_map(|part| {
            let (k, v) = part.trim().split_once('=')?;
            (k == name && !v.is_empty()).then(|| v.to_owned())
        })
}
pub(crate) fn session_cookie(state: &UserState, token: &str, max_age: u32) -> String {
    format!(
        "{COOKIE}={token}; HttpOnly; SameSite=Lax; Path=/user; Max-Age={max_age}{}",
        if state.public_base_url.starts_with("https://") {
            "; Secure"
        } else {
            ""
        }
    )
}
pub(crate) async fn load_session(
    state: &UserState,
    headers: &HeaderMap,
) -> Result<Option<UserSession>> {
    let token = if headers.contains_key(header::AUTHORIZATION) {
        if headers.get_all(header::AUTHORIZATION).iter().count() != 1 {
            return Err(UserError::unauthorized());
        }
        Some(
            headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split_once(' '))
                .filter(|(scheme, token)| {
                    scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()
                })
                .map(|(_, token)| token.to_owned())
                .ok_or_else(UserError::unauthorized)?,
        )
    } else {
        cookie(headers, COOKIE)
    };
    match token {
        Some(token) => Ok(state.storage.user_session(&token).await?),
        None => Ok(None),
    }
}
pub(crate) async fn browser_boundary(
    State(state): State<UserState>,
    request: Request,
    next: Next,
) -> Response {
    // Browser sessions are a distinct credential audience, even when another cookie is present.
    let rejection = if request.headers().contains_key(header::AUTHORIZATION) {
        match load_session(&state, request.headers()).await {
            Ok(Some(_)) => None,
            Ok(None) => Some(UserError::unauthorized()),
            Err(error) => Some(error),
        }
    } else if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) {
        // Fetch metadata describes the browser-facing origin, before a trusted
        // same-origin proxy changes the backend port. Scripts cannot set Sec-*.
        let site = request
            .headers()
            .get("sec-fetch-site")
            .and_then(|value| value.to_str().ok());
        let same_origin = site == Some("same-origin")
            || request
                .headers()
                .get(header::ORIGIN)
                .is_none_or(|v| v.to_str().ok() == Some(state.public_base_url.as_str()));
        if !same_origin || site == Some("cross-site") {
            Some(UserError(
                StatusCode::FORBIDDEN,
                "forbidden",
                "不接受跨站请求",
            ))
        } else {
            None
        }
    } else {
        None
    };
    let mut response = match rejection {
        Some(error) => error.into_response(),
        None => next.run(request).await,
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    response
        .headers_mut()
        .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    response
}
pub(crate) async fn require_session(
    State(state): State<UserState>,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let session = load_session(&state, request.headers())
        .await?
        .ok_or_else(UserError::unauthorized)?;
    if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) && request
        .headers()
        .get("x-csrf-token")
        .and_then(|h| h.to_str().ok())
        != Some(session.csrf_token.as_str())
    {
        return Err(UserError(
            StatusCode::FORBIDDEN,
            "forbidden",
            "CSRF 校验失败，请刷新后重试",
        ));
    }
    request.extensions_mut().insert(session);
    Ok(next.run(request).await)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Login {
    pub username: String,
    pub password: String,
}
pub(crate) async fn verify_user(
    state: &UserState,
    username: &str,
    password: String,
) -> Result<User> {
    if username.len() > 128
        || password.len() > 1024
        || username.trim().is_empty()
        || password.is_empty()
    {
        return Err(UserError::bad("请填写用户名和密码"));
    }
    if !state
        .storage
        .allow_virtual_login_attempt(&format!("user:{}", username.trim()))
        .await?
    {
        return Err(UserError(
            StatusCode::TOO_MANY_REQUESTS,
            "login_throttled",
            "登录尝试过多，请一分钟后重试",
        ));
    }
    let user = state.storage.user_by_username(username.trim()).await?;
    // Use a real Argon2 hash even for unknown identities to avoid fast username enumeration.
    let hash = match &user {
        Some(u) => u.password_hash.clone(),
        None => MISSING_USER_HASH.into(),
    };
    let verified =
        tokio::task::spawn_blocking(move || codex2api_storage::verify_password(&password, &hash))
            .await;
    match (user, verified) {
        (Some(user), Ok(Ok(true))) => Ok(user),
        _ => Err(UserError(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "用户名或密码错误，或账户已停用",
        )),
    }
}
pub(crate) async fn login(
    State(state): State<UserState>,
    headers: HeaderMap,
    Json(input): Json<Login>,
) -> Result<Response> {
    let user = verify_user(&state, &input.username, input.password).await?;
    let csrf = codex2api_storage::oauth_secret();
    let token = state
        .storage
        .create_user_session(&user, &csrf)
        .await?
        .ok_or_else(UserError::unauthorized)?;
    if let Some(old) = load_session(&state, &headers).await? {
        state.storage.revoke_user_session(&old.token_hash).await?;
    }
    Ok((
        [(header::SET_COOKIE, session_cookie(&state, &token, 86400))],
        Json(json!({"user":user.view(),"csrf_token":csrf})),
    )
        .into_response())
}
pub(crate) async fn logout(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
) -> Result<Response> {
    state
        .storage
        .revoke_user_session(&session.token_hash)
        .await?;
    Ok((
        [(header::SET_COOKIE, session_cookie(&state, "", 0))],
        Json(json!({"ok":true})),
    )
        .into_response())
}
