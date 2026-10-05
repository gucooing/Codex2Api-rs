//! Browser-only consent. Protocol token exchange remains on the AI API listener.
use crate::{
    UserState, auth,
    error::{Result, UserError},
};
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use codex2api_storage::{VirtualAccount, oauth_secret};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
    #[serde(default)]
    nonce: Option<String>,
}
impl AuthorizationRequest {
    pub fn provider(&self) -> &'static str {
        crate::providers::by_client(&self.client_id).map_or("", |p| p.id)
    }
    fn allowed_scopes(&self) -> &'static str {
        crate::providers::by_client(&self.client_id).map_or("", |p| p.scopes)
    }
    pub fn valid(&self) -> bool {
        let Ok(uri) = url::Url::parse(&self.redirect_uri) else {
            return false;
        };
        self.response_type == "code"
            && crate::providers::by_client(&self.client_id).is_some_and(|p| {
                (!p.requires_nonce
                    || self.nonce.as_deref().is_some_and(|s| {
                        !s.is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control)
                    }))
                    && uri.path() == p.callback_path
            })
            && self.code_challenge_method == "S256"
            && self.code_challenge.len() == 43
            && self
                .code_challenge
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            && !self.state.is_empty()
            && self.state.len() <= 1024
            && !self.state.chars().any(char::is_control)
            && self
                .scope
                .split_whitespace()
                .all(|s| self.allowed_scopes().split_whitespace().any(|a| a == s))
            && uri.scheme() == "http"
            && matches!(uri.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
            && uri.port().is_some_and(|p| p != 0)
            && uri.username().is_empty()
            && uri.password().is_none()
            && uri.query().is_none()
            && uri.fragment().is_none()
    }
    fn scopes(&self) -> String {
        let requested = if self.scope.trim().is_empty() {
            "openid profile email offline_access"
        } else {
            &self.scope
        };
        self.allowed_scopes()
            .split_whitespace()
            .filter(|s| requested.split_whitespace().any(|v| v == *s))
            .collect::<Vec<_>>()
            .join(" ")
    }
}
fn failure() -> UserError {
    UserError::bad("授权请求无效或已过期，请回到应用重新发起登录")
}
async fn flow_cookie(state: &UserState, flow: &str, value: &str, age: u32) -> Result<String> {
    let origin = state
        .storage
        .public_user_url(&state.public_base_url)
        .await?;
    Ok(format!(
        "c2a_flow_{flow}={value}; HttpOnly; SameSite=Lax; Path=/user/api/oauth; Max-Age={age}{}",
        if origin.starts_with("https://") {
            "; Secure"
        } else {
            ""
        }
    ))
}
async fn identity_view(state: &UserState, account: &VirtualAccount) -> Result<Value> {
    let username = match state.storage.virtual_account_user(&account.id).await? {
        Some(id) => state.storage.user(&id).await?.ok_or_else(failure)?.username,
        None => account.username.clone(),
    };
    Ok(
        json!({"account_id":account.id,"username":username,"name":account.name,"email":account.email,
        "provider_id":account.provider_id,"plan_type":account.effective_plan()}),
    )
}
async fn start(
    state: UserState,
    headers: HeaderMap,
    request: String,
    scope: String,
) -> Result<Response> {
    let provider = flow_provider(&request)?;
    let flow = uuid::Uuid::new_v4().simple().to_string();
    let browser = oauth_secret();
    let csrf = oauth_secret();
    state
        .storage
        .create_oauth_browser_flow(&flow, &browser, &csrf, &request)
        .await?;
    let mut identity = Value::Null;
    let mut user = Value::Null;
    let mut account_unavailable = false;
    if let Some(session) = auth::load_session(&state, &headers).await? {
        if let Some(account) = state
            .storage
            .user_platform_account(&session.user_id, provider)
            .await?
        {
            if state
                .storage
                .bind_browser_identity(&flow, &account, Some(&session.token_hash))
                .await?
            {
                identity = identity_view(&state, &account).await?;
            }
        } else {
            let current = state
                .storage
                .user(&session.user_id)
                .await?
                .ok_or_else(UserError::unauthorized)?;
            user = json!({"username":current.username,"name":current.name,"email":current.email});
            account_unavailable = true;
        }
    }
    Ok((
        [(
            header::SET_COOKIE,
            flow_cookie(&state, &flow, &browser, 600).await?,
        )],
        Json(json!({"request_id":flow,
        "csrf_token":csrf,"client_name":crate::providers::by_id(provider).map(|p|p.client_name),"provider_id":provider,"scope":scope,"identity":identity,"user":user,
        "account_unavailable":account_unavailable,"expires_in":600})),
    )
        .into_response())
}
pub(crate) async fn bootstrap(
    State(state): State<UserState>,
    headers: HeaderMap,
    Query(request): Query<AuthorizationRequest>,
) -> Result<Response> {
    if !request.valid() {
        return Err(failure());
    }
    let scope = request.scopes();
    start(
        state,
        headers,
        serde_json::to_string(&request).map_err(|_| failure())?,
        scope,
    )
    .await
}
pub(crate) async fn device_bootstrap(
    State(state): State<UserState>,
    headers: HeaderMap,
    Query(query): Query<DeviceBootstrap>,
) -> Result<Response> {
    if !codex2api_core::supported_provider(&query.provider) {
        return Err(failure());
    }
    let policy = crate::providers::by_id(&query.provider).ok_or_else(failure)?;
    start(
        state,
        headers,
        policy.device_flow.into(),
        policy.scopes.into(),
    )
    .await
}
#[derive(Deserialize)]
pub(crate) struct DeviceBootstrap {
    #[serde(default = "codex2api_core::default_provider")]
    provider: String,
}
fn flow_provider(raw: &str) -> Result<&'static str> {
    if let Some(policy) = crate::providers::by_device_flow(raw) {
        return Ok(policy.id);
    }
    let request: AuthorizationRequest = serde_json::from_str(raw).map_err(|_| failure())?;
    if !request.valid() {
        return Err(failure());
    }
    Ok(request.provider())
}

