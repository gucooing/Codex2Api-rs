use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use codex2api_accounts::SupplierContext;
use codex2api_storage::SupplierStatus;

use crate::error::{ApiError, Result};
use crate::state::ApiState;

#[derive(Clone)]
pub(crate) struct AccessCheck {
    pub hash: String,
    pub account_id: String,
}

impl AccessCheck {
    pub async fn allowed(&self, storage: &codex2api_storage::Storage) -> Result<bool> {
        let Some(access) = storage.virtual_access(&self.hash).await? else {
            return Ok(false);
        };
        if access.account_id.as_deref() != Some(self.account_id.as_str()) {
            return Ok(false);
        }
        let account = storage.get_account(&self.account_id).await?;
        if !account.is_some_and(|a| {
            a.status == SupplierStatus::Active
                && a.provider_id == access.provider_id
                && a.provider_id == codex2api_core::CHATGPT
        }) {
            return Ok(false);
        }
        if storage
            .supplier_health(&self.account_id)
            .await?
            .error_message
            .is_some()
        {
            return Ok(false);
        }
        if storage
            .load_supplier_tokens(&self.account_id)
            .await?
            .and_then(|t| t.access_token)
            .is_none_or(|t| t.is_empty())
        {
            return Ok(false);
        }
        storage.touch_virtual_access(&self.hash).await?;
        Ok(true)
    }
}

pub(crate) struct ExecutionPrincipal {
    pub id: String,
    pub name: String,
    pub access: AccessCheck,
}

pub(crate) async fn check_virtual_quota(
    storage: &codex2api_storage::Storage,
    id: &str,
) -> Result<()> {
    codex2api_service::ExecutionService::new(storage.clone())
        .check_budget(id)
        .await?;
    Ok(())
}

pub(crate) async fn check_unpriced_execution(
    storage: &codex2api_storage::Storage,
    id: &str,
) -> Result<()> {
    codex2api_service::ExecutionService::new(storage.clone())
        .check_unpriced(id)
        .await?;
    Ok(())
}

pub(crate) async fn resolve_supplier(
    state: &ApiState,
    headers: &HeaderMap,
    oauth: axum::Extension<codex2api_storage::VirtualAccess>,
) -> Result<(ExecutionPrincipal, SupplierContext)> {
    let axum::Extension(oauth) = oauth;
    let account_id = oauth.account_id.as_deref().ok_or_else(|| {
        ApiError::openai(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "server_error",
            "No upstream account is bound to this virtual account.",
            Some("upstream_unavailable"),
        )
    })?;
    let ctx = state.accounts.load_context(account_id).await?;
    if oauth.provider_id != ctx.account.provider_id || oauth.provider_id != codex2api_core::CHATGPT
    {
        return Err(codex2api_service::ServiceError::Policy(
            codex2api_core::PolicyError::ProviderMismatch,
        )
        .into());
    }
    if ctx.account.status != SupplierStatus::Active
        || state
            .storage
            .supplier_health(account_id)
            .await?
            .error_message
            .is_some()
    {
        return Err(ApiError::account_disabled());
    }
    let expected = oauth.virtual_account_id.as_str();
    if headers
        .get_all("chatgpt-account-id")
        .iter()
        .any(|v| v.to_str().ok() != Some(expected))
    {
        return Err(ApiError::openai(
            axum::http::StatusCode::FORBIDDEN,
            "permission_error",
            "The account does not match this OAuth credential.",
            Some("account_mismatch"),
        ));
    }
    state
        .storage
        .touch_virtual_access(&oauth.token_hash)
        .await?;
    state.storage.touch_account(account_id).await?;
    Ok((
        ExecutionPrincipal {
            id: oauth.virtual_account_id,
            name: oauth.name,
            access: AccessCheck {
                hash: oauth.token_hash,
                account_id: account_id.to_owned(),
            },
        },
        ctx,
    ))
}

