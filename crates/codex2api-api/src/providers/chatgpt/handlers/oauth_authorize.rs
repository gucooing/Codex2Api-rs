use super::oauth::PREFIX;
use crate::ApiState;
use axum::{
    Json,
    extract::{OriginalUri, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct AuthorizationRequest {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    state: String,
    code_challenge: String,
    code_challenge_method: String,
    #[serde(default)]
    scope: String,
}

impl AuthorizationRequest {
    fn granted_scopes(&self) -> String {
        let requested = if self.scope.trim().is_empty() {
            "openid profile email offline_access"
        } else {
            self.scope.as_str()
        };
        codex2api_version::OAUTH_SCOPE
            .split_whitespace()
            .filter(|scope| requested.split_whitespace().any(|s| s == *scope))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn valid(&self) -> bool {
        let Ok(uri) = url::Url::parse(&self.redirect_uri) else {
            return false;
        };
        self.response_type == "code"
            && self.client_id == codex2api_version::OAUTH_CLIENT_ID
            && self.code_challenge_method == "S256"
            && self.code_challenge.len() == 43
            && self
                .code_challenge
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            && !self.state.is_empty()
            && self.state.len() <= 1024
            && !self.state.chars().any(char::is_control)
            && self.scope.split_whitespace().all(|s| {
                codex2api_version::OAUTH_SCOPE
                    .split_whitespace()
                    .any(|allowed| s == allowed)
            })
            && uri.scheme() == "http"
            && matches!(uri.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
            && uri.port().is_some_and(|port| port != 0)
            && uri.path() == "/auth/callback"
            && uri.username().is_empty()
            && uri.password().is_none()
            && uri.query().is_none()
            && uri.fragment().is_none()
    }
}

fn page(status: StatusCode, _flow: &str, _csrf: &str, message: &str) -> Response {
    (
        status,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        Json(serde_json::json!({"error":{"code":"authorization_failed","message":message}})),
    )
        .into_response()
}

fn login_page(
    _request: &AuthorizationRequest,
    status: StatusCode,
    flow: &str,
    csrf: &str,
    message: &str,
) -> Response {
    page(status, flow, csrf, message)
}

fn failure() -> Response {
    page(
        StatusCode::BAD_REQUEST,
        "",
        "",
        "授权请求无效或已过期，请回到应用重新发起登录。",
    )
}

#[derive(Deserialize)]
pub struct DesktopAuthorizationQuery {
    authorize_url: String,
}

/// The desktop renderer preserves this exact pathname instead of wrapping it on chatgpt.com.
pub async fn desktop_authorize(Query(query): Query<DesktopAuthorizationQuery>) -> Response {
    if query.authorize_url.len() > 16 * 1024 {
        return failure();
    }
    let Ok(target) = url::Url::parse(&query.authorize_url) else {
        return failure();
    };
    if !matches!(target.scheme(), "http" | "https")
        || target.path() != format!("{PREFIX}/oauth/authorize")
        || !target.username().is_empty()
        || target.password().is_some()
        || target.fragment().is_some()
    {
        return failure();
    }
    // Discard the supplied origin entirely: this endpoint can redirect only to our local route.
    let local = format!("{PREFIX}/oauth/authorize?{}", target.query().unwrap_or(""));
    let Ok(uri) = local.parse::<axum::http::Uri>() else {
        return failure();
    };
    let Ok(Query(request)) = Query::<AuthorizationRequest>::try_from_uri(&uri) else {
        return failure();
    };
    if !request.valid() {
        return failure();
    }
    let mut response = Redirect::to(&local).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    response
}
fn secret() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn cookie_name(flow: &str) -> String {
    format!("c2a_oauth_{flow}")
}
fn cookie(headers: &HeaderMap, flow: &str) -> Option<String> {
    let name = cookie_name(flow);
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|item| {
            let (key, value) = item.trim().split_once('=')?;
            (key == name).then(|| value.to_owned())
        })
}

pub async fn authorize(
    Query(request): Query<AuthorizationRequest>,
    OriginalUri(uri): OriginalUri,
) -> Response {
    if !request.valid() {
        return failure();
    }
    let mut response = Redirect::to(&format!(
        "/admin/authorize/?{}",
        uri.query().unwrap_or_default()
    ))
    .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    response
}

pub async fn bootstrap(
    State(state): State<ApiState>,
    Query(request): Query<AuthorizationRequest>,
) -> Response {
    if !request.valid() {
        return failure();
    }
    let flow = uuid::Uuid::new_v4().simple().to_string();
    let browser = secret();
    let csrf = secret();
    let result = state
        .storage
        .create_oauth_browser_flow(
            &flow,
            &browser,
            &csrf,
            &serde_json::to_string(&request).unwrap(),
        )
        .await;
    if let Err(err) = result {
        tracing::error!(%err,"failed to create OAuth browser flow");
        return page(
            StatusCode::INTERNAL_SERVER_ERROR,
            "",
            "",
            "暂时无法登录，请稍后重试。",
        );
    }
    let mut response = ([(header::CACHE_CONTROL,"no-store"),(header::REFERRER_POLICY,"no-referrer")], Json(serde_json::json!({"request_id":flow,"csrf_token":csrf,"client_name":"Codex","scope":request.granted_scopes(),"redirect_uri":request.redirect_uri,"expires_in":600}))).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        format!(
            "{}={browser}; HttpOnly; SameSite=Lax; Path={PREFIX}/oauth/authorize; Max-Age=600",
            cookie_name(&flow)
        )
        .parse()
        .unwrap(),
    );
    response
}

#[derive(Deserialize)]
pub struct AuthorizeForm {
    #[serde(rename = "request_id")]
    flow: String,
    #[serde(rename = "csrf_token")]
    csrf: String,
    username: String,
    password: String,
}

pub async fn approve(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(form): Json<AuthorizeForm>,
) -> Response {
    if form.flow.len() != 32 || !form.flow.bytes().all(|b| b.is_ascii_hexdigit()) {
        return failure();
    }
    let Some(browser) = cookie(&headers, &form.flow) else {
        return failure();
    };
    let request = match state
        .storage
        .oauth_browser_flow(&form.flow, &browser, &form.csrf)
        .await
    {
        Ok(Some(request)) => request,
        Ok(None) => return failure(),
        Err(err) => {
            tracing::error!(%err,"failed to read OAuth browser flow");
            return failure();
        }
    };
    let Ok(request) = serde_json::from_str::<AuthorizationRequest>(&request) else {
        return failure();
    };
    if !request.valid() {
        return failure();
    }
    if form.username.len() > 128 || form.password.len() > 1024 {
        return failure();
    }
    if !matches!(
        state
            .storage
            .allow_virtual_login_attempt(form.username.trim())
            .await,
        Ok(true)
    ) {
        return login_page(
            &request,
            StatusCode::TOO_MANY_REQUESTS,
            &form.flow,
            &form.csrf,
            "登录尝试过多，请一分钟后重试。",
        );
    }
    let account = match state
        .storage
        .virtual_account_by_username(form.username.trim())
        .await
    {
        Ok(Some(account)) if account.provider_id == codex2api_core::CHATGPT => account,
        Ok(_) => {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            return login_page(
                &request,
                StatusCode::BAD_REQUEST,
                &form.flow,
                &form.csrf,
                "用户名或密码错误，或账号已停用。",
            );
        }
        Err(_) => return failure(),
    };
    let hash = account.password_hash.clone();
    let password = form.password;
    let verified =
        tokio::task::spawn_blocking(move || codex2api_storage::verify_password(&password, &hash))
            .await;
    if !matches!(verified, Ok(Ok(true))) {
        return login_page(
            &request,
            StatusCode::BAD_REQUEST,
            &form.flow,
            &form.csrf,
            "用户名或密码错误，或账号已停用。",
        );
    }
    let code = secret();
    match state
        .storage
        .authorize_oauth_browser_flow(codex2api_storage::BrowserAuthorization {
            id: &form.flow,
            cookie: &browser,
            csrf: &form.csrf,
            account: &account,
            code: &code,
            client_id: &request.client_id,
            redirect_uri: &request.redirect_uri,
            challenge: &request.code_challenge,
            scopes: &request.granted_scopes(),
        })
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            return login_page(
                &request,
                StatusCode::BAD_REQUEST,
                &form.flow,
                &form.csrf,
                "账号或绑定账户不可用，请联系管理员。",
            );
        }
        Err(err) => {
            tracing::error!(%err,"failed to approve OAuth login");
            return login_page(
                &request,
                StatusCode::INTERNAL_SERVER_ERROR,
                &form.flow,
                &form.csrf,
                "暂时无法授权，请稍后重试。",
            );
        }
    }
    let mut callback = url::Url::parse(&request.redirect_uri).unwrap();
    callback
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &request.state);
    let mut response = Json(serde_json::json!({"redirect_uri":callback.as_str()})).into_response();
    response
        .headers_mut()
        .insert(header::LOCATION, callback.as_str().parse().unwrap());
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    response.headers_mut().insert(
        header::SET_COOKIE,
        format!(
            "{}=; HttpOnly; SameSite=Lax; Path={PREFIX}/oauth/authorize; Max-Age=0",
            cookie_name(&form.flow)
        )
        .parse()
        .unwrap(),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_loopback_oauth_callbacks_are_authorized() {
        let mut request = AuthorizationRequest {
            response_type: "code".into(),
            client_id: codex2api_version::OAUTH_CLIENT_ID.into(),
            redirect_uri: String::new(),
            state: "fixture".into(),
            code_challenge: "A".repeat(43),
            code_challenge_method: "S256".into(),
            scope: String::new(),
        };
        for origin in [
            "http://localhost:1455",
            "http://127.0.0.1:23456",
            "http://[::1]:4567",
        ] {
            request.redirect_uri = format!("{origin}/auth/callback");
            assert!(request.valid());
        }
        request.redirect_uri = "https://untrusted.example/auth/callback".into();
        assert!(!request.valid());
    }
}
