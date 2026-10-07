use chrono::Utc;

use crate::error::{AuthError, Result};
use crate::oauth::{OAuthConfig, exchange_code_for_tokens, refresh_tokens, revoke_tokens};
use crate::tokens::{
    AUTH_MODE_CHATGPT, ExchangedTokens, IdTokenInfo, RefreshResponse, TokenSet, apply_refresh,
    parse_chatgpt_jwt_claims, should_refresh, token_set_from_auth,
};
use codex2api_accounts::{
    AccountIdentity, AuthDotJson, BoundAccount, PendingAccount, SupplierAccountStore, TokenData,
};
use codex2api_storage::SupplierAccount;

pub async fn persist_auth(
    accounts: &SupplierAccountStore,
    account_id: &str,
    auth: &AuthDotJson,
) -> Result<()> {
    accounts.save_auth_for_account(account_id, auth).await?;
    Ok(())
}

pub async fn load_auth(
    accounts: &SupplierAccountStore,
    account: &SupplierAccount,
) -> Result<Option<AuthDotJson>> {
    Ok(accounts.load_auth_for_account(account).await?)
}

pub fn auth_from_exchanged(tokens: &ExchangedTokens, claims: &IdTokenInfo) -> AuthDotJson {
    tokens.into_auth_json(claims.chatgpt_account_id.clone())
}

pub async fn persist_exchanged_tokens(
    accounts: &SupplierAccountStore,
    account_id: &str,
    tokens: &ExchangedTokens,
    claims: &IdTokenInfo,
) -> Result<AuthDotJson> {
    let auth = auth_from_exchanged(tokens, claims);
    persist_auth(accounts, account_id, &auth).await?;
    Ok(auth)
}

pub async fn bind_completed_login(
    accounts: &SupplierAccountStore,
    pending: &PendingAccount,
    claims: &IdTokenInfo,
    auth: &AuthDotJson,
) -> Result<BoundAccount> {
    if let Some(existing_id) = pending.account.chatgpt_account_id.as_deref() {
        match claims.chatgpt_account_id.as_deref() {
            Some(claimed) if claimed == existing_id => {}
            Some(_) => return Err(AuthError::AccountMismatch),
            None => return Err(AuthError::MissingChatgptAccountId),
        }
        let ctx = accounts.load_context(&pending.account.id).await?;
        let existing_user = ctx
            .account
            .chatgpt_user_id
            .clone()
            .or_else(|| {
                ctx.auth.as_ref()?.tokens.as_ref().and_then(|tokens| {
                    parse_chatgpt_jwt_claims(&tokens.id_token)
                        .ok()?
                        .chatgpt_user_id
                })
            })
            .filter(|id| !id.trim().is_empty())
            .ok_or(AuthError::MissingChatgptUserId)?;
        if claims.chatgpt_user_id.as_deref() != Some(existing_user.as_str()) {
            return Err(AuthError::AccountMismatch);
        }
        persist_auth(accounts, &pending.account.id, auth).await?;
        let ctx = accounts.load_context(&pending.account.id).await?;
        return Ok(BoundAccount {
            account: ctx.account,
            identity: ctx.identity,
            reused_existing: true,
        });
    }
    let oauth = claims.oauth_identity()?;
    Ok(accounts.bind(pending, oauth, Some(auth)).await?)
}

pub async fn complete_login(
    accounts: &SupplierAccountStore,
    http: &reqwest::Client,
    cfg: &OAuthConfig,
    pending: &PendingAccount,
    redirect_uri: &str,
    code_verifier: &str,
    code: &str,
) -> Result<CompletedLogin> {
    let exchanged = exchange_code_for_tokens(http, cfg, redirect_uri, code_verifier, code).await?;
    let claims = parse_chatgpt_jwt_claims(&exchanged.id_token)?;
    let mut auth = auth_from_exchanged(&exchanged, &claims);
    auth.openai_api_key = crate::oauth::obtain_api_key(http, cfg, &exchanged.id_token)
        .await
        .ok();
    let bound = bind_completed_login(accounts, pending, &claims, &auth).await?;
    Ok(CompletedLogin {
        account: bound.account,
        identity: bound.identity,
        reused_existing: bound.reused_existing,
        claims,
        tokens: token_set_from_auth(&auth),
        auth,
    })
}

#[derive(Debug, Clone)]
pub struct CompletedLogin {
    pub account: SupplierAccount,
    pub identity: AccountIdentity,
    pub reused_existing: bool,
    pub claims: IdTokenInfo,
    pub tokens: TokenSet,
    pub auth: AuthDotJson,
}

