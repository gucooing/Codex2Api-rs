//! Internal supplier failover before generation output is committed to the client.
use crate::{ApiError, ApiState, Result};
use axum::body::Bytes;
#[derive(Clone)]
pub(crate) struct SupplierContext {
    pub account: codex2api_storage::SupplierAccount,
}
use codex2api_storage::VirtualAccess;
use codex2api_upstream::{SupplierFailure, UpstreamError};
use futures::StreamExt;

#[derive(Clone, Copy)]
pub(crate) struct FirstEventAt(pub std::time::Instant);

pub(crate) fn exhausted() -> ApiError {
    ApiError::openai(
        http::StatusCode::SERVICE_UNAVAILABLE,
        "server_error",
        "No available supplier account in the configured tag pool. Please retry later.",
        Some("supplier_pool_exhausted"),
    )
}

pub(crate) async fn select(
    state: &ApiState,
    oauth: &VirtualAccess,
    excluded: &[String],
) -> Result<SupplierContext> {
    // Refresh the consumer credential on every internal attempt as well.
    let access = state
        .storage
        .virtual_access(&oauth.token_hash)
        .await?
        .ok_or_else(ApiError::invalid_token)?;
    let id = state
        .storage
        .select_pool_supplier(&access.virtual_account_id, &access.provider_id, excluded)
        .await?
        .ok_or_else(exhausted)?;
    Ok(SupplierContext {
        account: state.storage.require_account(&id).await?,
    })
}

pub(crate) async fn observe(
    state: &ApiState,
    id: &str,
    revision: Option<i64>,
    failure: &SupplierFailure,
) -> Result<()> {
    if let Some(revision) = revision {
        match failure {
            SupplierFailure::Authentication => {
                state.storage.reject_supplier_auth(id, revision).await?
            }
            SupplierFailure::PaymentRequired { code } => {
                state
                    .storage
                    .mark_supplier_payment_required(id, revision, code.as_deref(), None)
                    .await?
            }
            SupplierFailure::QuotaExhausted { until, code } => {
                state
                    .storage
                    .exhaust_supplier_quota(id, revision, *until, code)
                    .await?
            }
        }
    }
    Ok(())
}

pub(crate) async fn recover_stream_auth(
    state: &ApiState,
    ctx: &SupplierContext,
    observed_revision: Option<i64>,
) -> Result<Option<SupplierFailure>> {
    let snapshot = state
        .storage
        .supplier_auth_snapshot(&ctx.account.id)
        .await?
        .ok_or_else(|| UpstreamError::MissingAccessToken(ctx.account.id.clone()))?;
    if observed_revision.is_some_and(|revision| revision != snapshot.auth_revision) {
        return Ok(None);
    }
    if state
        .storage
        .supplier_health(&ctx.account.id)
        .await?
        .authentication_invalid
    {
        return Ok(Some(SupplierFailure::Authentication));
    }
    let token = snapshot.tokens.access_token.as_deref().unwrap_or("");
    match state
        .upstream
        .refresh_supplier_auth(&ctx.account, token)
        .await
    {
        Ok(_) => Ok(None),
        Err(error) => {
            match error
                .supplier_failure_for(&ctx.account.provider_id, chrono::Utc::now().timestamp())
            {
                Some(failure) => Ok(Some(failure)),
                None => Err(error.into()),
            }
        }
    }
}

/// Buffer only the non-output prelude. Once output/tool execution is visible,
/// replay would duplicate side effects and is not a recoverable rejection.
pub(crate) fn prelude_event(value: &serde_json::Value) -> bool {
    matches!(
        value.get("type").and_then(serde_json::Value::as_str),
        Some("response.created" | "response.in_progress" | "response.queued" | "codex.rate_limits")
    )
}

