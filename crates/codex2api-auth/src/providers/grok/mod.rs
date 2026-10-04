//! Grok Build supplier OAuth. Tokens, refresh locks and transports stay account-scoped.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use codex2api_accounts::providers::grok::GrokIdentity as AccountIdentity;
use codex2api_storage::{NewSupplierAccount, SupplierAccount};
use codex2api_version::grok as wire;
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
mod credentials;
mod error;
#[cfg(test)]
mod tests;
pub mod transport;
use GrokAuthError as AuthError;
pub use credentials::GrokCredentials;
pub use error::GrokAuthError;
pub type Result<T> = std::result::Result<T, GrokAuthError>;
use transport::GrokHttpClients;

#[derive(Clone)]
pub struct GrokAuthService {
    storage: Option<codex2api_storage::Storage>,
    grok_config: GrokConfig,
    grok_drafts: Arc<tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<Draft>>>>>,
    clients: Arc<tokio::sync::Mutex<HashMap<String, Arc<GrokHttpClients>>>>,
}

#[derive(Clone)]
pub struct GrokConfig {
    pub issuer: String,
    pub base_url: String,
}
impl Default for GrokConfig {
    fn default() -> Self {
        Self {
            issuer: wire::ISSUER.into(),
            base_url: wire::BASE_URL.into(),
        }
    }
}

pub(crate) struct Draft {
    expires: Instant,
    identity: AccountIdentity,
    proxy_id: Option<String>,
    expected: Option<String>,
    http: GrokHttpClients,
    verifier: String,
    nonce: String,
    redirect: String,
    device: Option<String>,
    interval: u64,
    last_poll: Instant,
    active: bool,
    authorized: Option<Tokens>,
}

#[derive(Clone, Deserialize)]
struct Tokens {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

pub fn headers(identity: &AccountIdentity, access: Option<&str>) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "user-agent",
        HeaderValue::from_str(&wire::user_agent(&identity.os_type, &identity.arch))
            .map_err(anyhow::Error::from)?,
    );
    headers.insert(
        "x-grok-client-version",
        HeaderValue::from_static(wire::VERSION),
    );
    headers.insert(
        "x-grok-client-identifier",
        HeaderValue::from_static("grok-pager"),
    );
    headers.insert(
        "x-grok-client-mode",
        HeaderValue::from_static("interactive"),
    );
    if let Some(access) = access {
        headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {access}")).map_err(anyhow::Error::from)?,
        );
        headers.insert(
            "x-xai-token-auth",
            HeaderValue::from_static(wire::TOKEN_AUTH),
        );
    }
    Ok(headers)
}

fn problem(message: &str) -> AuthError {
    anyhow::anyhow!("{message}").into()
}

fn subscription_display(profile: &Value) -> Option<&str> {
    profile["subscriptionTierDisplay"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            profile["subscriptionTier"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .map(|tier| {
                    codex2api_core::providers::grok::subscription(tier).map_or(tier, |s| s.display)
                })
        })
}

fn owner(profile: &Value) -> Result<(String, String)> {
    let user = profile
        .get("userId")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| problem("Grok 未返回用户身份"))?;
    let principal = if profile["principalType"].as_str() == Some("Team") {
        let team = profile["teamId"]
            .as_str()
            .or_else(|| profile["principalId"].as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| problem("Grok 未返回团队身份"))?;
        format!("team:{team}")
    } else {
        format!("user:{user}")
    };
    Ok((principal, user.to_owned()))
}

