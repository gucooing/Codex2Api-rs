//! Browser callback submission and the pinned Codex device-code protocol.
use crate::oauth::{
    CALLBACK_PATH, PkceCodes, build_authorize_url, exchange_code_for_tokens, generate_state,
    start_pending_login,
};
use crate::persist::{auth_from_exchanged, bind_completed_login, pending_from_account};
use crate::tokens::{parse_chatgpt_jwt_claims, token_set_from_auth};
use crate::{AuthError, AuthService, CallbackQuery, CompletedLogin, Result};
use base64::Engine;
use codex2api_accounts::PendingAccount;
use codex2api_storage::{DEFAULT_OAUTH_PENDING_TTL, OAuthPending};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum LoginFlow {
    Callback {
        authorize_url: String,
    },
    Device {
        verification_url: String,
        user_code: String,
        device_auth_id: String,
        interval: u64,
    },
}

#[derive(Deserialize)]
struct DeviceCode {
    device_auth_id: String,
    #[serde(alias = "usercode")]
    user_code: String,
    #[serde(default, deserialize_with = "parse_interval")]
    interval: u64,
}

fn parse_interval<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<u64, D::Error> {
    String::deserialize(deserializer)?
        .trim()
        .parse()
        .map_err(serde::de::Error::custom)
}

#[derive(Deserialize)]
pub(crate) struct DeviceAuthorization {
    pub(crate) authorization_code: String,
    pub(crate) code_verifier: String,
}

impl AuthService {
    pub fn login_flow(&self, pending: &OAuthPending) -> Result<LoginFlow> {
        if pending.flow_data_json == "{}" {
            let pkce = PkceCodes {
                code_verifier: pending.code_verifier.clone(),
                code_challenge: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(Sha256::digest(pending.code_verifier.as_bytes())),
            };
            return Ok(LoginFlow::Callback {
                authorize_url: build_authorize_url(
                    self.config(),
                    &pending.redirect_uri,
                    &pkce,
                    &pending.state,
                ),
            });
        }
        Ok(serde_json::from_str(&pending.flow_data_json)?)
    }

    pub async fn begin_login(&self) -> Result<OAuthPending> {
        self.begin_manual_login(None, false).await
    }

    pub async fn begin_relogin(&self, account_id: &str) -> Result<OAuthPending> {
        self.begin_manual_login(Some(account_id), false).await
    }

    pub async fn begin_manual_login(
        &self,
        account_id: Option<&str>,
        device: bool,
    ) -> Result<OAuthPending> {
        self.begin_manual_login_with_proxy(account_id, device, None)
            .await
    }

    pub async fn begin_manual_login_with_proxy(
        &self,
        account_id: Option<&str>,
        device: bool,
        proxy_id: Option<&str>,
    ) -> Result<OAuthPending> {
        let proxy_id = proxy_id.filter(|id| !id.is_empty());
        if account_id.is_none()
            && let Some(id) = proxy_id
        {
            self.storage()?.require_outbound_proxy(id).await?;
        }
        let account = match account_id {
            Some(id) => pending_from_account(self.accounts(), id).await?,
            None => self.accounts().create_pending().await?,
        };
        let result = async {
            if account_id.is_none() && proxy_id.is_some() {
                self.storage()?
                    .set_account_proxy(&account.account.id, proxy_id)
                    .await?;
            }
            self.start_flow(&account, device).await
        }
        .await;
        if result.is_err() && account_id.is_none() {
            let _ = self.accounts().abandon_pending(&account).await;
        }
        result
    }

    async fn start_flow(&self, account: &PendingAccount, device: bool) -> Result<OAuthPending> {
        let http = self.account_http(&account.account.id).await?;
        let (login, ttl) = self.prepare_login_flow(&http, device).await?;
        let pending = self
            .storage()?
            .insert_oauth_pending(
                &login.state,
                &login.code_verifier,
                &login.redirect_uri,
                Some(&account.account.id),
                ttl,
            )
            .await?;
        match self
            .storage()?
            .set_oauth_flow(&pending.state, &login.flow_data_json)
            .await
        {
            Ok(pending) => Ok(pending),
            Err(error) => {
                let _ = self.storage()?.delete_oauth_pending(&pending.state).await;
                Err(error.into())
            }
        }
    }

    pub(crate) async fn prepare_login_flow(
        &self,
        http: &crate::transport::AccountHttpClients,
        device: bool,
    ) -> Result<(OAuthPending, Duration)> {
        let (state, verifier, redirect, flow, ttl) = if device {
            let issuer = self.config().issuer.trim_end_matches('/');
            let response = http
                .raw
                .post(format!("{issuer}/api/accounts/deviceauth/usercode"))
                .timeout(Duration::from_secs(30))
                .json(&serde_json::json!({"client_id": self.config().client_id}))
                .send()
                .await?;
            if !response.status().is_success() {
                return Err(AuthError::token_endpoint(
                    response.status(),
                    "无法获取设备码",
                ));
            }
            let code: DeviceCode = response.json().await?;
            let flow = LoginFlow::Device {
                verification_url: format!("{issuer}/codex/device"),
                user_code: code.user_code,
                device_auth_id: code.device_auth_id,
                interval: code.interval,
            };
            (
                generate_state(),
                String::new(),
                format!("{issuer}/deviceauth/callback"),
                flow,
                Duration::from_secs(15 * 60),
            )
        } else {
            let login = start_pending_login(self.config(), self.config().callback_port);
            (
                login.state,
                login.pkce.code_verifier,
                login.redirect_uri,
                LoginFlow::Callback {
                    authorize_url: login.authorize_url,
                },
                DEFAULT_OAUTH_PENDING_TTL,
            )
        };
        let now = chrono::Utc::now();
        Ok((
            OAuthPending {
                state,
                code_verifier: verifier,
                redirect_uri: redirect,
                account_id: None,
                created_at: now.to_rfc3339(),
                expires_at: (now + chrono::Duration::from_std(ttl).unwrap()).to_rfc3339(),
                flow_data_json: serde_json::to_string(&flow)?,
                last_polled_at_ms: None,
            },
            ttl,
        ))
    }