pub async fn refresh_account(
    accounts: &SupplierAccountStore,
    http: &reqwest::Client,
    cfg: &OAuthConfig,
    account: &SupplierAccount,
    force: bool,
) -> Result<AuthDotJson> {
    let storage = accounts.storage()?;
    let snapshot = storage
        .supplier_auth_snapshot(&account.id)
        .await?
        .ok_or_else(|| AuthError::TokensNotFound(account.id.clone()))?;
    let mut auth = AuthDotJson::from_supplier_tokens(&snapshot.tokens)?;
    if let Some(tokens) = auth.tokens.as_mut()
        && tokens.account_id.is_none()
    {
        tokens.account_id = snapshot.chatgpt_account_id.clone();
    }
    if !force && !should_refresh(&auth) {
        return Ok(auth);
    }
    let refresh_token = auth
        .tokens
        .as_ref()
        .map(|t| t.refresh_token.clone())
        .filter(|s| !s.is_empty())
        .ok_or(AuthError::MissingRefreshToken)?;
    let previous_account_id = auth
        .tokens
        .as_ref()
        .and_then(|t| t.account_id.clone())
        .or_else(|| account.chatgpt_account_id.clone());
    let previous_user_id = account.chatgpt_user_id.clone().or_else(|| {
        parse_chatgpt_jwt_claims(&auth.tokens.as_ref()?.id_token)
            .ok()?
            .chatgpt_user_id
    });
    let refresh: RefreshResponse = refresh_tokens(http, cfg, &refresh_token).await?;
    if let Some(id_token) = &refresh.id_token {
        let claims = parse_chatgpt_jwt_claims(id_token)?;
        let expected_user = previous_user_id
            .as_deref()
            .filter(|id| !id.trim().is_empty())
            .ok_or(AuthError::MissingChatgptUserId)?;
        if claims.chatgpt_user_id.as_deref() != Some(expected_user)
            || claims.chatgpt_account_id != previous_account_id
        {
            return Err(AuthError::AccountMismatch);
        }
    }
    apply_refresh(&mut auth, &refresh)?;
    if let Some(expected) = previous_account_id.as_deref()
        && let Some(new_id) = auth.tokens.as_ref().and_then(|t| t.account_id.as_deref())
        && new_id != expected
    {
        return Err(AuthError::AccountMismatch);
    }
    // Refresh is not a new login: preserve administrator intent and billing
    // rejection, and never overwrite credentials from a concurrent reauthorization.
    storage
        .replace_supplier_tokens(
            &account.id,
            snapshot.auth_revision,
            auth.to_supplier_tokens(&account.id),
        )
        .await?;
    let current = storage
        .supplier_auth_snapshot(&account.id)
        .await?
        .ok_or_else(|| AuthError::TokensNotFound(account.id.clone()))?;
    let mut auth = AuthDotJson::from_supplier_tokens(&current.tokens)?;
    if let Some(tokens) = auth.tokens.as_mut()
        && tokens.account_id.is_none()
    {
        tokens.account_id = current.chatgpt_account_id.clone();
    }
    if let Some(tokens) = auth.tokens.as_ref()
        && let Ok(claims) = parse_chatgpt_jwt_claims(&tokens.id_token)
        && let Ok(oauth) = claims.oauth_identity()
    {
        storage
            .update_supplier_profile(
                &account.id,
                current.auth_revision,
                oauth.email.as_deref(),
                oauth.plan_type.as_deref(),
            )
            .await?;
    }
    Ok(auth)
}

pub async fn revoke_account(
    accounts: &SupplierAccountStore,
    http: &reqwest::Client,
    cfg: &OAuthConfig,
    account: &SupplierAccount,
) -> Result<()> {
    let auth = load_auth(accounts, account).await?;
    let (refresh, access) = match auth.as_ref().and_then(|a| a.tokens.as_ref()) {
        Some(tokens) => (
            Some(tokens.refresh_token.as_str()).filter(|s| !s.is_empty()),
            Some(tokens.access_token.as_str()).filter(|s| !s.is_empty()),
        ),
        None => (None, None),
    };
    if let Err(err) = revoke_tokens(http, cfg, refresh, access).await {
        tracing::warn!(account_id = %account.id, "failed to revoke auth tokens: {err}");
    }
    persist_auth(accounts, &account.id, &empty_chatgpt_auth()).await?;
    Ok(())
}

pub async fn pending_from_account(
    accounts: &SupplierAccountStore,
    account_id: &str,
) -> Result<PendingAccount> {
    let ctx = accounts.load_context(account_id).await?;
    Ok(PendingAccount {
        account: ctx.account,
        identity: ctx.identity,
    })
}

pub fn empty_chatgpt_auth() -> AuthDotJson {
    AuthDotJson {
        auth_mode: Some(AUTH_MODE_CHATGPT.to_string()),
        openai_api_key: None,
        tokens: None,
        last_refresh: None,
        extra: serde_json::Map::new(),
    }
}

pub fn chatgpt_auth(
    id_token: String,
    access_token: String,
    refresh_token: String,
    account_id: Option<String>,
) -> AuthDotJson {
    AuthDotJson::chatgpt(
        TokenData {
            id_token,
            access_token,
            refresh_token,
            account_id,
        },
        Some(Utc::now()),
    )
}
