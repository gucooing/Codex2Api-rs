use crate::pool_execution::SupplierContext;
use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use codex2api_storage::SupplierStatus;

use crate::error::{ApiError, Result};
use crate::state::ApiState;

#[derive(Clone)]
pub(crate) struct AccessCheck {
    pub hash: String,
    pub account_id: String,
}

impl AccessCheck {
    pub async fn validate(&self, storage: &codex2api_storage::Storage) -> Result<()> {
        let access = storage
            .virtual_access(&self.hash)
            .await?
            .ok_or_else(ApiError::invalid_token)?;
        if access.account_id.as_deref() != Some(self.account_id.as_str()) {
            return Err(ApiError::openai(
                axum::http::StatusCode::CONFLICT,
                "configuration_error",
                "The supplier binding changed. Reconnect to continue.",
                Some("supplier_binding_changed"),
            ));
        }
        let account = storage
            .get_account(&self.account_id)
            .await?
            .ok_or_else(|| {
                ApiError::openai(
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "server_error",
                    "The supplier is unavailable.",
                    Some("upstream_unavailable"),
                )
            })?;
        if account.provider_id != access.provider_id
            || account.provider_id != codex2api_core::CHATGPT
        {
            return Err(ApiError::openai(
                axum::http::StatusCode::FORBIDDEN,
                "permission_error",
                "Provider mismatch.",
                Some("provider_mismatch"),
            ));
        }
        if account.status != SupplierStatus::Active {
            return Err(ApiError::account_disabled());
        }
        if storage
            .supplier_health(&self.account_id)
            .await?
            .authentication_invalid
        {
            return Err(codex2api_upstream::UpstreamError::Unauthorized.into());
        }
        if storage
            .load_supplier_tokens(&self.account_id)
            .await?
            .and_then(|t| t.access_token)
            .is_none_or(|t| t.is_empty())
        {
            return Err(codex2api_upstream::UpstreamError::MissingAccessToken(
                self.account_id.clone(),
            )
            .into());
        }
        storage.touch_virtual_access(&self.hash).await?;
        Ok(())
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
    let ctx = crate::pool_execution::select(state, &oauth, &[]).await?;
    let account_id = ctx.account.id.clone();
    if oauth.provider_id != ctx.account.provider_id || oauth.provider_id != codex2api_core::CHATGPT
    {
        return Err(codex2api_service::ServiceError::Policy(
            codex2api_core::PolicyError::ProviderMismatch,
        )
        .into());
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
    state.storage.touch_account(&account_id).await?;
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
    if headers.get_all(AUTHORIZATION).iter().count() != 1 {
        return Err(ApiError::missing_token());
    }
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
        assert!(original.validate(&storage).await.is_ok());
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
        // 内层窗口从首次使用开始，先为本账户建立请求记录。
        storage
            .insert_usage(&codex2api_storage::UsageRecord {
                id: "prior-request".into(),
                account_id: first.id.clone(),
                subject_id: account.id.clone(),
                endpoint: "/v1/responses".into(),
                transport: "websocket".into(),
                requested_at_ms: chrono::Utc::now().timestamp_millis(),
                status: "in_progress".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let mut plan = storage
            .virtual_plan(&account.plan_id)
            .await
            .unwrap()
            .unwrap();
        plan.config["spending_windows"] = serde_json::json!([{"duration_seconds":604800,"cost_limit_usd":"10"},{"duration_seconds":18000,"cost_limit_usd":"0"}]);
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
        assert!(original.validate(&storage).await.is_err());
        let rebound = AccessCheck {
            hash: hash.clone(),
            account_id: second.id,
        };
        assert!(rebound.validate(&storage).await.is_ok());
        storage
            .reject_supplier_auth(
                &rebound.account_id,
                storage
                    .supplier_auth_revision(&rebound.account_id)
                    .await
                    .unwrap()
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(rebound.validate(&storage).await.is_err());
        let health = storage.supplier_health(&rebound.account_id).await.unwrap();
        assert!(
            storage
                .recover_supplier(&rebound.account_id, health.revision)
                .await
                .unwrap()
        );
        assert!(rebound.validate(&storage).await.is_ok());
        storage
            .revoke_virtual_device(&account.id, &device)
            .await
            .unwrap();
        assert!(rebound.validate(&storage).await.is_err());
        storage.close().await;
        assert!(rebound.validate(&storage).await.is_err());
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
