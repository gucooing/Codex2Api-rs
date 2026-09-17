//! New-account authorization stays in memory until credentials have been exchanged.
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use codex2api_accounts::{AccountIdentity, AuthDotJson};
use codex2api_storage::OAuthPending;
use tokio::sync::Mutex;

use crate::manual::{DeviceAuthorization, parse_callback_url};
use crate::oauth::{exchange_code_for_tokens, refresh_tokens};
use crate::persist::{auth_from_exchanged, chatgpt_auth};
use crate::tokens::{apply_refresh, parse_chatgpt_jwt_claims, token_set_from_auth};
use crate::transport::AccountHttpClients;
use crate::{AuthError, AuthService, CompletedLogin, LoginFlow, Result};

pub(crate) struct DraftLogin {
    expires: Instant,
    inner: Mutex<DraftState>,
}

struct DraftState {
    pending: OAuthPending,
    identity: AccountIdentity,
    proxy_id: Option<String>,
    http: AccountHttpClients,
    active: bool,
    last_poll: Option<Instant>,
    authorized: Option<AuthDotJson>,
}

impl AuthService {
    async fn draft_http(
        &self,
        identity: &AccountIdentity,
        proxy_id: Option<&str>,
    ) -> Result<AccountHttpClients> {
        let proxy = match proxy_id.filter(|id| !id.is_empty()) {
            Some(id) => Some(self.storage()?.require_outbound_proxy(id).await?),
            None => None,
        };
        AccountHttpClients::with_proxy(identity, proxy.as_ref().map(|proxy| proxy.url.as_str()))
    }

    pub async fn begin_manual_login_with_identity(
        &self,
        identity: AccountIdentity,
        device: bool,
        proxy_id: Option<&str>,
    ) -> Result<OAuthPending> {
        let http = self.draft_http(&identity, proxy_id).await?;
        let (pending, ttl) = self.prepare_login_flow(&http, device).await?;
        let draft = Arc::new(DraftLogin {
            expires: Instant::now() + ttl,
            inner: Mutex::new(DraftState {
                pending: pending.clone(),
                identity,
                proxy_id: proxy_id.filter(|id| !id.is_empty()).map(str::to_owned),
                http,
                active: true,
                last_poll: None,
                authorized: None,
            }),
        });
        let mut drafts = self.drafts.lock().await;
        drafts.retain(|_, draft| draft.expires > Instant::now());
        drafts.insert(pending.state.clone(), draft);
        Ok(pending)
    }

    pub(crate) async fn draft_login(&self, state: &str) -> Option<Arc<DraftLogin>> {
        let mut drafts = self.drafts.lock().await;
        drafts.retain(|_, draft| draft.expires > Instant::now());
        drafts.get(state).cloned()
    }

    pub async fn pending_login(&self, state: &str) -> Result<Option<OAuthPending>> {
        if let Some(draft) = self.draft_login(state).await {
            let inner = draft.inner.lock().await;
            return Ok(
                (inner.active && draft.expires > Instant::now()).then(|| inner.pending.clone())
            );
        }
        Ok(self.storage()?.get_oauth_pending(state).await?)
    }

    pub async fn cancel_draft_login(&self, state: &str) {
        let draft = self.drafts.lock().await.remove(state);
        if let Some(draft) = draft {
            draft.inner.lock().await.active = false;
        }
    }

    async fn save_draft_auth(
        &self,
        identity: &AccountIdentity,
        proxy_id: Option<&str>,
        auth: AuthDotJson,
    ) -> Result<CompletedLogin> {
        let tokens = auth
            .tokens
            .as_ref()
            .ok_or(AuthError::MissingChatgptAccountId)?;
        let claims = parse_chatgpt_jwt_claims(&tokens.id_token)?;
        let bound = self
            .accounts()
            .save_authorized_identity(identity, claims.oauth_identity()?, &auth, proxy_id)
            .await?;
        // Reused accounts may have changed their route; reload against persisted identity.
        self.evict_account_http(&bound.account.id).await;
        Ok(CompletedLogin {
            account: bound.account,
            identity: bound.identity,
            reused_existing: bound.reused_existing,
            claims,
            tokens: token_set_from_auth(&auth),
            auth,
        })
    }

