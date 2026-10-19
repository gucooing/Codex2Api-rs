use crate::{
    ApiError, ApiState, InboundRequest, Result,
    runtime::{ApiPermit, Limits},
    usage::ApiLog,
};
use axum::response::Response;
use codex2api_storage::{ApiPrincipal, UsageRecord};
use http::HeaderMap;
use serde_json::Value;

pub(crate) async fn execute_prepared(
    state: ApiState,
    principal: ApiPrincipal,
    token: String,
    headers: HeaderMap,
    prepared: InboundRequest,
) -> Result<Response> {
    // The accepted request and its usage continue if the downstream disconnects.
    tokio::spawn(codex2api_upstream::RequestFailureLog::inherit(run(
        state, principal, token, headers, prepared,
    )))
    .await
    .map_err(|error| {
        tracing::error!(%error,"API request worker failed");
        ApiError::internal()
    })?
}

async fn run(
    state: ApiState,
    principal: ApiPrincipal,
    token: String,
    headers: HeaderMap,
    prepared: InboundRequest,
) -> Result<Response> {
    let user_agent = codex2api_upstream::request_user_agent(&headers);
    let (mut log, permit) = admit(
        &state,
        &principal,
        &prepared.body,
        prepared.protocol.endpoint(),
        prepared.stream,
        "http",
        user_agent.as_deref(),
    )
    .await?;
    let result = crate::dispatch::forward(&state, &principal, &token, &prepared, &mut log).await;
    match result {
        Ok((response, context, route, prepared)) => {
            crate::response::response_body(state, log, permit, context, route, response, prepared)
                .await
        }
        Err(error) => {
            log.fail(&error);
            log.settle().await?;
            Err(error)
        }
    }
}

/// HTTP requests and each WebSocket generation share one admission and ledger.
pub(crate) async fn admit(
    state: &ApiState,
    principal: &ApiPrincipal,
    body: &Value,
    endpoint: &str,
    stream: bool,
    transport: &str,
    user_agent: Option<&str>,
) -> Result<(ApiLog, ApiPermit)> {
    let metadata = codex2api_upstream::request_metadata_value(body);
    let model = metadata
        .model
        .as_deref()
        .filter(|model| codex2api_common::valid_model(model))
        .ok_or_else(|| ApiError::bad_request("model must name a configured model"))?;
    let rule = principal
        .group
        .model_rules
        .iter()
        .find(|rule| rule.model == model);
    let limits = Limits {
        group_concurrency: principal.group.concurrency_limit,
        group_rpm: principal.group.rpm_limit,
        model_concurrency: rule.map_or(0, |rule| rule.concurrency_limit),
        model_rpm: rule.map_or(0, |rule| rule.rpm_limit),
    };
    let model = model.to_owned();
    let clock = codex2api_upstream::RequestClock::current().unwrap_or_default();
    let started = clock.started;
    let record = UsageRecord {
        id: uuid::Uuid::new_v4().to_string(),
        provider_id: principal.group.provider_id.clone(),
        subject_id: principal.key_id.clone(),
        subject_kind: "api_key".into(),
        endpoint: endpoint.into(),
        transport: transport.into(),
        user_agent: user_agent.map(str::to_owned),
        stream: Some(stream),
        model: metadata.model,
        reasoning_effort: metadata.reasoning_effort,
        service_tier: metadata.service_tier,
        image_size: metadata.image_size,
        image_input_usage_json: metadata.image_input_sizes.map(|sizes| {
            serde_json::to_string(
                &sizes
                    .into_iter()
                    .map(|resolution| codex2api_storage::ImageUsage {
                        resolution: Some(resolution),
                        count: 1,
                    })
                    .collect::<Vec<_>>(),
            )
            .expect("image usage")
        }),
        requested_at_ms: clock.requested_at_ms,
        client_ip: clock.client_ip(),
        status: "in_progress".into(),
        ..Default::default()
    };
    state.storage.admit_api_request(principal, &record).await?;
    let mut log = ApiLog::new(state.storage.clone(), record, started);
    let permit = match state
        .runtime
        .acquire(
            &principal.user_id,
            &principal.key_id,
            &principal.group.id,
            &model,
            limits,
        )
        .await
    {
        Ok(permit) => permit,
        Err(error) => {
            let error = ApiError::from(error);
            log.fail(&error);
            log.settle().await?;
            return Err(error);
        }
    };
    Ok((log, permit))
}
