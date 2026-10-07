//! Calls to official account services shared by administrator pages.
use crate::AdminState;
use axum::http::HeaderMap;
use bytes::Bytes;
use codex2api_upstream::BackendEndpoint as E;
use serde_json::Value;
use std::collections::HashMap;

pub(crate) async fn quota(
    state: &AdminState,
    id: &str,
    refresh: bool,
) -> Result<codex2api_storage::QuotaSnapshot, String> {
    quota_with_fetch(state, id, refresh, || async {
        request_with_revision(state, id, E::Usage, &HashMap::new(), None, None).await
    })
    .await
}

async fn quota_with_fetch<F, Fut>(
    state: &AdminState,
    id: &str,
    refresh: bool,
    fetch: F,
) -> Result<codex2api_storage::QuotaSnapshot, String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(Value, Option<i64>), String>>,
{
    state
        .supplier_cache
        .get_or_fetch(
            id,
            codex2api_storage::SupplierInfoSection::Quota,
            refresh,
            || async {
                let health = state
                    .storage
                    .supplier_health(id)
                    .await
                    .map_err(|e| e.to_string())?;
                let (value, revision) = fetch().await?;
                if value.get("rate_limit").is_none()
                    && value.get("plan_type").and_then(Value::as_str).is_none()
                {
                    return Err("ChatGPT 官方额度响应格式无效".into());
                }
                if let Some(revision) = revision
                    && let Some(until) = codex2api_upstream::quota_unavailable_until(
                        &value,
                        chrono::Utc::now().timestamp(),
                    )
                {
                    state
                        .storage
                        .exhaust_supplier_quota_if_unchanged(
                            id,
                            revision,
                            until,
                            "usage_limit_reached",
                            health.cooldown_revision,
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                } else if let Some(revision) = revision
                    && codex2api_upstream::quota_available(&value)
                {
                    state
                        .storage
                        .recover_supplier_quota(id, revision, health.cooldown_revision)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                Ok(value)
            },
        )
        .await
}

pub(crate) async fn request(
    state: &AdminState,
    id: &str,
    endpoint: E,
    params: &HashMap<String, String>,
    query: Option<&str>,
    value: Option<Value>,
) -> Result<Value, String> {
    request_with_revision(state, id, endpoint, params, query, value)
        .await
        .map(|(value, _)| value)
}

async fn request_with_revision(
    state: &AdminState,
    id: &str,
    endpoint: E,
    params: &HashMap<String, String>,
    query: Option<&str>,
    value: Option<Value>,
) -> Result<(Value, Option<i64>), String> {
    let future = async {
        let account = state
            .storage
            .require_account(id)
            .await
            .map_err(|e| e.to_string())?;
        if account.status == codex2api_storage::SupplierStatus::Pending {
            return Err("账户尚未完成授权".into());
        }
        let auth_revision = state
            .storage
            .supplier_auth_revision(id)
            .await
            .map_err(|e| e.to_string())?;
        let cooldown_revision = state
            .storage
            .supplier_health(id)
            .await
            .map_err(|e| e.to_string())?
            .cooldown_revision;
        let body = value
            .map(|v| serde_json::to_vec(&v))
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        // This is the same account client used for execution: proactive token
        // refresh, one reactive refresh/retry on 401, and permanent rejection.
        let result = async {
            let client = state.upstream.get(id).await?;
            client
                .forward_backend(endpoint, params, query, Bytes::from(body), HeaderMap::new())
                .await
        }
        .await;
        let response = match result {
            Ok(response) => response,
            Err(error) => {
                observe_request_failure(state, id, auth_revision, cooldown_revision, &error)
                    .await?;
                return Err(error.to_string());
            }
        };
        let status = response.status();
        let headers = response.headers().clone();
        let revision = response
            .extensions()
            .get::<codex2api_upstream::SupplierAuthRevision>()
            .map(|revision| revision.0);
        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(_) => {
                return Err("ChatGPT 官方响应读取失败".into());
            }
        };
        if !status.is_success() {
            let error = codex2api_upstream::UpstreamError::status_with_headers(
                status,
                String::from_utf8_lossy(&bytes).into_owned(),
                headers,
            );
            observe_request_failure(state, id, revision, cooldown_revision, &error).await?;
        }
        let value: Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => {
                return Err(format!("官方返回 HTTP {}，响应不是 JSON", status.as_u16()));
            }
        };
        if !status.is_success() {
            let message = value
                .pointer("/error/message")
                .or_else(|| value.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("请求未成功");
            return Err(format!(
                "官方返回 HTTP {}：{}",
                status.as_u16(),
                message.chars().take(500).collect::<String>()
            ));
        }
        Ok((value, revision))
    };
    match tokio::time::timeout(std::time::Duration::from_secs(30), future).await {
        Ok(result) => result,
        Err(_) => Err(if endpoint == E::ConsumeCredit {
            "官方响应超时，操作结果尚未确认，请先查看额度状态".to_string()
        } else {
            "官方请求超时，请重试".to_string()
        }),
    }
}

async fn observe_request_failure(
    state: &AdminState,
    id: &str,
    auth_revision: Option<i64>,
    cooldown_revision: i64,
    error: &codex2api_upstream::UpstreamError,
) -> Result<(), String> {
    let Some(revision) = auth_revision else {
        return Ok(());
    };
    // Share ChatGPT execution's classification; a probe has no separate failure counter
    // or blanket "keep state" rule. Transient failures are not auth rejection.
    match error.supplier_failure_for(codex2api_core::CHATGPT, chrono::Utc::now().timestamp()) {
        Some(codex2api_upstream::SupplierFailure::Authentication) => {
            state
                .storage
                .reject_supplier_auth(id, revision)
                .await
                .map_err(|e| e.to_string())?;
        }
        Some(codex2api_upstream::SupplierFailure::QuotaExhausted { until, code }) => {
            state
                .storage
                .exhaust_supplier_quota_if_unchanged(id, revision, until, &code, cooldown_revision)
                .await
                .map_err(|e| e.to_string())?;
        }
        None => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex2api_storage::{Storage, SupplierStatus, SupplierTokens};
    use serde_json::json;

    #[tokio::test]
    async fn failed_checks_follow_execution_auth_and_quota_classification() {
        use codex2api_upstream::UpstreamError;
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("quota-errors.sqlite"))
            .await
            .unwrap();
        let state = AdminState::new(storage.clone()).unwrap();
        let id = state.accounts.create_pending().await.unwrap().account.id;
        storage
            .upsert_supplier_tokens(SupplierTokens {
                account_id: id.clone(),
                access_token: Some("fixture".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        storage
            .set_account_status(&id, SupplierStatus::Active)
            .await
            .unwrap();
        let auth = storage.supplier_auth_revision(&id).await.unwrap().unwrap();
        let until = chrono::Utc::now().timestamp() + 86400;
        storage
            .exhaust_supplier_quota(&id, auth, until, "usage_limit_reached")
            .await
            .unwrap();
        let generation = storage
            .supplier_health(&id)
            .await
            .unwrap()
            .cooldown_revision;
        for error in [
            UpstreamError::status(reqwest::StatusCode::FORBIDDEN, "{}"),
            UpstreamError::status(
                reqwest::StatusCode::TOO_MANY_REQUESTS,
                r#"{"error":{"code":"rate_limit_exceeded"}}"#,
            ),
            UpstreamError::status(reqwest::StatusCode::SERVICE_UNAVAILABLE, "{}"),
            UpstreamError::StreamIdleTimeout,
            UpstreamError::Refresh(codex2api_auth::AuthError::RefreshRejected {
                status: 503,
                code: None,
                message: "temporary outage".into(),
            }),
        ] {
            observe_request_failure(&state, &id, Some(auth), generation, &error)
                .await
                .unwrap();
            let health = storage.supplier_health(&id).await.unwrap();
            assert!(!health.authentication_invalid);
            assert_eq!(health.cooldown_revision, generation);
        }
        let revoked = UpstreamError::Refresh(codex2api_auth::AuthError::RefreshRejected {
            status: 400,
            code: Some("invalid_grant".into()),
            message: "refresh token revoked".into(),
        });
        assert!(
            quota_with_fetch(&state, &id, true, || async {
                observe_request_failure(&state, &id, Some(auth), generation, &revoked).await?;
                Err(revoked.to_string())
            })
            .await
            .is_err()
        );
        assert!(
            storage
                .supplier_health(&id)
                .await
                .unwrap()
                .authentication_invalid
        );
        assert!(
            storage
                .supplier_quota_probe_candidates("chatgpt")
                .await
                .unwrap()
                .is_empty()
        );
        storage
            .upsert_supplier_tokens(SupplierTokens {
                account_id: id.clone(),
                access_token: Some("reauthorized-fixture".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        let rejected = UpstreamError::status(reqwest::StatusCode::UNAUTHORIZED, "{}");
        observe_request_failure(&state, &id, Some(auth), generation, &rejected)
            .await
            .unwrap();
        assert!(
            !storage
                .supplier_health(&id)
                .await
                .unwrap()
                .authentication_invalid
        );
        let auth = storage.supplier_auth_revision(&id).await.unwrap().unwrap();
        let exhausted = UpstreamError::status(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            json!({"error":{"type":"usage_limit_reached","resets_at":until}}).to_string(),
        );
        observe_request_failure(&state, &id, Some(auth), generation, &exhausted)
            .await
            .unwrap();
        let generation = storage
            .supplier_health(&id)
            .await
            .unwrap()
            .cooldown_revision;
        assert_eq!(
            storage
                .supplier_health(&id)
                .await
                .unwrap()
                .cooldown_kind
                .as_deref(),
            Some("quota_exhausted")
        );
        storage.reset_supplier_quota(&id).await.unwrap();
        observe_request_failure(&state, &id, Some(auth), generation, &exhausted)
            .await
            .unwrap();
        assert!(
            storage
                .supplier_health(&id)
                .await
                .unwrap()
                .cooldown_kind
                .is_none()
        );
        observe_request_failure(&state, &id, Some(auth), generation, &rejected)
            .await
            .unwrap();
        assert!(
            storage
                .supplier_health(&id)
                .await
                .unwrap()
                .authentication_invalid
        );
    }

    #[tokio::test]
    async fn fresh_official_reset_recovers_but_cached_unknown_failed_and_late_checks_do_not() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("quota-recovery.sqlite"))
            .await
            .unwrap();
        let state = AdminState::new(storage.clone()).unwrap();
        let id = state.accounts.create_pending().await.unwrap().account.id;
        storage
            .upsert_supplier_tokens(SupplierTokens {
                account_id: id.clone(),
                access_token: Some("fixture".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        storage
            .set_account_status(&id, SupplierStatus::Active)
            .await
            .unwrap();
        let auth = storage.supplier_auth_revision(&id).await.unwrap().unwrap();
        let until = chrono::Utc::now().timestamp() + 86400;
        let available = json!({"rate_limit":{"allowed":true,"primary_window":{"used_percent":0,"reset_at":until}}});
        let exhausted = json!({"rate_limit":{"allowed":false,"primary_window":{"used_percent":100,"reset_at":until}}});
        quota_with_fetch(&state, &id, true, || async {
            Ok((available.clone(), Some(auth)))
        })
        .await
        .unwrap();
        storage
            .exhaust_supplier_quota(&id, auth, until, "usage_limit_reached")
            .await
            .unwrap();
        quota_with_fetch(&state, &id, false, || async {
            panic!("fresh cache must be reused")
        })
        .await
        .unwrap();
        assert!(
            storage
                .supplier_health(&id)
                .await
                .unwrap()
                .cooldown_kind
                .is_some()
        );
        for value in [json!({"plan_type":"plus"}), exhausted.clone()] {
            quota_with_fetch(&state, &id, true, || async { Ok((value, Some(auth))) })
                .await
                .unwrap();
            assert!(
                storage
                    .supplier_health(&id)
                    .await
                    .unwrap()
                    .cooldown_kind
                    .is_some()
            );
        }
        let before_failure = storage.get_account_quota(&id).await.unwrap();
        assert!(
            quota_with_fetch(&state, &id, true, || async { Err("offline".into()) })
                .await
                .is_err()
        );
        assert_eq!(
            storage.get_account_quota(&id).await.unwrap(),
            before_failure
        );
        assert!(
            storage
                .supplier_health(&id)
                .await
                .unwrap()
                .cooldown_kind
                .is_some()
        );
        quota_with_fetch(&state, &id, true, || async {
            storage
                .exhaust_supplier_quota(&id, auth, until, "usage_limit_reached")
                .await
                .unwrap();
            Ok((available.clone(), Some(auth)))
        })
        .await
        .unwrap();
        assert!(
            storage
                .supplier_health(&id)
                .await
                .unwrap()
                .cooldown_kind
                .is_some()
        );
        let recovered = quota_with_fetch(&state, &id, true, || async {
            Ok((available.clone(), Some(auth)))
        })
        .await
        .unwrap();
        assert!(
            storage
                .supplier_health(&id)
                .await
                .unwrap()
                .cooldown_kind
                .is_none()
        );
        assert_eq!(
            storage.get_account_quota(&id).await.unwrap(),
            Some(recovered)
        );
        quota_with_fetch(&state, &id, true, || async {
            storage.reset_supplier_quota(&id).await.unwrap();
            Ok((exhausted, Some(auth)))
        })
        .await
        .unwrap();
        assert!(
            storage
                .supplier_health(&id)
                .await
                .unwrap()
                .cooldown_kind
                .is_none()
        );
        assert!(
            storage
                .supplier_quota_probe_candidates("chatgpt")
                .await
                .unwrap()
                .is_empty()
        );
    }
}
