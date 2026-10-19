//! Antigravity supplier authorization: the desktop client's Google OAuth, Cloud Code
//! Assist onboarding and account-scoped HTTP clients. No Codex or Grok headers.
mod credentials;
mod error;
pub mod identity;
pub mod transport;

pub use credentials::AntigravityCredentials;
pub use error::AntigravityAuthError;
pub use identity::AntigravityIdentity;
pub type Result<T> = std::result::Result<T, AntigravityAuthError>;

use AntigravityAuthError as AuthError;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use codex2api_common::version::antigravity as wire;
use codex2api_storage::{NewSupplierAccount, SupplierAccount};
use reqwest::{
    Client,
    header::{HeaderMap, HeaderValue},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use transport::AntigravityHttpClients;

#[derive(Clone)]
pub struct AntigravityConfig {
    pub authorize_url: String,
    pub token_url: String,
    pub userinfo_url: String,
    /// Service roots in preference order; a later root is tried only after a
    /// connection failure or an unroutable response from the earlier one.
    pub base_urls: Vec<String>,
}
impl Default for AntigravityConfig {
    fn default() -> Self {
        Self {
            authorize_url: wire::OAUTH_AUTHORIZE_URL.into(),
            token_url: wire::OAUTH_TOKEN_URL.into(),
            userinfo_url: wire::USERINFO_URL.into(),
            base_urls: vec![wire::BASE_URL.into(), wire::DAILY_BASE_URL.into()],
        }
    }
}
impl AntigravityConfig {
    /// Paid tiers execute on the daily root like the desktop client; others on production.
    pub fn service_roots(&self, paid: bool) -> impl Iterator<Item = &str> {
        let mut roots: Vec<&str> = self.base_urls.iter().map(String::as_str).collect();
        if paid && roots.len() > 1 {
            roots.swap(0, 1);
        }
        roots.into_iter()
    }
}

#[derive(Clone)]
pub struct AntigravityAuthService {
    storage: Option<codex2api_storage::Storage>,
    config: AntigravityConfig,
    drafts: Arc<tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<Draft>>>>>,
    clients: Arc<crate::account_cache::AccountCache<CachedAccountHttp>>,
}

#[derive(Clone)]
struct CachedAccountHttp {
    snapshot: Arc<codex2api_storage::SupplierTransportSnapshot>,
    clients: Arc<AntigravityHttpClients>,
}

struct Draft {
    expires: Instant,
    identity: AntigravityIdentity,
    proxy_id: Option<String>,
    expected: Option<String>,
    http: AntigravityHttpClients,
    verifier: String,
    active: bool,
}

#[derive(Clone, Deserialize)]
struct Tokens {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// The onboarding result the desktop client needs before it can generate.
pub struct CodeAssist {
    pub project: Option<String>,
    pub tier: Option<String>,
    pub raw: Value,
}

pub fn headers(identity: &AntigravityIdentity, access: Option<&str>) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::USER_AGENT,
        HeaderValue::from_str(&identity.user_agent()).map_err(anyhow::Error::from)?,
    );
    if let Some(access) = access {
        headers.insert(
            reqwest::header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {access}")).map_err(anyhow::Error::from)?,
        );
    }
    Ok(headers)
}

pub(crate) fn problem(message: &str) -> AuthError {
    anyhow::anyhow!("{message}").into()
}

fn owner(profile: &Value) -> Result<String> {
    profile["email"]
        .as_str()
        .map(str::trim)
        .filter(|s| s.contains('@'))
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| problem("Google 未返回账户邮箱"))
}

/// Supplier ownership is the Google account; a workspace project can change.
fn principal(email: &str) -> String {
    format!("google:{email}")
}

/// A root is abandoned for a connection failure or a response no root should give.
fn unroutable(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::NOT_FOUND
        || status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status.is_server_error()
}