    pub(crate) async fn complete_draft_callback(
        &self,
        state: &str,
        draft: Arc<DraftLogin>,
        callback_url: &str,
    ) -> Result<CompletedLogin> {
        let mut inner = draft.inner.lock().await;
        if !inner.active || draft.expires <= Instant::now() {
            return Err(AuthError::PendingNotFound);
        }
        if !matches!(self.login_flow(&inner.pending)?, LoginFlow::Callback { .. }) {
            return Err(AuthError::Other(anyhow::anyhow!(
                "当前授权方式不是回调链接"
            )));
        }
        let query = parse_callback_url(callback_url, &inner.pending.redirect_uri, state)?;
        if query.is_cancel() {
            return Err(AuthError::Cancelled);
        }
        if let Some(code) = query.error {
            return Err(AuthError::Callback {
                code,
                description: query.error_description,
            });
        }
        if inner.authorized.is_none() {
            let exchanged = exchange_code_for_tokens(
                &inner.http.raw,
                self.config(),
                &inner.pending.redirect_uri,
                &inner.pending.code_verifier,
                query.code.as_deref().ok_or(AuthError::MissingCode)?,
            )
            .await?;
            let claims = parse_chatgpt_jwt_claims(&exchanged.id_token)?;
            let mut auth = auth_from_exchanged(&exchanged, &claims);
            auth.openai_api_key =
                crate::oauth::obtain_api_key(&inner.http.raw, self.config(), &exchanged.id_token)
                    .await
                    .ok();
            inner.authorized = Some(auth);
        }
        let done = self
            .save_draft_auth(
                &inner.identity,
                inner.proxy_id.as_deref(),
                inner.authorized.clone().unwrap(),
            )
            .await?;
        inner.active = false;
        self.drafts.lock().await.remove(state);
        Ok(done)
    }

    pub(crate) async fn poll_draft_device(
        &self,
        state: &str,
        draft: Arc<DraftLogin>,
    ) -> Result<Option<CompletedLogin>> {
        let mut inner = draft.inner.lock().await;
        if !inner.active || draft.expires <= Instant::now() {
            return Err(AuthError::PendingNotFound);
        }
        let LoginFlow::Device {
            device_auth_id,
            user_code,
            interval,
            ..
        } = self.login_flow(&inner.pending)?
        else {
            return Err(AuthError::Other(anyhow::anyhow!("当前授权方式不是设备码")));
        };
        if inner.authorized.is_none() {
            if inner
                .last_poll
                .is_some_and(|last| last.elapsed() < Duration::from_secs(interval.max(1)))
            {
                return Ok(None);
            }
            inner.last_poll = Some(Instant::now());
            let response = inner
                .http
                .raw
                .post(format!(
                    "{}/api/accounts/deviceauth/token",
                    self.config().issuer.trim_end_matches('/')
                ))
                .timeout(Duration::from_secs(30))
                .json(&serde_json::json!({"device_auth_id":device_auth_id,"user_code":user_code}))
                .send()
                .await?;
            let status = response.status();
            if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::NOT_FOUND
            {
                return Ok(None);
            }
            if !status.is_success() {
                return Err(AuthError::token_endpoint(status, "设备码授权失败"));
            }
            let code: DeviceAuthorization = response.json().await?;
            let exchanged = exchange_code_for_tokens(
                &inner.http.raw,
                self.config(),
                &inner.pending.redirect_uri,
                &code.code_verifier,
                &code.authorization_code,
            )
            .await?;
            let claims = parse_chatgpt_jwt_claims(&exchanged.id_token)?;
            inner.authorized = Some(auth_from_exchanged(&exchanged, &claims));
        }
        let done = self
            .save_draft_auth(
                &inner.identity,
                inner.proxy_id.as_deref(),
                inner.authorized.clone().unwrap(),
            )
            .await?;
        inner.active = false;
        self.drafts.lock().await.remove(state);
        Ok(Some(done))
    }

    pub async fn login_with_refresh_token(
        &self,
        identity: AccountIdentity,
        proxy_id: Option<&str>,
        refresh_token: &str,
    ) -> Result<CompletedLogin> {
        let refresh_token = refresh_token.trim();
        if refresh_token.is_empty() {
            return Err(AuthError::MissingRefreshToken);
        }
        if refresh_token.len() > 64 * 1024 {
            return Err(AuthError::Other(anyhow::anyhow!(
                "refresh token is too long"
            )));
        }
        let http = self.draft_http(&identity, proxy_id).await?;
        let refresh = refresh_tokens(&http.authenticated, self.config(), refresh_token).await?;
        let mut auth = chatgpt_auth(String::new(), String::new(), refresh_token.to_owned(), None);
        apply_refresh(&mut auth, &refresh)?;
        if auth
            .tokens
            .as_ref()
            .is_none_or(|tokens| tokens.id_token.is_empty() || tokens.access_token.is_empty())
        {
            return Err(AuthError::Refresh(
                "刷新响应缺少 ID Token 或 Access Token".into(),
            ));
        }
        self.save_draft_auth(&identity, proxy_id.filter(|id| !id.is_empty()), auth)
            .await
    }
}