impl GrokAuthService {
    pub fn new(storage: Option<codex2api_storage::Storage>) -> Self {
        Self {
            storage,
            grok_config: Default::default(),
            grok_drafts: Arc::default(),
            clients: Arc::default(),
        }
    }
    pub fn storage(&self) -> Result<&codex2api_storage::Storage> {
        self.storage
            .as_ref()
            .ok_or_else(|| problem("Grok storage unavailable"))
    }
    pub async fn account_http(&self, id: &str) -> Result<Arc<GrokHttpClients>> {
        let account = self.storage()?.require_account(id).await?;
        if account.provider_id != codex2api_core::GROK {
            return Err(AuthError::AccountMismatch);
        }
        let identity = AccountIdentity::from_account(&account);
        let proxy = match account.proxy_id {
            Some(id) => Some(self.storage()?.require_outbound_proxy(&id).await?.url),
            None => None,
        };
        let mut clients = self.clients.lock().await;
        if let Some(client) = clients.get(id)
            && client.identity == identity
            && client.proxy == proxy
        {
            return Ok(client.clone());
        }
        let mut client = GrokHttpClients::with_proxy(&identity, proxy.as_deref())?;
        if let Some(old) = clients.get(id) {
            client.refresh_lock = old.refresh_lock.clone();
        }
        let client = Arc::new(client);
        clients.insert(id.into(), client.clone());
        Ok(client)
    }
    pub async fn evict_account_http(&self, id: &str) {
        self.clients.lock().await.remove(id);
    }

    /// Explicit composition/test override; never resolved from an inbound request.
    pub fn with_grok_config(mut self, config: GrokConfig) -> Self {
        self.grok_config = config;
        self
    }
    pub fn grok_config(&self) -> &GrokConfig {
        &self.grok_config
    }