async fn inspect_for(
    provider: &str,
    is_prelude: fn(&serde_json::Value) -> bool,
    mut response: reqwest::Response,
) -> std::result::Result<reqwest::Response, UpstreamError> {
    let status = response.status();
    let headers = response.headers().clone();
    if !status.is_success() {
        let body = response.text().await?;
        return Err(UpstreamError::status_with_headers(status, body, headers));
    }
    if !headers
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .is_some_and(|v| v.starts_with("text/event-stream"))
    {
        return Ok(response);
    }
    let revision = response
        .extensions()
        .get::<codex2api_upstream::SupplierAuthRevision>()
        .copied();
    let mut prefix = Vec::new();
    let mut parsed = 0;
    let mut first_event = None;
    'read: while prefix.len() < 256 * 1024 {
        let chunk = tokio::time::timeout(
            codex2api_upstream::DEFAULT_STREAM_IDLE_TIMEOUT,
            response.chunk(),
        )
        .await
        .map_err(|_| UpstreamError::StreamIdleTimeout)??;
        let Some(chunk) = chunk else {
            break;
        };
        prefix.extend_from_slice(&chunk);
        loop {
            let remaining = &prefix[parsed..];
            let boundary = remaining
                .windows(2)
                .position(|s| s == b"\n\n")
                .map(|p| (p, 2))
                .or_else(|| {
                    remaining
                        .windows(4)
                        .position(|s| s == b"\r\n\r\n")
                        .map(|p| (p, 4))
                });
            let Some((end, separator)) = boundary else {
                break;
            };
            let frame = String::from_utf8_lossy(&remaining[..end]);
            let data = frame
                .lines()
                .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
                .collect::<Vec<_>>()
                .join("\n");
            parsed += end + separator;
            if data.is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&data) else {
                break 'read;
            };
            if value
                .get("type")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|kind| kind.starts_with("response.") || kind == "error")
            {
                first_event.get_or_insert_with(std::time::Instant::now);
            }
            if codex2api_upstream::classify_provider_failure(
                provider,
                None,
                &value,
                &headers,
                chrono::Utc::now().timestamp(),
            )
            .is_some()
            {
                let body = value.get("response").unwrap_or(&value).to_string();
                let failure = codex2api_upstream::ResponseFailure::from_error(None, &value);
                return Err(UpstreamError::Status {
                    status: failure.status.unwrap_or(429),
                    body,
                    headers,
                    auth_revision: revision.map(|revision| revision.0),
                });
            }
            if !is_prelude(&value) {
                break 'read;
            }
        }
    }
    let stream = futures::stream::once(async move { Ok::<_, reqwest::Error>(Bytes::from(prefix)) })
        .chain(response.bytes_stream());
    let mut rebuilt = http::Response::new(reqwest::Body::wrap_stream(stream));
    *rebuilt.status_mut() = status;
    *rebuilt.headers_mut() = headers;
    if let Some(at) = first_event {
        rebuilt.extensions_mut().insert(FirstEventAt(at));
    }
    if let Some(revision) = revision {
        rebuilt.extensions_mut().insert(revision);
    }
    Ok(reqwest::Response::from(rebuilt))
}

pub(crate) async fn execute<F, Fut>(
    state: &ApiState,
    oauth: &VirtualAccess,
    ctx: SupplierContext,
    log: &mut Option<crate::usage::RequestLog>,
    send: F,
) -> Result<reqwest::Response>
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<reqwest::Response, UpstreamError>>,
{
    execute_with_prelude(state, oauth, ctx, log, prelude_event, send).await
}

pub(crate) async fn execute_with_prelude<F, Fut>(
    state: &ApiState,
    oauth: &VirtualAccess,
    mut ctx: SupplierContext,
    log: &mut Option<crate::usage::RequestLog>,
    is_prelude: fn(&serde_json::Value) -> bool,
    mut send: F,
) -> Result<reqwest::Response>
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<reqwest::Response, UpstreamError>>,
{
    let mut excluded = Vec::new();
    let mut refreshed = std::collections::HashSet::new();
    loop {
        if let Some(log) = log {
            log.rebind_supplier(&ctx.account).await?;
        }
        let mut revision = state
            .storage
            .supplier_auth_revision(&ctx.account.id)
            .await?;
        let result = async {
            let response = send(ctx.account.id.clone()).await?;
            revision = response
                .extensions()
                .get::<codex2api_upstream::SupplierAuthRevision>()
                .map(|r| r.0)
                .or(revision);
            inspect_for(&ctx.account.provider_id, is_prelude, response).await
        }
        .await;
        match result {
            Ok(response) => return Ok(response),
            Err(error) => {
                revision = error.auth_revision().or(revision);
                let Some(mut failure) = error
                    .supplier_failure_for(&ctx.account.provider_id, chrono::Utc::now().timestamp())
                else {
                    if let Some(log) = log {
                        log.upstream_failure(&error);
                    }
                    return Err(error.into());
                };
                if failure == SupplierFailure::Authentication
                    && refreshed.insert(ctx.account.id.clone())
                {
                    match recover_stream_auth(state, &ctx, revision).await? {
                        None => {
                            ctx = select(state, oauth, &excluded).await?;
                            continue;
                        }
                        Some(rejection) => failure = rejection,
                    }
                }
                observe(state, &ctx.account.id, revision, &failure).await?;
                if let Some(log) = log {
                    let cause = match &failure {
                        SupplierFailure::QuotaExhausted { code, .. } => {
                            codex2api_upstream::ResponseFailure::new(
                                Some(429),
                                Some(code),
                                Some("Supplier temporarily unavailable"),
                            )
                        }
                        SupplierFailure::Authentication
                        | SupplierFailure::PaymentRequired { .. } => error.failure(),
                    };
                    log.supplier_attempt_failed(cause);
                }
                excluded.push(ctx.account.id.clone());
                ctx = select(state, oauth, &excluded).await?;
                if let Some(log) = log {
                    log.retry_generation();
                }
            }
        }
    }
}

