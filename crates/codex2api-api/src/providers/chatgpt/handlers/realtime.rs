use crate::{ApiState, Result};
use axum::body::{Body, Bytes};
use axum::extract::{Extension, OriginalUri, Path, State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::Response;
use codex2api_upstream::{RealtimeKind, strip_hop_by_hop_headers};
use std::collections::HashMap;

pub async fn call(
    _: crate::user_agent::AllowedUserAgent,
    State(state): State<ApiState>,
    Extension(kind): Extension<RealtimeKind>,
    oauth: Extension<codex2api_storage::VirtualAccess>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    let virtual_id = oauth.virtual_account_id.clone();
    if query_parameter(uri.query(), "call_id")?.is_some() {
        return Err(crate::ApiError::bad_request(
            "Call creation cannot select an existing call.",
        ));
    }
    let query_model = query_parameter(uri.query(), "model")?;
    let models = codex2api_upstream::realtime_call_models(body.clone(), &headers).await?;
    let model = resolve_model(query_model, models.model)?;
    let (key, ctx) =
        crate::providers::chatgpt::access::resolve_supplier(&state, &headers, oauth).await?;
    let context = crate::execution::ExecutionContext::new(
        state.storage.clone(),
        &ctx.account,
        &key.id,
        &key.name,
        if kind == RealtimeKind::Wham {
            "/backend-api/wham/realtime/calls"
        } else {
            "/v1/realtime/calls"
        },
        "http",
    );
    if let Some(model) = &models.transcription_model {
        context
            .authorize(
                &codex2api_upstream::RequestMetadata {
                    model: Some(model.clone()),
                    ..Default::default()
                },
                true,
            )
            .await?;
    }
    let mut log = context
        .start(
            codex2api_upstream::RequestMetadata {
                model: Some(model.clone()),
                ..Default::default()
            },
            std::time::Instant::now(),
            chrono::Utc::now().timestamp_millis(),
        )
        .await?;
    let client = state.upstream.get(&ctx.account.id).await?;
    let response = match client
        .forward_realtime_call(kind, uri.query(), body, headers)
        .await
    {
        Ok(response) => response,
        Err(error) => {
            log.upstream_failure(&error);
            return Err(error.into());
        }
    };
    let mut response_headers = response.headers().clone();
    log.http_status(response.status().as_u16());
    log.response_headers(response.headers());
    if !response.status().is_success() {
        return Ok(crate::response::forward_response(
            response.status(),
            response_headers,
            log.wrap(response),
        ));
    }
    let call_id = response_headers
        .get("location")
        .and_then(|value| value.to_str().ok())
        .and_then(|location| location.split('?').next())
        .and_then(|path| path.rsplit('/').next())
        .filter(|id| {
            kind != RealtimeKind::Wham
                || id
                    .strip_prefix("rtc_")
                    .is_some_and(|suffix| !suffix.is_empty())
        })
        .filter(|id| {
            codex2api_upstream::realtime_url(RealtimeKind::CodexSideband, Some(id), None).is_ok()
        })
        .map(str::to_owned);
    let Some(call_id) = call_id else {
        log.failure(
            "invalid_realtime_response",
            "上游语音响应缺少有效的通话标识",
        );
        log.finish("failed");
        return Err(crate::ApiError::openai(
            axum::http::StatusCode::BAD_GATEWAY,
            "api_error",
            "Upstream realtime response has no valid call ID.",
            Some("invalid_realtime_response"),
        ));
    };
    let status = response.status();
    let answer = match response.bytes().await {
        Ok(answer) => answer,
        Err(error) => {
            let error = codex2api_upstream::UpstreamError::from(error);
            log.upstream_failure(&error);
            return Err(error.into());
        }
    };
    if answer.is_empty() || std::str::from_utf8(&answer).is_err() {
        log.failure("invalid_realtime_response", "上游语音响应未提供 SDP 文本");
        log.finish("failed");
        return Err(crate::ApiError::openai(
            axum::http::StatusCode::BAD_GATEWAY,
            "api_error",
            "Upstream realtime response has no SDP text.",
            Some("invalid_realtime_response"),
        ));
    }
    state.storage.save_virtual_resource(
        &virtual_id, "realtime_call", &call_id, Some(&ctx.account.id),
        &serde_json::json!({"id":call_id,"model":model,"transcription_model":models.transcription_model,"status":"created","created_at_ms":chrono::Utc::now().timestamp_millis()}),
    ).await?;
    log.finish("submitted");
    crate::providers::chatgpt::identity::quota_headers(
        &state.storage,
        &virtual_id,
        &mut response_headers,
    )
    .await?;
    Ok(crate::response::forward_response(
        status,
        response_headers,
        Body::from(answer),
    ))
}

pub async fn socket(
    (_, Extension(kind)): (crate::user_agent::AllowedUserAgent, Extension<RealtimeKind>),
    State(state): State<ApiState>,
    oauth: Extension<codex2api_storage::VirtualAccess>,
    Path(parameters): Path<HashMap<String, String>>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    let mut model = query_parameter(uri.query(), "model")?;
    let query_call = query_parameter(uri.query(), "call_id")?;
    let path_call = parameters.get("call_id");
    if query_call
        .as_ref()
        .zip(path_call)
        .is_some_and(|(a, b)| a != b)
    {
        return Err(crate::ApiError::bad_request(
            "Realtime call identifiers disagree.",
        ));
    }
    let call = query_call.as_ref().or(path_call);
    let mut transcription_model = None;
    {
        let access = &oauth;
        if let Some(call) = call {
            let (source, call_record) = state
                .storage
                .virtual_resource(&access.virtual_account_id, "realtime_call", call)
                .await?
                .ok_or_else(super::virtual_data::not_found)?;
            model = Some(resolve_model(
                model,
                call_record["model"].as_str().map(str::to_owned),
            )?);
            transcription_model = call_record["transcription_model"]
                .as_str()
                .map(str::to_owned);
            if source.is_none() || source != access.account_id {
                return Err(super::virtual_data::not_found());
            }
        }
        crate::providers::chatgpt::access::check_unpriced_execution(
            &state.storage,
            &access.virtual_account_id,
        )
        .await?;
    }
    let (key, ctx) =
        crate::providers::chatgpt::access::resolve_supplier(&state, &headers, oauth).await?;
    let model = resolve_model(model, None)?;
    let context = crate::execution::ExecutionContext::new(
        state.storage.clone(),
        &ctx.account,
        &key.id,
        &key.name,
        "/v1/realtime",
        "websocket",
    );
    context
        .authorize(
            &codex2api_upstream::RequestMetadata {
                model: Some(model.clone()),
                ..Default::default()
            },
            false,
        )
        .await?;
    if let Some(model) = &transcription_model {
        context
            .authorize(
                &codex2api_upstream::RequestMetadata {
                    model: Some(model.clone()),
                    ..Default::default()
                },
                true,
            )
            .await?;
    }
    let client = state.upstream.get(&ctx.account.id).await?;
    let (upstream, mut headers) = client
        .connect_realtime(
            kind,
            parameters.get("call_id").map(String::as_str),
            uri.query(),
            headers,
        )
        .await?;
    crate::providers::chatgpt::identity::quota_headers(&state.storage, &key.id, &mut headers)
        .await?;
    strip_hop_by_hop_headers(&mut headers);
    for name in [
        "sec-websocket-accept",
        "sec-websocket-extensions",
        "sec-websocket-protocol",
        "set-cookie",
        "content-length",
    ] {
        headers.remove(name);
    }
    let identity = client.identity().installation_id.clone();
    let mut ledger = crate::usage::WsLedger::realtime(context, model);
    ledger.transcription_model = transcription_model;
    let mut response = upgrade
        .max_message_size(codex2api_upstream::MAX_REQUEST_BYTES)
        .on_upgrade(move |socket| {
            super::websocket::bridge_recorded(
                socket,
                upstream,
                identity,
                true,
                None,
                ledger,
                (state.storage, key.access),
                None,
            )
        });
    response.headers_mut().extend(headers);
    Ok(response)
}

fn query_parameter(query: Option<&str>, name: &str) -> Result<Option<String>> {
    let mut model = None;
    for (key, value) in url::form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        if key == name && model.replace(value.into_owned()).is_some() {
            return Err(crate::ApiError::bad_request(
                "Duplicate realtime parameter.",
            ));
        }
    }
    Ok(model)
}

fn resolve_model(first: Option<String>, second: Option<String>) -> Result<String> {
    match (first, second) {
        (Some(a), Some(b)) if a != b => Err(crate::ApiError::bad_request(
            "Realtime model parameters disagree.",
        )),
        (Some(model), _) | (_, Some(model)) if codex2api_core::valid_model(&model) => Ok(model),
        _ => Err(crate::ApiError::bad_request(
            "An explicit realtime model is required.",
        )),
    }
}