    async fn grok_token(
        &self,
        http: &Client,
        identity: &AccountIdentity,
        fields: &[(&str, &str)],
        refresh: bool,
    ) -> Result<Tokens> {
        let response = http
            .post(format!(
                "{}/oauth2/token",
                self.grok_config.issuer.trim_end_matches('/')
            ))
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
            // Never include a remote response body: it can echo credentials.
            return Err(if refresh {
                AuthError::RefreshRejected {
                    status: status.as_u16(),
                    code,
                    message: "Grok 拒绝刷新凭据".into(),
                }
            } else {
                AuthError::TokenEndpoint {
                    status: status.as_u16(),
                    message: "Grok 授权失败".into(),
                }
            });
        }
        let tokens: Tokens = response.json().await?;
        if tokens.access_token.is_empty() || tokens.expires_in.is_some_and(|n| n <= 0) {
            return Err(problem("Grok 返回无效凭据"));
        }
        Ok(tokens)
    }

    async fn grok_profile(
        &self,
        http: &Client,
        identity: &AccountIdentity,
        token: &str,
    ) -> Result<Value> {
        let response = http
            .get(format!(
                "{}/user?include=subscription",
                self.grok_config.base_url.trim_end_matches('/')
            ))
            .headers(headers(identity, Some(token))?)
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(AuthError::TokenEndpoint {
                status: response.status().as_u16(),
                message: "无法验证 Grok 账户身份".into(),
            });
        }
        let mut profile: Value = response.json().await?;
        owner(&profile)?;
        // Grok Build displays /settings.subscription_tier_display. The /user tier
        // can be absent or an internal enum and must not replace that product label.
        let settings = http
            .get(format!(
                "{}/settings",
                self.grok_config.base_url.trim_end_matches('/')
            ))
            .headers(headers(identity, Some(token))?)
            .timeout(Duration::from_secs(15))
            .send()
            .await;
        if let Ok(response) = settings
            && response.status().is_success()
            && let Ok(settings) = response.json::<Value>().await
        {
            if let Some(display) = settings["subscription_tier_display"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
            {
                profile["subscriptionTierDisplay"] = display.into();
            }
            if profile["subscriptionTier"]
                .as_str()
                .is_none_or(|s| s.trim().is_empty())
                && let Some(tier) = settings["subscription_tier"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
            {
                profile["subscriptionTier"] = tier.into();
            }
        }
        Ok(profile)
    }

    /// Explicit administrator refresh also repairs older rows with incomplete profile data.
    pub async fn refresh_profile(&self, id: &str) -> Result<Value> {
        self.refresh_grok(id, false, None).await?;
        let http = self.account_http(id).await?;
        let snapshot = self
            .storage()?
            .supplier_auth_snapshot(id)
            .await?
            .ok_or_else(|| AuthError::TokensNotFound(id.into()))?;
        let account = self.storage()?.require_account(id).await?;
        let profile = self
            .grok_profile(
                &http.raw,
                &http.identity,
                snapshot
                    .tokens
                    .access_token
                    .as_deref()
                    .ok_or_else(|| AuthError::TokensNotFound(id.into()))?,
            )
            .await?;
        let (principal, user) = owner(&profile)?;
        if account.chatgpt_account_id.as_deref() != Some(principal.as_str())
            || account.chatgpt_user_id.as_deref() != Some(user.as_str())
        {
            return Err(AuthError::AccountMismatch);
        }
        self.storage()?
            .update_supplier_profile(
                id,
                snapshot.auth_revision,
                profile["email"].as_str(),
                subscription_display(&profile),
            )
            .await?;
        self.storage()?
            .store_supplier_info(
                id,
                codex2api_storage::SupplierInfoSection::Details,
                &codex2api_storage::QuotaSnapshot {
                    value: profile.clone(),
                    observed_at: Utc::now(),
                },
            )
            .await?;
        Ok(profile)
    }

    async fn grok_verify_id(&self, http: &Client, jwt: &str, nonce: Option<&str>) -> Result<()> {
        let parts: Vec<_> = jwt.split('.').collect();
        if parts.len() != 3 {
            return Err(AuthError::InvalidIdToken);
        }
        let decode = |s: &str| {
            URL_SAFE_NO_PAD
                .decode(s)
                .map_err(|_| AuthError::InvalidIdToken)
        };
        let header: Value = serde_json::from_slice(&decode(parts[0])?)?;
        if header["alg"] != "ES256" || header.get("jku").is_some() || header.get("x5u").is_some() {
            return Err(AuthError::InvalidIdToken);
        }
        let kid = header["kid"].as_str().ok_or(AuthError::InvalidIdToken)?;
        let keys: Value = http
            .get(format!(
                "{}/.well-known/jwks.json",
                self.grok_config.issuer.trim_end_matches('/')
            ))
            .timeout(Duration::from_secs(15))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let key = keys["keys"]
            .as_array()
            .and_then(|keys| {
                keys.iter()
                    .find(|k| k["kid"] == kid && k["kty"] == "EC" && k["crv"] == "P-256")
            })
            .ok_or(AuthError::InvalidIdToken)?;
        let x = decode(key["x"].as_str().ok_or(AuthError::InvalidIdToken)?)?;
        let y = decode(key["y"].as_str().ok_or(AuthError::InvalidIdToken)?)?;
        if x.len() != 32 || y.len() != 32 {
            return Err(AuthError::InvalidIdToken);
        }
        let mut public = vec![4];
        public.extend(x);
        public.extend(y);
        ring::signature::UnparsedPublicKey::new(&ring::signature::ECDSA_P256_SHA256_FIXED, public)
            .verify(
                format!("{}.{}", parts[0], parts[1]).as_bytes(),
                &decode(parts[2])?,
            )
            .map_err(|_| AuthError::InvalidIdToken)?;
        let claims: Value = serde_json::from_slice(&decode(parts[1])?)?;
        let now = Utc::now().timestamp();
        let audience = claims["aud"] == wire::CLIENT_ID
            || claims["aud"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v == wire::CLIENT_ID));
        if claims["iss"] != self.grok_config.issuer
            || !audience
            || claims["exp"].as_i64().is_none_or(|n| n <= now)
            || claims["nbf"].as_i64().is_some_and(|n| n > now + 30)
            || nonce.is_some_and(|nonce| claims["nonce"].as_str() != Some(nonce))
        {
            return Err(AuthError::InvalidIdToken);
        }
        Ok(())
    }

    fn grok_auth(
        tokens: Tokens,
        profile: Value,
        previous_refresh: Option<&str>,
    ) -> GrokCredentials {
        GrokCredentials {
            access_token: tokens.access_token,
            refresh_token: tokens
                .refresh_token
                .or_else(|| previous_refresh.map(str::to_owned)),
            id_token: tokens.id_token,
            expires_at: tokens
                .expires_in
                .map(|ttl| Utc::now().timestamp().saturating_add(ttl)),
            profile,
        }
    }

    async fn save_grok(
        &self,
        identity: &AccountIdentity,
        proxy: Option<&str>,
        expected: Option<&str>,
        tokens: Tokens,
        previous_refresh: Option<&str>,
        http: &Client,
    ) -> Result<SupplierAccount> {
        let profile = self
            .grok_profile(http, identity, &tokens.access_token)
            .await?;
        let (principal, user) = owner(&profile)?;
        if let Some(id) = expected {
            let old = self.storage()?.require_account(id).await?;
            if old.provider_id != codex2api_core::GROK
                || old.chatgpt_account_id.as_deref() != Some(&principal)
                || old.chatgpt_user_id.as_deref() != Some(&user)
            {
                return Err(AuthError::AccountMismatch);
            }
        }
        let mut new = NewSupplierAccount::pending_identity(
            &identity.installation_id,
            &identity.originator,
            &identity.user_agent,
            &identity.os_type,
            &identity.os_version,
            &identity.arch,
            "",
            identity.fingerprint_json()?,
        );
        new.id = Some(identity.account_id.clone());
        new.provider_id = codex2api_core::GROK.into();
        new.chatgpt_account_id = Some(principal);
        new.chatgpt_user_id = Some(user);
        new.email = profile["email"].as_str().map(str::to_owned);
        new.display_name = profile["firstName"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .or_else(|| new.email.clone());
        new.plan_type = subscription_display(&profile).map(str::to_owned);
        let auth = Self::grok_auth(tokens, profile.clone(), previous_refresh);
        let account = self
            .storage()?
            .save_authorized_account(new, auth.to_supplier_tokens(&identity.account_id), proxy)
            .await?;
        self.storage()?
            .store_supplier_info(
                &account.id,
                codex2api_storage::SupplierInfoSection::Details,
                &codex2api_storage::QuotaSnapshot {
                    value: profile,
                    observed_at: Utc::now(),
                },
            )
            .await?;
        self.evict_account_http(&account.id).await;
        Ok(account)
    }

    pub async fn grok_login_rt(
        &self,
        identity: AccountIdentity,
        proxy: Option<&str>,
        expected: Option<&str>,
        refresh: &str,
    ) -> Result<SupplierAccount> {
        let refresh = refresh.trim();
        if refresh.is_empty() || refresh.len() > 32768 || refresh.chars().any(char::is_control) {
            return Err(problem("请输入有效的 Grok Refresh Token"));
        }
        let http = self.grok_draft_http(&identity, proxy).await?;
        let tokens = self
            .grok_token(
                &http.raw,
                &identity,
                &[
                    ("grant_type", "refresh_token"),
                    ("client_id", wire::CLIENT_ID),
                    ("refresh_token", refresh),
                ],
                true,
            )
            .await?;
        if let Some(jwt) = &tokens.id_token {
            self.grok_verify_id(&http.raw, jwt, None).await?;
        }
        self.save_grok(&identity, proxy, expected, tokens, Some(refresh), &http.raw)
            .await
    }

    async fn grok_draft_http(
        &self,
        identity: &AccountIdentity,
        proxy: Option<&str>,
    ) -> Result<GrokHttpClients> {
        let proxy = match proxy {
            Some(id) => Some(self.storage()?.require_outbound_proxy(id).await?),
            None => None,
        };
        GrokHttpClients::with_proxy(identity, proxy.as_ref().map(|p| p.url.as_str()))
    }

    pub async fn grok_begin(
        &self,
        identity: AccountIdentity,
        proxy: Option<&str>,
        expected: Option<&str>,
        device: bool,
    ) -> Result<Value> {
        let http = self.grok_draft_http(&identity, proxy).await?;
        let verifier = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>());
        let challenge =
            URL_SAFE_NO_PAD.encode(<sha2::Sha256 as sha2::Digest>::digest(verifier.as_bytes()));
        let state = format!("grok_{}", codex2api_storage::oauth_secret());
        let nonce = codex2api_storage::oauth_secret();
        let redirect = format!(
            "http://127.0.0.1:{}/callback",
            rand::random_range(49152..=65535)
        );
        let mut ttl = 600;
        let mut interval = 5;
        let mut device_code = None;
        let view = if device {
            let response = http
                .raw
                .post(format!(
                    "{}/oauth2/device/code",
                    self.grok_config.issuer.trim_end_matches('/')
                ))
                .headers(headers(&identity, None)?)
                .header("x-grok-client-surface", "ui")
                .timeout(Duration::from_secs(30))
                .form(&[
                    ("client_id", wire::CLIENT_ID),
                    ("scope", wire::SCOPE),
                    ("referrer", "grok-build"),
                ])
                .send()
                .await?;
            if !response.status().is_success() {
                return Err(problem("Grok 设备授权请求失败"));
            }
            let value: Value = response.json().await?;
            device_code = Some(
                value["device_code"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| problem("Grok 设备授权响应无效"))?
                    .to_owned(),
            );
            ttl = value["expires_in"]
                .as_u64()
                .filter(|n| *n > 0)
                .unwrap_or(600)
                .min(3600);
            interval = value["interval"].as_u64().unwrap_or(5).clamp(1, 60);
            let url = value["verification_uri_complete"]
                .as_str()
                .or_else(|| value["verification_uri"].as_str())
                .ok_or_else(|| problem("Grok 设备授权地址缺失"))?;
            let parsed = url::Url::parse(url).map_err(|_| problem("Grok 设备授权地址无效"))?;
            if parsed.scheme() != "https" {
                return Err(problem("Grok 设备授权地址必须使用 HTTPS"));
            }
            json!({"status":"pending","method":"device","state":state,"verification_url":url,"user_code":value["user_code"],"interval":interval})
        } else {
            let mut url = url::Url::parse(&format!(
                "{}/oauth2/authorize",
                self.grok_config.issuer.trim_end_matches('/')
            ))
            .map_err(anyhow::Error::from)?;
            url.query_pairs_mut().extend_pairs([
                ("response_type", "code"),
                ("client_id", wire::CLIENT_ID),
                ("redirect_uri", redirect.as_str()),
                ("scope", wire::SCOPE),
                ("state", state.as_str()),
                ("nonce", nonce.as_str()),
                ("code_challenge", challenge.as_str()),
                ("code_challenge_method", "S256"),
                ("referrer", "grok-build"),
            ]);
            json!({"status":"pending","method":"callback","state":state,"authorize_url":url.as_str()})
        };
        let mut drafts = self.grok_drafts.lock().await;
        drafts.retain(|_, d| {
            d.try_lock()
                .map_or(true, |d| d.active && d.expires > Instant::now())
        });
        drafts.insert(
            state,
            Arc::new(tokio::sync::Mutex::new(Draft {
                expires: Instant::now() + Duration::from_secs(ttl),
                identity,
                proxy_id: proxy.map(str::to_owned),
                expected: expected.map(str::to_owned),
                http,
                verifier: verifier,
                nonce,
                redirect,
                device: device_code,
                interval,
                last_poll: Instant::now(),
                active: true,
                authorized: None,
            })),
        );
        Ok(view)
    }

    pub async fn grok_cancel(&self, state: &str) {
        let draft = self.grok_drafts.lock().await.remove(state);
        if let Some(draft) = draft {
            draft.lock().await.active = false;
        }
    }

    pub async fn grok_complete(
        &self,
        state: &str,
        callback: Option<&str>,
    ) -> Result<Option<SupplierAccount>> {
        let draft = self
            .grok_drafts
            .lock()
            .await
            .get(state)
            .cloned()
            .ok_or(AuthError::PendingNotFound)?;
        let mut d = draft.lock().await;
        if !d.active || d.expires <= Instant::now() {
            return Err(AuthError::PendingNotFound);
        }
        if d.authorized.is_none() {
            let tokens = if let Some(device) = &d.device {
                if callback.is_some() {
                    return Err(AuthError::StateMismatch);
                }
                if d.last_poll.elapsed() < Duration::from_secs(d.interval) {
                    return Ok(None);
                }
                let fields = [
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", wire::CLIENT_ID),
                    ("device_code", device.as_str()),
                ];
                let response = d
                    .http
                    .raw
                    .post(format!(
                        "{}/oauth2/token",
                        self.grok_config.issuer.trim_end_matches('/')
                    ))
                    .headers(headers(&d.identity, None)?)
                    .header("x-grok-client-surface", "ui")
                    .timeout(Duration::from_secs(30))
                    .form(&fields)
                    .send()
                    .await?;
                d.last_poll = Instant::now();
                let status = response.status();
                let value: Value = response.json().await?;
                if !status.is_success() {
                    match value["error"].as_str() {
                        Some("authorization_pending") => return Ok(None),
                        Some("slow_down") => {
                            d.interval = d.interval.saturating_add(5);
                            return Ok(None);
                        }
                        _ => {
                            d.active = false;
                            return Err(problem("Grok 设备授权被拒绝或已过期"));
                        }
                    }
                }
                serde_json::from_value::<Tokens>(value)?
            } else {
                let code =
                    callback_code(callback.ok_or(AuthError::MissingCode)?, &d.redirect, state)?;
                self.grok_token(
                    &d.http.raw,
                    &d.identity,
                    &[
                        ("grant_type", "authorization_code"),
                        ("client_id", wire::CLIENT_ID),
                        ("code", &code),
                        ("redirect_uri", &d.redirect),
                        ("code_verifier", &d.verifier),
                    ],
                    false,
                )
                .await?
            };
            if let Some(jwt) = &tokens.id_token {
                self.grok_verify_id(
                    &d.http.raw,
                    jwt,
                    d.device.is_none().then_some(d.nonce.as_str()),
                )
                .await?;
            } else if d.device.is_none() {
                return Err(AuthError::InvalidIdToken);
            }
            d.authorized = Some(tokens);
        }
        let account = self
            .save_grok(
                &d.identity,
                d.proxy_id.as_deref(),
                d.expected.as_deref(),
                d.authorized.clone().unwrap(),
                None,
                &d.http.raw,
            )
            .await?;
        d.active = false;
        drop(d);
        self.grok_drafts.lock().await.remove(state);
        Ok(Some(account))
    }

    pub async fn refresh_grok(
        &self,
        id: &str,
        force: bool,
        rejected: Option<&str>,
    ) -> Result<GrokCredentials> {
        let http = self.account_http(id).await?;
        let _guard = http.refresh_lock.lock().await;
        let account = self.storage()?.require_account(id).await?;
        if account.provider_id != codex2api_core::GROK {
            return Err(AuthError::AccountMismatch);
        }
        let snapshot = self
            .storage()?
            .supplier_auth_snapshot(id)
            .await?
            .ok_or_else(|| AuthError::TokensNotFound(id.into()))?;
        let current = GrokCredentials::from_supplier_tokens(&snapshot.tokens)?;
        let access = snapshot
            .tokens
            .access_token
            .as_deref()
            .ok_or_else(|| AuthError::TokensNotFound(id.into()))?;
        if rejected.is_some_and(|v| v != access) {
            return Ok(current);
        }
        let expires = current.expires_at;
        if !force && expires.is_none_or(|n| n > Utc::now().timestamp() + 300) {
            return Ok(current);
        }
        let rt = snapshot
            .tokens
            .refresh_token
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or(AuthError::MissingRefreshToken)?;
        let result = self
            .grok_token(
                &http.raw,
                &http.identity,
                &[
                    ("grant_type", "refresh_token"),
                    ("client_id", wire::CLIENT_ID),
                    ("refresh_token", rt),
                ],
                true,
            )
            .await;
        let tokens = match result {
            Ok(tokens) => tokens,
            Err(error) => {
                if matches!(&error, AuthError::RefreshRejected { code:Some(code), .. } if matches!(code.as_str(),"invalid_grant"|"invalid_client"))
                {
                    self.storage()?
                        .reject_supplier_auth(id, snapshot.auth_revision)
                        .await?;
                }
                return Err(error);
            }
        };
        if let Some(jwt) = &tokens.id_token {
            self.grok_verify_id(&http.raw, jwt, None).await?;
        }
        let profile = self
            .grok_profile(&http.raw, &http.identity, &tokens.access_token)
            .await?;
        let (principal, user) = owner(&profile)?;
        if account.chatgpt_account_id.as_deref() != Some(&principal)
            || account.chatgpt_user_id.as_deref() != Some(&user)
        {
            return Err(AuthError::AccountMismatch);
        }
        let auth = Self::grok_auth(tokens, profile, Some(rt));
        if !self
            .storage()?
            .replace_supplier_tokens(id, snapshot.auth_revision, auth.to_supplier_tokens(id))
            .await?
        {
            return GrokCredentials::from_supplier_tokens(
                &self
                    .storage()?
                    .load_supplier_tokens(id)
                    .await?
                    .ok_or_else(|| AuthError::TokensNotFound(id.into()))?,
            )
            .map_err(Into::into);
        }
        Ok(auth)
    }

    pub async fn revoke_grok(&self, id: &str) -> Result<()> {
        let http = self.account_http(id).await?;
        let _guard = http.refresh_lock.lock().await;
        let tokens = self
            .storage()?
            .supplier_auth_snapshot(id)
            .await?
            .ok_or_else(|| AuthError::TokensNotFound(id.into()))?;
        let token = tokens
            .tokens
            .refresh_token
            .as_deref()
            .filter(|s| !s.is_empty())
            .or(tokens.tokens.access_token.as_deref())
            .ok_or(AuthError::MissingRefreshToken)?;
        http.raw
            .post(format!(
                "{}/oauth2/revoke",
                self.grok_config.issuer.trim_end_matches('/')
            ))
            .headers(headers(&http.identity, None)?)
            .timeout(Duration::from_secs(30))
            .form(&[("token", token), ("client_id", wire::CLIENT_ID)])
            .send()
            .await?
            .error_for_status()?;
        self.storage()?
            .replace_supplier_tokens(
                id,
                tokens.auth_revision,
                codex2api_storage::SupplierTokens {
                    account_id: id.into(),
                    ..Default::default()
                },
            )
            .await?;
        Ok(())
    }
}

fn callback_code(raw: &str, redirect: &str, state: &str) -> Result<String> {
    let raw = raw.trim();
    if raw.is_empty()
        || raw.len() > 16384
        || raw.chars().any(char::is_whitespace)
        || raw.chars().any(char::is_control)
    {
        return Err(problem("请输入 Grok 官方返回的授权代码或完整回调链接"));
    }
    // grok-build's oidc/login.rs parse_pasted_input explicitly accepts a bare
    // authorization code. Its PKCE verifier and nonce still belong to this draft.
    if !raw.contains("://") && !raw.contains(['?', '#']) {
        return Ok(raw.to_owned());
    }
    let mut url = url::Url::parse(raw)
        .map_err(|_| problem("请输入 Grok 官方返回的授权代码或完整回调链接"))?;
    let query = url.query_pairs().into_owned().collect::<Vec<_>>();
    url.set_query(None);
    if url.as_str() != redirect || url.fragment().is_some() {
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