#[cfg(test)]
async fn inspect(
    response: reqwest::Response,
) -> std::result::Result<reqwest::Response, UpstreamError> {
    inspect_for(codex2api_core::CHATGPT, prelude_event, response).await
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use codex2api_accounts::{AuthDotJson, SupplierAccountStore, TokenData};
    use codex2api_storage::{
        OAuthDeviceIdentity, Storage, SupplierAccountUpdate, SupplierStatus, UsageRecord,
        VirtualAccount,
    };
    use serde_json::json;

    #[tokio::test]
    async fn payment_required_switches_http_and_sse_supply_without_rejecting_credentials() {
        for stream in [false, true] {
            let (_dir, state, oauth, ids) = setup_pool().await;
            let first = ids[0].clone();
            let ctx = select(&state, &oauth, &[]).await.unwrap();
            let result = execute(&state, &oauth, ctx, &mut None, move |id| {
                let first = first.clone();
                async move {
                    if id == first {
                        Ok(if stream {
                            response(200, true, "data: {\"type\":\"response.created\",\"response\":{\"id\":\"rejected\"}}\n\ndata: {\"type\":\"error\",\"status\":402,\"error\":{\"message\":\"Payment Required\"}}\n\n")
                        } else {
                            response(402, false, "Payment Required")
                        })
                    } else {
                        Ok(response(200, false, "{\"id\":\"success\"}"))
                    }
                }
            }).await.unwrap();
            assert_eq!(result.text().await.unwrap(), "{\"id\":\"success\"}");
            let health = state.storage.supplier_health(&ids[0]).await.unwrap();
            assert!(health.payment_required);
            assert!(!health.authentication_invalid);
            assert!(health.cooldown_until.is_none());
            assert_eq!(
                state
                    .storage
                    .load_supplier_tokens(&ids[0])
                    .await
                    .unwrap()
                    .unwrap()
                    .access_token
                    .as_deref(),
                Some("fixture")
            );
            assert_eq!(
                select(&state, &oauth, &[]).await.unwrap().account.id,
                ids[1]
            );
        }
    }

    #[tokio::test]
    async fn billing_failure_uses_the_actual_attempt_revision_after_a_token_refresh() {
        let (_dir, state, oauth, ids) = setup_pool().await;
        let first = ids[0].clone();
        let storage = state.storage.clone();
        let ctx = select(&state, &oauth, &[]).await.unwrap();
        let result = execute(&state, &oauth, ctx, &mut None, move |id| {
            let (first, storage) = (first.clone(), storage.clone());
            async move {
                if id == first {
                    let snapshot = storage.supplier_auth_snapshot(&id).await.unwrap().unwrap();
                    let mut tokens = snapshot.tokens;
                    tokens.access_token = Some("refreshed-before-discovery".into());
                    storage
                        .replace_supplier_tokens(&id, snapshot.auth_revision, tokens)
                        .await
                        .unwrap();
                    let revision = storage.supplier_auth_revision(&id).await.unwrap().unwrap();
                    Err(UpstreamError::status(
                        http::StatusCode::PAYMENT_REQUIRED,
                        "Payment Required",
                    )
                    .with_auth_revision(revision))
                } else {
                    Ok(response(200, false, "{}"))
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(result.status(), 200);
        assert!(
            state
                .storage
                .supplier_health(&ids[0])
                .await
                .unwrap()
                .payment_required
        );
        assert_eq!(
            select(&state, &oauth, &[]).await.unwrap().account.id,
            ids[1]
        );
    }

    #[tokio::test]
    async fn request_throttling_is_forwarded_without_changing_supplier_or_health() {
        use axum::response::IntoResponse;
        let (_dir, state, oauth, ids) = setup_pool().await;
        let original = state
            .storage
            .execution_route(&oauth.virtual_account_id, "chatgpt")
            .await
            .unwrap()
            .unwrap();
        for code in [Some("rate_limit_exceeded"), Some("slow_down"), None] {
            let value = json!({"error":{"type":"rate_limit_error","code":code,"message":"Please try again in 7s."}});
            let mut calls = 0;
            let ctx = select(&state, &oauth, &[]).await.unwrap();
            let error = execute(&state, &oauth, ctx, &mut None, |supplier| {
                calls += 1;
                assert_eq!(supplier, ids[0]);
                let mut upstream = response(429, false, &value.to_string());
                upstream
                    .headers_mut()
                    .insert("retry-after", http::HeaderValue::from_static("7"));
                std::future::ready(Ok(upstream))
            })
            .await
            .unwrap_err();
            assert_eq!(calls, 1);
            let response = error.into_response();
            assert_eq!(response.status(), http::StatusCode::TOO_MANY_REQUESTS);
            assert_eq!(response.headers()["retry-after"], "7");
            let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
                .await
                .unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
                serde_json::json!({"error":crate::public_output::error(&value["error"])})
            );
            let health = state.storage.supplier_health(&ids[0]).await.unwrap();
            assert!(!health.authentication_invalid);
            assert!(health.cooldown_kind.is_none());
            assert!(health.cooldown_until.is_none());
            let route = state
                .storage
                .execution_route(&oauth.virtual_account_id, "chatgpt")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(route.supplier_account_id, original.supplier_account_id);
            assert_eq!(route.revision, original.revision);
        }
        let frame = "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"rate_limit_exceeded\",\"message\":\"Please try again in 7s.\"}}}\n\n";
        assert_eq!(
            inspect(response(200, true, frame))
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            frame
        );
    }

    fn response(status: u16, sse: bool, text: &str) -> reqwest::Response {
        http::Response::builder()
            .status(status)
            .header(
                "content-type",
                if sse {
                    "text/event-stream"
                } else {
                    "application/json"
                },
            )
            .body(reqwest::Body::from(text.to_owned()))
            .unwrap()
            .into()
    }

    #[tokio::test]
    async fn prelude_rejections_are_hidden_but_output_is_never_replayed() {
        let prefix = "data: {\"type\":\"response.created\",\"response\":{\"id\":\"r\"}}\n\n";
        let rejection = "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"usage_limit_reached\",\"resets_at\":2000000000}}}\n\n";
        let error = inspect(response(200, true, &format!("{prefix}{rejection}")))
            .await
            .unwrap_err();
        assert!(matches!(
            error.supplier_failure(100),
            Some(SupplierFailure::QuotaExhausted { .. })
        ));
        let output = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"visible\"}\n\n";
        let body = format!("{prefix}{output}{rejection}");
        assert_eq!(
            inspect(response(200, true, &body))
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            body
        );
        let policy = "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"invalid_prompt\"}}}\n\n";
        assert_eq!(
            inspect(response(200, true, policy))
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            policy
        );
    }

    pub(crate) async fn setup_pool() -> (tempfile::TempDir, ApiState, VirtualAccess, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("http-pool.sqlite"))
            .await
            .unwrap();
        let accounts = SupplierAccountStore::open(storage.clone());
        let mut ids = Vec::new();
        for _ in 0..2 {
            let a = accounts.create_pending().await.unwrap().account;
            storage
                .update_account(
                    &a.id,
                    SupplierAccountUpdate {
                        status: Some(SupplierStatus::Active),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            accounts
                .save_auth_for_account(
                    &a.id,
                    &AuthDotJson::chatgpt(
                        TokenData {
                            id_token: "fixture".into(),
                            access_token: "fixture".into(),
                            refresh_token: "fixture".into(),
                            account_id: Some(a.id.clone()),
                        },
                        None,
                    ),
                )
                .await
                .unwrap();
            ids.push(a.id);
        }
        let owner = VirtualAccount {
            provider_id: "chatgpt".into(),
            id: "consumer".into(),
            username: "consumer".into(),
            password_hash: "fixture".into(),
            name: "Consumer".into(),
            email: "consumer@example.test".into(),
            plan_type: "plus".into(),
            plan_id: "plus".into(),
            subscription_expires_at: None,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        storage.save_virtual_account(&owner).await.unwrap();
        storage
            .save_supplier_tag("pool", "chatgpt", "Pool")
            .await
            .unwrap();
        storage
            .edit_supplier_tags(&ids, &["pool".into()], false)
            .await
            .unwrap();
        storage
            .save_pool_route(&owner.id, "chatgpt", Some("pool"), Some(&ids[0]), None)
            .await
            .unwrap();
        let device = storage
            .create_virtual_device(&owner, "refresh", &OAuthDeviceIdentity::default())
            .await
            .unwrap()
            .unwrap();
        storage
            .register_virtual_access(
                &device,
                "refresh",
                "access",
                chrono::Utc::now().timestamp() + 600,
            )
            .await
            .unwrap();
        let oauth = storage
            .virtual_access(&codex2api_storage::hash_token("access"))
            .await
            .unwrap()
            .unwrap();
        let auth = codex2api_auth::AuthService::new(accounts.clone()).unwrap();
        let state = ApiState::new(
            storage.clone(),
            accounts,
            codex2api_upstream::UpstreamPool::new(auth),
        );
        (dir, state, oauth, ids)
    }

    #[tokio::test]
    async fn failover_keeps_one_request_and_attributes_winning_usage_to_selected_supplier() {
        let (_dir, state, oauth, ids) = setup_pool().await;
        let storage = state.storage.clone();
        let owner = storage
            .effective_virtual_account(&oauth.virtual_account_id)
            .await
            .unwrap()
            .unwrap();
        let mut log = Some(
            crate::usage::RequestLog::begin(
                storage.clone(),
                UsageRecord {
                    id: "one-request".into(),
                    account_id: ids[0].clone(),
                    subject_id: owner.id.clone(),
                    endpoint: "/v1/responses".into(),
                    model: Some("gpt-5.5".into()),
                    status: "in_progress".into(),
                    requested_at_ms: chrono::Utc::now().timestamp_millis(),
                    ..Default::default()
                },
                std::time::Instant::now(),
            )
            .await
            .unwrap(),
        );
        let first = ids[0].clone();
        let second = ids[1].clone();
        let ctx = select(&state, &oauth, &[]).await.unwrap();
        let response=execute(&state,&oauth,ctx,&mut log,move |id|{
            let first=first.clone();let second=second.clone();
            async move {
                if id==first {Ok(response(429,false,&json!({"error":{"type":"usage_limit_reached","resets_at":chrono::Utc::now().timestamp()+600}}).to_string()))}
                else {assert_eq!(id,second);Ok(response(200,true,"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"success\",\"usage\":{\"input_tokens\":7,\"output_tokens\":3}}}\n\n"))}
            }
        }).await.unwrap();
        assert_eq!(
            storage
                .execution_route(&owner.id, "chatgpt")
                .await
                .unwrap()
                .unwrap()
                .supplier_account_id
                .as_deref(),
            Some(ids[1].as_str())
        );
        axum::body::to_bytes(log.take().unwrap().wrap(response), 1024 * 1024)
            .await
            .unwrap();
        for _ in 0..100 {
            let row: (String, String, Option<i64>) = sqlx::query_as(
                "SELECT account_id,status,input_tokens FROM usage_records WHERE id='one-request'",
            )
            .fetch_one(storage.pool())
            .await
            .unwrap();
            if row.1 == "completed" {
                assert_eq!(row.0, ids[1]);
                assert_eq!(row.2, Some(7));
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let rows: Vec<(String, Option<i64>)> =
            sqlx::query_as("SELECT status,input_tokens FROM usage_records")
                .fetch_all(storage.pool())
                .await
                .unwrap();
        assert_eq!(rows, vec![("completed".into(), Some(7))]);
    }
}