    pub async fn complete_manual_callback(
        &self,
        state: &str,
        callback_url: &str,
    ) -> Result<CompletedLogin> {
        if let Some(draft) = self.draft_login(state).await {
            return self
                .complete_draft_callback(state, draft, callback_url)
                .await;
        }
        let pending = self.storage()?.require_oauth_pending(state).await?;
        if !matches!(self.login_flow(&pending)?, LoginFlow::Callback { .. }) {
            return Err(AuthError::Other(anyhow::anyhow!(
                "当前授权方式不是回调链接"
            )));
        }
        let query = parse_callback_url(callback_url, &pending.redirect_uri, state)?;
        let id = pending
            .account_id
            .as_deref()
            .ok_or(AuthError::PendingNotFound)?;
        let http = self.account_http(id).await?;
        let _guard = http.refresh_lock.lock().await;
        self.complete_from_callback(&query).await
    }

    pub async fn poll_device_login(&self, state: &str) -> Result<Option<CompletedLogin>> {
        if let Some(draft) = self.draft_login(state).await {
            return self.poll_draft_device(state, draft).await;
        }
        let initial = self.storage()?.require_oauth_pending(state).await?;
        let id = initial
            .account_id
            .as_deref()
            .ok_or(AuthError::PendingNotFound)?;
        let http = self.account_http(id).await?;
        let _guard = http.refresh_lock.lock().await;
        let pending = self.storage()?.require_oauth_pending(state).await?;
        let LoginFlow::Device {
            device_auth_id,
            user_code,
            interval,
            ..
        } = self.login_flow(&pending)?
        else {
            return Err(AuthError::Other(anyhow::anyhow!("当前授权方式不是设备码")));
        };
        if !self
            .storage()?
            .claim_oauth_poll(state, interval.max(1))
            .await?
        {
            return Ok(None);
        }
        let response = http
            .raw
            .post(format!(
                "{}/api/accounts/deviceauth/token",
                self.config().issuer.trim_end_matches('/')
            ))
            .timeout(Duration::from_secs(30))
            .json(&serde_json::json!({"device_auth_id": device_auth_id, "user_code": user_code}))
            .send()
            .await?;
        let status = response.status();
        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(AuthError::token_endpoint(status, "设备码授权失败"));
        }
        let code: DeviceAuthorization = response.json().await?;
        let exchanged = exchange_code_for_tokens(
            &http.raw,
            self.config(),
            &pending.redirect_uri,
            &code.code_verifier,
            &code.authorization_code,
        )
        .await?;
        let claims = parse_chatgpt_jwt_claims(&exchanged.id_token)?;
        let auth = auth_from_exchanged(&exchanged, &claims);
        let account = pending_from_account(self.accounts(), id).await?;
        let bound = bind_completed_login(self.accounts(), &account, &claims, &auth).await?;
        self.storage()?.delete_oauth_pending(state).await?;
        Ok(Some(CompletedLogin {
            account: bound.account,
            identity: bound.identity,
            reused_existing: bound.reused_existing,
            claims,
            tokens: token_set_from_auth(&auth),
            auth,
        }))
    }
}

pub(crate) fn parse_callback_url(raw: &str, redirect: &str, state: &str) -> Result<CallbackQuery> {
    let url = url::Url::parse(raw.trim())
        .map_err(|_| AuthError::Other(anyhow::anyhow!("请粘贴完整的回调链接")))?;
    let expected = url::Url::parse(redirect).map_err(anyhow::Error::from)?;
    if url.scheme() != expected.scheme()
        || url.host_str() != expected.host_str()
        || url.port_or_known_default() != expected.port_or_known_default()
        || url.path() != CALLBACK_PATH
        || url.path() != expected.path()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(AuthError::Other(anyhow::anyhow!(
            "回调地址与本次授权不匹配"
        )));
    }
    let mut query = CallbackQuery {
        path: CALLBACK_PATH.into(),
        ..Default::default()
    };
    for (name, value) in url.query_pairs() {
        let field = match name.as_ref() {
            "code" => &mut query.code,
            "state" => &mut query.state,
            "error" => &mut query.error,
            "error_description" => &mut query.error_description,
            _ => continue,
        };
        if field.replace(value.into_owned()).is_some() {
            return Err(AuthError::Other(anyhow::anyhow!("回调链接包含重复参数")));
        }
    }
    if query.state.as_deref() != Some(state) {
        return Err(AuthError::StateMismatch);
    }
    if query.error.is_none() && query.code.as_deref().is_none_or(str::is_empty) {
        return Err(AuthError::MissingCode);
    }
    Ok(query)
}