async fn read_flow(
    state: &UserState,
    headers: &HeaderMap,
    flow: &str,
    csrf: &str,
) -> Result<(String, String)> {
    if flow.len() != 32 || !flow.bytes().all(|b| b.is_ascii_hexdigit()) || csrf.len() != 64 {
        return Err(failure());
    }
    let cookie = auth::cookie(headers, &format!("c2a_flow_{flow}")).ok_or_else(failure)?;
    let request = state
        .storage
        .oauth_browser_flow(flow, &cookie, csrf)
        .await?
        .ok_or_else(failure)?;
    Ok((cookie, request))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reset {
    request_id: String,
    csrf_token: String,
}
pub(crate) async fn cancel(
    State(state): State<UserState>,
    headers: HeaderMap,
    Json(input): Json<Reset>,
) -> Result<Response> {
    let (browser, raw) = read_flow(&state, &headers, &input.request_id, &input.csrf_token).await?;
    let result = if crate::providers::by_device_flow(&raw).is_some() {
        json!({"cancelled":true})
    } else {
        let request: AuthorizationRequest = serde_json::from_str(&raw).map_err(|_| failure())?;
        if !request.valid() {
            return Err(failure());
        }
        let mut callback = url::Url::parse(&request.redirect_uri).map_err(|_| failure())?;
        callback
            .query_pairs_mut()
            .append_pair("error", "access_denied")
            .append_pair("state", &request.state);
        json!({"redirect_uri":callback.as_str()})
    };
    if !state
        .storage
        .cancel_oauth_browser_flow(&input.request_id, &browser, &input.csrf_token)
        .await?
    {
        return Err(failure());
    }
    Ok((
        [(
            header::SET_COOKIE,
            flow_cookie(&state, &input.request_id, "", 0).await?,
        )],
        Json(result),
    )
        .into_response())
}
pub(crate) async fn reset(
    State(state): State<UserState>,
    headers: HeaderMap,
    Json(input): Json<Reset>,
) -> Result<Json<Value>> {
    read_flow(&state, &headers, &input.request_id, &input.csrf_token).await?;
    state
        .storage
        .clear_browser_identity(&input.request_id)
        .await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identify {
    request_id: String,
    csrf_token: String,
    kind: String,
    username: String,
    password: String,
}
pub(crate) async fn identify(
    State(state): State<UserState>,
    headers: HeaderMap,
    Json(input): Json<Identify>,
) -> Result<Json<Value>> {
    let (_, raw) = read_flow(&state, &headers, &input.request_id, &input.csrf_token).await?;
    let provider = flow_provider(&raw)?;
    state
        .storage
        .clear_browser_identity(&input.request_id)
        .await?;
    let account = match input.kind.as_str() {
        "user" => {
            let user = auth::verify_user(&state, &input.username, input.password).await?;
            state
                .storage
                .user_platform_account(&user.id, provider)
                .await?
                .ok_or_else(|| UserError::bad("该平台账户已停用，请联系管理员"))?
        }
        "virtual" => {
            if input.username.len() > 128
                || input.password.len() > 1024
                || !state
                    .storage
                    .allow_virtual_login_attempt(&format!("virtual:{}", input.username.trim()))
                    .await?
            {
                return Err(UserError(
                    StatusCode::TOO_MANY_REQUESTS,
                    "login_throttled",
                    "登录尝试过多，请稍后重试",
                ));
            }
            let account = state
                .storage
                .virtual_account_by_username(input.username.trim())
                .await?;
            let Some(account) = account.filter(|a| a.provider_id == provider) else {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                return Err(UserError::bad("用户名或密码错误，或账户已停用"));
            };
            let hash = account.password_hash.clone();
            if !matches!(
                tokio::task::spawn_blocking(move || codex2api_storage::verify_password(
                    &input.password,
                    &hash
                ))
                .await,
                Ok(Ok(true))
            ) {
                return Err(UserError::bad("用户名或密码错误，或账户已停用"));
            }
            account
        }
        _ => return Err(UserError::bad("请选择登录身份")),
    };
    if !state
        .storage
        .bind_browser_identity(&input.request_id, &account, None)
        .await?
    {
        return Err(failure());
    }
    // No persistent login cookie: this credential proof applies only to this authorization link.
    Ok(Json(
        json!({"identity":identity_view(&state,&account).await?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Approve {
    request_id: String,
    csrf_token: String,
    account_id: String,
    confirmed: bool,
    #[serde(default)]
    user_code: Option<String>,
}
async fn approved_account(state: &UserState, input: &Approve) -> Result<VirtualAccount> {
    if !input.confirmed {
        return Err(UserError::bad("请先确认登录身份"));
    }
    state
        .storage
        .browser_identity(&input.request_id)
        .await?
        .filter(|a| a.id == input.account_id)
        .ok_or_else(failure)
}
pub(crate) async fn approve(
    State(state): State<UserState>,
    headers: HeaderMap,
    Json(input): Json<Approve>,
) -> Result<Response> {
    let (browser, raw) = read_flow(&state, &headers, &input.request_id, &input.csrf_token).await?;
    let request: AuthorizationRequest = serde_json::from_str(&raw).map_err(|_| failure())?;
    if !request.valid() || input.user_code.is_some() {
        return Err(failure());
    }
    let account = approved_account(&state, &input).await?;
    if account.provider_id != request.provider() {
        return Err(failure());
    }
    let code = oauth_secret();
    if !state
        .storage
        .authorize_oauth_browser_flow(codex2api_storage::BrowserAuthorization {
            id: &input.request_id,
            cookie: &browser,
            csrf: &input.csrf_token,
            account: &account,
            code: &code,
            client_id: &request.client_id,
            redirect_uri: &request.redirect_uri,
            challenge: &request.code_challenge,
            scopes: &request.scopes(),
        })
        .await?
    {
        return Err(failure());
    }
    let mut callback = url::Url::parse(&request.redirect_uri).map_err(|_| failure())?;
    callback
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &request.state);
    Ok((
        [(
            header::SET_COOKIE,
            flow_cookie(&state, &input.request_id, "", 0).await?,
        )],
        Json(json!({"redirect_uri":callback.as_str()})),
    )
        .into_response())
}
pub(crate) async fn device_approve(
    State(state): State<UserState>,
    headers: HeaderMap,
    Json(input): Json<Approve>,
) -> Result<Response> {
    let (browser, raw) = read_flow(&state, &headers, &input.request_id, &input.csrf_token).await?;
    if !crate::providers::by_device_flow(&raw).is_some() {
        return Err(failure());
    }
    let account = approved_account(&state, &input).await?;
    if account.provider_id != flow_provider(&raw)? {
        return Err(failure());
    }
    let user_code = input
        .user_code
        .as_deref()
        .filter(|v| !v.is_empty() && v.len() <= 32)
        .ok_or_else(failure)?;
    let normalized = user_code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase();
    if !state
        .storage
        .approve_device_authorization(
            &normalized,
            codex2api_storage::DeviceAuthorizationApproval {
                id: &input.request_id,
                cookie: &browser,
                csrf: &input.csrf_token,
                account: &account,
                code: &oauth_secret(),
                scopes: crate::providers::by_id(&account.provider_id)
                    .ok_or_else(failure)?
                    .scopes,
            },
        )
        .await?
    {
        return Err(UserError::bad("设备码无效、已使用或已过期"));
    }
    Ok((
        [(
            header::SET_COOKIE,
            flow_cookie(&state, &input.request_id, "", 0).await?,
        )],
        Json(json!({"authorized":true})),
    )
        .into_response())
}
