use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use codex2api_accounts::AccountContext;
use codex2api_storage::{AccountStatus, ProxyApiKey};

use crate::error::{ApiError, Result};
use crate::state::ApiState;

/// Resolve `Authorization: Bearer <proxy api key>` to an isolated account context.
pub async fn authenticate(
    state: &ApiState,
    headers: &HeaderMap,
) -> Result<(ProxyApiKey, AccountContext)> {
    let token = extract_bearer(headers)?;
    let key = state
        .storage
        .lookup_proxy_api_key(token)
        .await?
        .ok_or_else(ApiError::invalid_api_key)?;
    let ctx = state.accounts.load_context(&key.account_id).await?;
    match ctx.account.status {
        AccountStatus::Active => {}
        AccountStatus::Disabled => return Err(ApiError::account_disabled()),
        AccountStatus::Pending => return Err(ApiError::account_not_ready()),
    }
    if ctx
        .auth
        .as_ref()
        .and_then(|auth| auth.tokens.as_ref())
        .is_none_or(|tokens| tokens.access_token.is_empty())
    {
        return Err(ApiError::account_not_authenticated());
    }

    if let Err(err) = state.storage.touch_proxy_api_key(&key.id).await {
        tracing::warn!(error = %err, key_id = %key.id, "failed to touch proxy API key");
    }
    if let Err(err) = state.storage.touch_account(&ctx.account.id).await {
        tracing::warn!(error = %err, account_id = %ctx.account.id, "failed to touch account");
    }

    Ok((key, ctx))
}

pub fn extract_bearer(headers: &HeaderMap) -> Result<&str> {
    let value = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(ApiError::missing_api_key)?;
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .ok_or_else(ApiError::missing_api_key)?
        .trim();
    if token.is_empty() {
        return Err(ApiError::missing_api_key());
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[tokio::test]
    async fn new_requests_use_new_binding_and_existing_context_keeps_its_account() {
        use codex2api_accounts::{AccountStore, AuthDotJson, TokenData};
        let dir = tempfile::tempdir().unwrap();
        let storage = codex2api_storage::Storage::open(dir.path().join("binding.sqlite"))
            .await
            .unwrap();
        let accounts = AccountStore::open(storage.clone());
        let first = accounts.create_pending().await.unwrap().account;
        let second = accounts.create_pending().await.unwrap().account;
        for account in [&first, &second] {
            storage
                .set_account_status(&account.id, AccountStatus::Active)
                .await
                .unwrap();
            accounts
                .save_auth_for_account(
                    &account.id,
                    &AuthDotJson::chatgpt(
                        TokenData {
                            id_token: "id-fixture".into(),
                            access_token: format!("access-{}", account.id),
                            refresh_token: "refresh-fixture".into(),
                            account_id: Some(account.id.clone()),
                        },
                        None,
                    ),
                )
                .await
                .unwrap();
        }
        let auth = codex2api_auth::AuthService::new(accounts.clone()).unwrap();
        let state = ApiState::new(
            storage.clone(),
            accounts,
            codex2api_upstream::UpstreamPool::new(auth),
        );
        let key = storage
            .create_proxy_api_key(&first.id, Some("key"))
            .await
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", key.token)).unwrap(),
        );
        let (_, existing) = authenticate(&state, &headers).await.unwrap();
        storage
            .bind_proxy_api_key(&key.record.id, &second.id)
            .await
            .unwrap();
        let (_, next) = authenticate(&state, &headers).await.unwrap();
        assert_eq!(existing.account.id, first.id);
        assert_eq!(existing.identity.installation_id, first.installation_id);
        assert_eq!(next.account.id, second.id);
        assert_eq!(next.identity.installation_id, second.installation_id);
        assert_eq!(
            existing.auth.unwrap().tokens.unwrap().access_token,
            format!("access-{}", first.id)
        );
        assert_eq!(
            next.auth.unwrap().tokens.unwrap().access_token,
            format!("access-{}", second.id)
        );
        storage.close().await;
    }

    #[test]
    fn extracts_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer c2a_abc"));
        assert_eq!(extract_bearer(&headers).unwrap(), "c2a_abc");
    }

    #[test]
    fn rejects_missing_and_empty() {
        assert!(extract_bearer(&HeaderMap::new()).is_err());
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer  "));
        assert!(extract_bearer(&headers).is_err());
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Basic abc"));
        assert!(extract_bearer(&headers).is_err());
    }
}