impl AntigravityAuthService {
    pub fn new(storage: Option<codex2api_storage::Storage>) -> Self {
        Self {
            storage,
            config: Default::default(),
            drafts: Arc::default(),
            clients: Arc::default(),
        }
    }
    pub fn storage(&self) -> Result<&codex2api_storage::Storage> {
        self.storage
            .as_ref()
            .ok_or_else(|| problem("Antigravity storage unavailable"))
    }
    /// Explicit composition/test override; never resolved from an inbound request.
    pub fn with_config(mut self, config: AntigravityConfig) -> Self {
        self.config = config;
        self
    }
    pub fn config(&self) -> &AntigravityConfig {
        &self.config
    }
    pub async fn account_http(&self, id: &str) -> Result<Arc<AntigravityHttpClients>> {
        let snapshot = self.storage()?.supplier_transport_snapshot(id).await?;
        if snapshot.provider_id != codex2api_common::ANTIGRAVITY {
            return Err(AuthError::AccountMismatch);
        }
        let current = snapshot.clone();
        Ok(self
            .clients
            .get_or_update(
                id,
                |old| {
                    Arc::ptr_eq(&old.snapshot, &current)
                        || old.snapshot.cache_generation > current.cache_generation
                },
                |old| async move {
                    let identity = AntigravityIdentity::from_transport(&snapshot)?;
                    let proxy = snapshot.proxy_url.as_deref();
                    if let Some(old) = &old
                        && old.clients.identity == identity
                        && old.clients.proxy.as_deref() == proxy
                    {
                        return Ok(CachedAccountHttp {
                            snapshot,
                            clients: old.clients.clone(),
                        });
                    }
                    let mut client = AntigravityHttpClients::with_proxy(&identity, proxy)?;
                    if let Some(old) = old {
                        client.refresh_lock = old.clients.refresh_lock.clone();
                    }
                    Ok::<_, AuthError>(CachedAccountHttp {
                        snapshot,
                        clients: Arc::new(client),
                    })
                },
            )
            .await?
            .clients)
    }
    pub async fn evict_account_http(&self, id: &str) {
        self.clients.remove(id);
    }

    async fn draft_http(
        &self,
        identity: &AntigravityIdentity,
        proxy: Option<&str>,
    ) -> Result<AntigravityHttpClients> {
        let proxy = match proxy {
            Some(id) => Some(self.storage()?.require_outbound_proxy(id).await?),
            None => None,
        };
        AntigravityHttpClients::with_proxy(identity, proxy.as_ref().map(|p| p.url.as_str()))
    }