pub fn extract_bearer(headers: &HeaderMap) -> Result<&str> {
    let value = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(ApiError::missing_token)?;
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .ok_or_else(ApiError::missing_token)?
        .trim();
    if token.is_empty() {
        return Err(ApiError::missing_token());
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[tokio::test]
    async fn oauth_quota_is_local_and_old_connections_stop_after_rebinding() {
        use codex2api_accounts::{AuthDotJson, SupplierAccountStore, TokenData};
        use codex2api_storage::{
            OAuthDeviceIdentity, SupplierAccountUpdate, VirtualAccount, hash_token,
        };
        use serde_json::{Value, json};
        let dir = tempfile::tempdir().unwrap();
        let storage = codex2api_storage::Storage::open(dir.path().join("oauth-quota.sqlite"))
            .await
            .unwrap();
        let accounts = SupplierAccountStore::open(storage.clone());
        let first = accounts.create_pending().await.unwrap().account;
        let second = accounts.create_pending().await.unwrap().account;
        for real in [&first, &second] {
            storage
                .update_account(
                    &real.id,
                    SupplierAccountUpdate {
                        status: Some(SupplierStatus::Active),
                        chatgpt_account_id: Some(real.id.clone()),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            accounts
                .save_auth_for_account(
                    &real.id,
                    &AuthDotJson::chatgpt(
                        TokenData {
                            id_token: "fixture".into(),
                            access_token: "fixture-access".into(),
                            refresh_token: "fixture-refresh".into(),
                            account_id: Some(real.id.clone()),
                        },
                        None,
                    ),
                )
                .await
                .unwrap();
        }
        let account = VirtualAccount {
            provider_id: "chatgpt".into(),
            id: "virtual-id".into(),
            username: "user".into(),
            password_hash: "unused-hash".into(),
            name: "Virtual Name".into(),
            email: "virtual@example.test".into(),
            plan_type: "pro".into(),
            plan_id: "pro".into(),
            subscription_expires_at: None,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        storage.save_virtual_account(&account).await.unwrap();
        storage
            .save_execution_route(&account.id, "chatgpt", Some(&first.id), None)
            .await
            .unwrap();
        let device = storage
            .create_virtual_device(&account, "refresh-fixture", &OAuthDeviceIdentity::default())
            .await
            .unwrap()
            .unwrap();
        storage
            .register_virtual_access(
                &device,
                "refresh-fixture",
                "access-fixture",
                chrono::Utc::now().timestamp() + 600,
            )
            .await
            .unwrap();
        let hash = hash_token("access-fixture");
        let original = AccessCheck {
            hash: hash.clone(),
            account_id: first.id.clone(),
        };
        assert!(original.allowed(&storage).await.unwrap());
        let mut headers = HeaderMap::new();
        headers.insert("x-codex-primary-used-percent", "17".parse().unwrap());
        headers.insert("x-codex-active-limit", "upstream".parse().unwrap());
        headers.insert("x-codex-models-etag", "preserved".parse().unwrap());
        crate::providers::chatgpt::identity::quota_headers(&storage, &account.id, &mut headers)
            .await
            .unwrap();
        assert!(!headers.contains_key("x-codex-primary-used-percent"));
        assert!(!headers.contains_key("x-codex-primary-reset-at"));
        assert!(!headers.contains_key("x-codex-active-limit"));
        assert_eq!(headers["x-codex-models-etag"], "preserved");
        let event=json!({"type":"codex.rate_limits","plan_type":"business","rate_limits":{"primary":{"used_percent":17.0,"window_minutes":300,"reset_at":123}},"credits":{"has_credits":true,"unlimited":false,"balance":"12"}}).to_string();
        let custom: Value = serde_json::from_str(
            &crate::providers::chatgpt::identity::websocket_message(&storage, &hash, &event)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(custom["rate_limits"]["primary"].is_null());
        assert_eq!(custom["plan_type"], "pro");
        assert_eq!(custom["credits"]["has_credits"], false);
        let text = r#"{"type":"response.output_text.delta","delta":"codex.rate_limits"}"#;
        assert_eq!(
            crate::providers::chatgpt::identity::websocket_message(&storage, &hash, text)
                .await
                .unwrap(),
            text
        );
        storage.save_virtual_account(&account).await.unwrap();
        let official: Value = serde_json::from_str(
            &crate::providers::chatgpt::identity::websocket_message(&storage, &hash, &event)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(official["rate_limits"]["primary"].is_null());
        assert_eq!(official["plan_type"], "pro");
        let real = storage.get_account(&first.id).await.unwrap().unwrap();
        let mut metadata = json!({"email":"real@example.test","name":"Real Name","account_id":first.id,"user_id":"unrecorded-real-user","rate_limit":{"primary_window":{"used_percent":17.0}}});
        crate::providers::chatgpt::identity::mask(&mut metadata, &account, &real);
        assert_eq!(metadata["name"], "Virtual Name");
        assert_eq!(metadata["account_id"], "virtual-id");
        assert_eq!(metadata["email"], "virtual@example.test");
        assert_eq!(metadata["user_id"], "user-virtual-id");
        assert_eq!(
            metadata["rate_limit"]["primary_window"]["used_percent"],
            17.0
        );
        let mut plan = storage
            .virtual_plan(&account.plan_id)
            .await
            .unwrap()
            .unwrap();
        plan.config["primary_cost_limit_usd"] = json!(0);
        plan.config["weekly_cost_limit_usd"] = json!(10);
        assert!(
            storage
                .save_virtual_plan(&plan, Some(plan.revision))
                .await
                .unwrap()
        );
        crate::providers::chatgpt::identity::quota_headers(&storage, &account.id, &mut headers)
            .await
            .unwrap();
        assert_eq!(headers["x-codex-primary-window-minutes"], "300");
        assert_eq!(headers["x-codex-secondary-window-minutes"], "10080");
        assert_eq!(headers["x-codex-primary-used-percent"], "100");
        let updated: Value = serde_json::from_str(
            &crate::providers::chatgpt::identity::websocket_message(&storage, &hash, &event)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(updated["rate_limits"]["primary"]["window_minutes"], 300);
        assert_eq!(updated["rate_limits"]["secondary"]["window_minutes"], 10080);
        assert_eq!(updated["rate_limits"]["primary"]["used_percent"], 100);
        assert_eq!(
            updated["rate_limits"]["primary"]["reset_at"]
                .as_i64()
                .unwrap()
                .to_string(),
            headers["x-codex-primary-reset-at"]
        );
        assert!(check_virtual_quota(&storage, &account.id).await.is_err());
        storage
            .save_execution_route(&account.id, "chatgpt", Some(&second.id), Some(1))
            .await
            .unwrap();
        storage.save_virtual_account(&account).await.unwrap();
        assert!(!original.allowed(&storage).await.unwrap());
        let rebound = AccessCheck {
            hash: hash.clone(),
            account_id: second.id,
        };
        assert!(rebound.allowed(&storage).await.unwrap());
        storage
            .record_supplier_error(&rebound.account_id, "ChatGPT 官方通信失败")
            .await
            .unwrap();
        assert!(!rebound.allowed(&storage).await.unwrap());
        let health = storage.supplier_health(&rebound.account_id).await.unwrap();
        assert!(
            storage
                .recover_supplier(&rebound.account_id, health.revision)
                .await
                .unwrap()
        );
        assert!(rebound.allowed(&storage).await.unwrap());
        storage
            .revoke_virtual_device(&account.id, &device)
            .await
            .unwrap();
        assert!(!rebound.allowed(&storage).await.unwrap());
        storage.close().await;
        assert!(rebound.allowed(&storage).await.is_err());
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