    async fn token(
        &self,
        http: &Client,
        identity: &AntigravityIdentity,
        fields: &[(&str, &str)],
        refresh: bool,
    ) -> Result<Tokens> {
        let response = http
            .post(&self.config.token_url)
            .headers(headers(identity, None)?)
            .timeout(Duration::from_secs(30))
            .form(fields)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            let value: Value = response.json().await.unwrap_or_default();
            let code = value["error"]
                .as_str()
                .filter(|v| {
                    v.len() <= 128 && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                })
                .map(str::to_owned);
            // Never include the remote body: it can echo credentials.
            return Err(if refresh {
                AuthError::RefreshRejected {
                    status: status.as_u16(),
                    code,
                    message: "Google 拒绝刷新凭据".into(),
                }
            } else {
                AuthError::TokenEndpoint {
                    status: status.as_u16(),
                    message: "Google 授权失败".into(),
                }
            });
        }
        let tokens: Tokens = response.json().await?;
        if tokens.access_token.is_empty() || tokens.expires_in.is_some_and(|n| n <= 0) {
            return Err(problem("Google 返回无效凭据"));
        }
        Ok(tokens)
    }

    async fn userinfo(
        &self,
        http: &Client,
        identity: &AntigravityIdentity,
        access: &str,
    ) -> Result<Value> {
        let response = http
            .get(&self.config.userinfo_url)
            .headers(headers(identity, Some(access))?)
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(AuthError::TokenEndpoint {
                status: response.status().as_u16(),
                message: "无法验证 Google 账户身份".into(),
            });
        }
        let profile: Value = response.json().await?;
        owner(&profile)?;
        Ok(profile)
    }

    /// One desktop-client service call, moving to the next root when this one is unreachable.
    pub async fn service_call(
        &self,
        http: &Client,
        identity: &AntigravityIdentity,
        access: &str,
        paid: bool,
        path: &str,
        body: &Value,
    ) -> Result<Value> {
        let mut last = None;
        for root in self.config.service_roots(paid) {
            let response = http
                .post(format!("{}{path}", root.trim_end_matches('/')))
                .headers(headers(identity, Some(access))?)
                .timeout(Duration::from_secs(30))
                .json(body)
                .send()
                .await;
            let response = match response {
                Ok(response) => response,
                Err(error) if error.is_connect() || error.is_timeout() => {
                    last = Some(AuthError::Http(error));
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let status = response.status();
            if unroutable(status) {
                last = Some(AuthError::Service {
                    status: status.as_u16(),
                    message: "Cloud Code Assist 服务不可用".into(),
                });
                continue;
            }
            if !status.is_success() {
                return Err(AuthError::Service {
                    status: status.as_u16(),
                    message: "Cloud Code Assist 拒绝了请求".into(),
                });
            }
            return Ok(response.json().await?);
        }
        Err(last.unwrap_or_else(|| problem("Antigravity 没有可用的服务地址")))
    }

    /// loadCodeAssist, then the desktop client's onboarding when no project exists yet.
    pub async fn code_assist(
        &self,
        http: &Client,
        identity: &AntigravityIdentity,
        access: &str,
        paid: bool,
    ) -> Result<CodeAssist> {
        let raw = self
            .service_call(
                http,
                identity,
                access,
                paid,
                wire::PATH_LOAD_CODE_ASSIST,
                &json!({"metadata":{"ideType":wire::IDE_TYPE,"ideVersion":wire::VERSION,"ideName":wire::IDE_NAME}}),
            )
            .await?;
        let tier_id = |value: &Value| {
            value
                .as_str()
                .or_else(|| value["id"].as_str())
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
        };
        let tier = tier_id(&raw["paidTier"]).or_else(|| tier_id(&raw["currentTier"]));
        let mut project = raw["cloudaicompanionProject"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned);
        if project.is_none()
            && let Some(default_tier) = raw["allowedTiers"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|t| t["isDefault"] == true)
                .and_then(|t| tier_id(&t["id"]))
        {
            let body = json!({"tierId":default_tier,"metadata":{"ideType":wire::IDE_TYPE,"platform":wire::ONBOARD_PLATFORM,"pluginType":wire::ONBOARD_PLUGIN_TYPE}});
            for _ in 0..5 {
                let onboarded = self
                    .service_call(http, identity, access, paid, wire::PATH_ONBOARD_USER, &body)
                    .await?;
                if onboarded["done"] == true {
                    let value = &onboarded["response"]["cloudaicompanionProject"];
                    project = value
                        .as_str()
                        .or_else(|| value["id"].as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned);
                    break;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
        Ok(CodeAssist { project, tier, raw })
    }

    fn credentials(
        tokens: Tokens,
        previous_refresh: Option<&str>,
        assist: &CodeAssist,
        profile: Value,
    ) -> AntigravityCredentials {
        AntigravityCredentials {
            access_token: tokens.access_token,
            refresh_token: tokens
                .refresh_token
                .or_else(|| previous_refresh.map(str::to_owned)),
            expires_at: tokens
                .expires_in
                .map(|ttl| Utc::now().timestamp().saturating_add(ttl)),
            project_id: assist.project.clone(),
            tier: assist.tier.clone(),
            profile,
        }
    }

    async fn save(
        &self,
        identity: &AntigravityIdentity,
        proxy: Option<&str>,
        expected: Option<&str>,
        tokens: Tokens,
        previous_refresh: Option<&str>,
        http: &Client,
    ) -> Result<SupplierAccount> {
        let profile = self.userinfo(http, identity, &tokens.access_token).await?;
        let email = owner(&profile)?;
        if let Some(id) = expected {
            let old = self.storage()?.require_account(id).await?;
            if old.provider_id != codex2api_common::ANTIGRAVITY
                || old.chatgpt_user_id.as_deref() != Some(&email)
            {
                return Err(AuthError::AccountMismatch);
            }
        }
        // Onboarding can be retried later from the account page; the login itself stands.
        let assist = match self
            .code_assist(http, identity, &tokens.access_token, false)
            .await
        {
            Ok(assist) => assist,
            Err(error) => {
                tracing::warn!(%error, "Antigravity onboarding deferred after login");
                CodeAssist {
                    project: None,
                    tier: None,
                    raw: Value::Null,
                }
            }
        };
        let mut new = NewSupplierAccount::pending_identity(
            &identity.installation_id,
            &identity.os_type,
            &identity.os_version,
            &identity.arch,
            identity.fingerprint_json()?,
        );
        new.id = Some(identity.account_id.clone());
        new.provider_id = codex2api_common::ANTIGRAVITY.into();
        new.chatgpt_account_id = Some(principal(&email));
        new.chatgpt_user_id = Some(email.clone());
        new.email = Some(email.clone());
        new.display_name = profile["name"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
            .or(Some(email));
        new.plan_type = assist
            .tier
            .as_deref()
            .map(|tier| wire::tier_display(tier).to_owned());
        let auth = Self::credentials(tokens, previous_refresh, &assist, profile);
        let account = self
            .storage()?
            .save_authorized_account(new, auth.to_supplier_tokens(&identity.account_id), proxy)
            .await?;
        if !assist.raw.is_null() {
            self.storage()?
                .store_supplier_info(
                    &account.id,
                    codex2api_storage::SupplierInfoSection::Details,
                    &codex2api_storage::QuotaSnapshot::new(assist.raw, Utc::now()),
                )
                .await?;
        }
        self.evict_account_http(&account.id).await;
        Ok(account)
    }

    pub async fn login_rt(
        &self,
        identity: AntigravityIdentity,
        proxy: Option<&str>,
        expected: Option<&str>,
        refresh: &str,
    ) -> Result<SupplierAccount> {
        let refresh = refresh.trim();
        if refresh.is_empty() || refresh.len() > 4096 || refresh.chars().any(char::is_control) {
            return Err(problem("请输入有效的 Google Refresh Token"));
        }
        let http = self.draft_http(&identity, proxy).await?;
        let tokens = self
            .token(
                &http.client,
                &identity,
                &[
                    ("grant_type", "refresh_token"),
                    ("client_id", wire::CLIENT_ID),
                    ("client_secret", wire::CLIENT_SECRET),
                    ("refresh_token", refresh),
                ],
                true,
            )
            .await?;
        self.save(
            &identity,
            proxy,
            expected,
            tokens,
            Some(refresh),
            &http.client,
        )
        .await
    }

    /// The desktop client's PKCE authorization; the browser result is pasted back.
    pub async fn begin(
        &self,
        identity: AntigravityIdentity,
        proxy: Option<&str>,
        expected: Option<&str>,
    ) -> Result<Value> {
        let http = self.draft_http(&identity, proxy).await?;
        let verifier = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>());
        let challenge =
            URL_SAFE_NO_PAD.encode(<sha2::Sha256 as sha2::Digest>::digest(verifier.as_bytes()));
        let state = format!("antigravity_{}", codex2api_storage::oauth_secret());
        let mut url = url::Url::parse(&self.config.authorize_url).map_err(anyhow::Error::from)?;
        url.query_pairs_mut().extend_pairs([
            ("client_id", wire::CLIENT_ID),
            ("redirect_uri", wire::REDIRECT_URI),
            ("response_type", "code"),
            ("scope", wire::SCOPE),
            ("state", state.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("include_granted_scopes", "true"),
        ]);
        let mut drafts = self.drafts.lock().await;
        drafts.retain(|_, d| {
            d.try_lock()
                .map_or(true, |d| d.active && d.expires > Instant::now())
        });
        drafts.insert(
            state.clone(),
            Arc::new(tokio::sync::Mutex::new(Draft {
                expires: Instant::now() + Duration::from_secs(600),
                identity,
                proxy_id: proxy.map(str::to_owned),
                expected: expected.map(str::to_owned),
                http,
                verifier,
                active: true,
            })),
        );
        Ok(
            json!({"status":"pending","method":"callback","state":state,"authorize_url":url.as_str()}),
        )
    }

    pub async fn cancel(&self, state: &str) {
        let draft = self.drafts.lock().await.remove(state);
        if let Some(draft) = draft {
            draft.lock().await.active = false;
        }
    }

    pub async fn complete(&self, state: &str, callback: &str) -> Result<SupplierAccount> {
        let draft = self
            .drafts
            .lock()
            .await
            .get(state)
            .cloned()
            .ok_or(AuthError::PendingNotFound)?;
        let mut d = draft.lock().await;
        if !d.active || d.expires <= Instant::now() {
            return Err(AuthError::PendingNotFound);
        }
        let code = callback_code(callback, state)?;
        let tokens = self
            .token(
                &d.http.client,
                &d.identity,
                &[
                    ("grant_type", "authorization_code"),
                    ("client_id", wire::CLIENT_ID),
                    ("client_secret", wire::CLIENT_SECRET),
                    ("code", &code),
                    ("redirect_uri", wire::REDIRECT_URI),
                    ("code_verifier", &d.verifier),
                ],
                false,
            )
            .await?;
        let account = self
            .save(
                &d.identity,
                d.proxy_id.as_deref(),
                d.expected.as_deref(),
                tokens,
                None,
                &d.http.client,
            )
            .await?;
        d.active = false;
        drop(d);
        self.drafts.lock().await.remove(state);
        Ok(account)
    }

    /// Current credentials, refreshed when expiring or when the given access token was rejected.
    pub async fn refresh(
        &self,
        id: &str,
        force: bool,
        rejected: Option<&str>,
    ) -> Result<Arc<AntigravityCredentials>> {
        let snapshot = self
            .storage()?
            .supplier_auth_snapshot(id)
            .await?
            .ok_or_else(|| AuthError::TokensNotFound(id.into()))?;
        if snapshot.provider_id != codex2api_common::ANTIGRAVITY {
            return Err(AuthError::AccountMismatch);
        }
        let current =
            snapshot.decode(|| AntigravityCredentials::from_supplier_tokens(&snapshot.tokens))?;
        if rejected.is_some_and(|v| v != current.access_token) {
            return Ok(current);
        }
        let valid = |expires: Option<i64>| expires.is_none_or(|n| n > Utc::now().timestamp() + 300);
        if !force && valid(current.expires_at) {
            return Ok(current);
        }
        let http = self.account_http(id).await?;
        let _guard = http.refresh_lock.lock().await;
        let snapshot = self
            .storage()?
            .supplier_auth_snapshot(id)
            .await?
            .ok_or_else(|| AuthError::TokensNotFound(id.into()))?;
        let current =
            snapshot.decode(|| AntigravityCredentials::from_supplier_tokens(&snapshot.tokens))?;
        if rejected.is_some_and(|v| v != current.access_token)
            || (!force && valid(current.expires_at))
        {
            return Ok(current);
        }
        let rt = current
            .refresh_token
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or(AuthError::MissingRefreshToken)?;
        let tokens = match self
            .token(
                &http.client,
                &http.identity,
                &[
                    ("grant_type", "refresh_token"),
                    ("client_id", wire::CLIENT_ID),
                    ("client_secret", wire::CLIENT_SECRET),
                    ("refresh_token", rt),
                ],
                true,
            )
            .await
        {
            Ok(tokens) => tokens,
            Err(error) => {
                if error.permanent_refresh_failure() {
                    self.storage()?
                        .reject_supplier_auth(id, snapshot.auth_revision)
                        .await?;
                }
                return Err(error);
            }
        };
        let auth = AntigravityCredentials {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token.or_else(|| Some(rt.to_owned())),
            expires_at: tokens
                .expires_in
                .map(|ttl| Utc::now().timestamp().saturating_add(ttl)),
            project_id: current.project_id.clone(),
            tier: current.tier.clone(),
            profile: current.profile.clone(),
        };
        if !self
            .storage()?
            .replace_supplier_tokens(id, snapshot.auth_revision, auth.to_supplier_tokens(id))
            .await?
        {
            return AntigravityCredentials::from_supplier_tokens(
                &self
                    .storage()?
                    .load_supplier_tokens(id)
                    .await?
                    .ok_or_else(|| AuthError::TokensNotFound(id.into()))?,
            )
            .map(Arc::new);
        }
        Ok(Arc::new(auth))
    }

    /// Explicit onboarding refresh: project, tier and the official details snapshot.
    pub async fn refresh_profile(&self, id: &str) -> Result<Value> {
        let current = self.refresh(id, false, None).await?;
        let http = self.account_http(id).await?;
        let snapshot = self
            .storage()?
            .supplier_auth_snapshot(id)
            .await?
            .ok_or_else(|| AuthError::TokensNotFound(id.into()))?;
        let assist = self
            .code_assist(
                &http.client,
                &http.identity,
                &current.access_token,
                current.paid(),
            )
            .await?;
        let auth = AntigravityCredentials {
            project_id: assist
                .project
                .clone()
                .or_else(|| current.project_id.clone()),
            tier: assist.tier.clone().or_else(|| current.tier.clone()),
            ..(*current).clone()
        };
        self.storage()?
            .replace_supplier_tokens(id, snapshot.auth_revision, auth.to_supplier_tokens(id))
            .await?;
        self.storage()?
            .update_supplier_profile(
                id,
                snapshot.auth_revision,
                None,
                auth.tier.as_deref().map(wire::tier_display),
            )
            .await?;
        self.storage()?
            .store_supplier_info(
                id,
                codex2api_storage::SupplierInfoSection::Details,
                &codex2api_storage::QuotaSnapshot::new(assist.raw.clone(), Utc::now()),
            )
            .await?;
        Ok(assist.raw)
    }
}

/// A bare authorization code or the full loopback callback URL from the browser.
fn callback_code(raw: &str, state: &str) -> Result<String> {
    let raw = raw.trim();
    if raw.is_empty()
        || raw.len() > 16384
        || raw.chars().any(char::is_whitespace)
        || raw.chars().any(char::is_control)
    {
        return Err(problem("请输入 Google 返回的授权代码或完整回调链接"));
    }
    if !raw.contains("://") && !raw.contains(['?', '#']) {
        return Ok(raw.to_owned());
    }
    let mut url =
        url::Url::parse(raw).map_err(|_| problem("请输入 Google 返回的授权代码或完整回调链接"))?;
    let query = url.query_pairs().into_owned().collect::<Vec<_>>();
    url.set_query(None);
    if url.as_str() != wire::REDIRECT_URI || url.fragment().is_some() {
        return Err(AuthError::StateMismatch);
    }
    let mut fields = HashMap::new();
    for (key, value) in query {
        if fields.insert(key, value).is_some() {
            return Err(AuthError::StateMismatch);
        }
    }
    if fields.get("state").map(String::as_str) != Some(state) {
        return Err(AuthError::StateMismatch);
    }
    fields
        .remove("code")
        .filter(|s| !s.is_empty())
        .ok_or(AuthError::MissingCode)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_accepts_bare_codes_and_only_the_desktop_loopback_redirect() {
        assert_eq!(
            callback_code("4/0Ab_code-value", "s1").unwrap(),
            "4/0Ab_code-value"
        );
        assert_eq!(
            callback_code(
                "http://localhost:8085/callback?state=s1&code=4%2F0Ab&scope=email",
                "s1"
            )
            .unwrap(),
            "4/0Ab"
        );
        assert!(callback_code("http://localhost:8085/callback?state=other&code=x", "s1").is_err());
        assert!(callback_code("http://localhost:9999/callback?state=s1&code=x", "s1").is_err());
        assert!(callback_code("http://localhost:8085/callback?state=s1", "s1").is_err());
    }
}
