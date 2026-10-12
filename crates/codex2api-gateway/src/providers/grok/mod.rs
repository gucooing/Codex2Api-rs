//! Native Grok APIs keep their own headers, OAuth transport and request formats.
pub(crate) mod images;
use crate::{OutputMode, PreparedRequest, Protocol, Result};
use serde_json::{Value, json};

pub(crate) fn prepare(protocol: Protocol, mut body: Value) -> Result<PreparedRequest> {
    let stream = crate::boolean(&body, "stream", false)?;
    let include_usage = if protocol == Protocol::ChatCompletions {
        crate::include_usage(&body, stream)?
    } else {
        if body
            .get("stream_options")
            .is_some_and(|value| !value.is_null())
        {
            return Err(crate::GatewayError::bad_request(
                "Responses stream_options is not supported by this platform",
            ));
        }
        false
    };
    let (upstream_path, output) = match protocol {
        Protocol::Responses => {
            body.as_object_mut()
                .expect("decoded request")
                .remove("stream_options");
            ("responses", OutputMode::Responses)
        }
        Protocol::ChatCompletions => ("chat/completions", OutputMode::ChatCompletions),
    };
    if protocol == Protocol::ChatCompletions && stream {
        body["stream_options"] = json!({"include_usage":true});
    }
    Ok(PreparedRequest {
        body,
        endpoint: protocol.endpoint(),
        upstream_path,
        output,
        stream,
        include_usage,
    })
}
pub(crate) fn prelude(value: &Value) -> bool {
    if value["object"] == "chat.completion.chunk" {
        return value["choices"].as_array().is_some_and(|rows| {
            rows.iter().all(|row| {
                row["delta"]["content"].as_str().is_none_or(str::is_empty)
                    && row["delta"]["tool_calls"].is_null()
                    && row["delta"]["reasoning_content"]
                        .as_str()
                        .is_none_or(str::is_empty)
                    && row["finish_reason"].is_null()
            })
        });
    }
    codex2api_scheduler::prelude_event(value)
}
pub(crate) fn scheduling(
    headers: &http::HeaderMap,
    body: &Value,
) -> codex2api_scheduler::ScheduleRequest {
    use codex2api_scheduler::{body_seed, hash, header_seed, text};
    let seed = header_seed(headers)
        .or_else(|| {
            headers
                .get("x-grok-conv-id")
                .and_then(|v| v.to_str().ok())
                .filter(|v| !v.trim().is_empty())
                .map(str::to_owned)
        })
        .or_else(|| text(body, "prompt_cache_key"))
        .or_else(|| text(body, "previous_response_id").map(|id| format!("grok-prev-resp:{id}")))
        .or_else(|| body_seed(body));
    let model = text(body, "model");
    let session_hash = seed.map(|seed| {
        hash(&format!(
            "grok-affinity:v1:{}:{seed}",
            model.as_deref().unwrap_or("").to_lowercase()
        ))
    });
    codex2api_scheduler::ScheduleRequest {
        model,
        session_hash,
        previous_response: text(body, "previous_response_id"),
        previous_can_move: false,
        ..Default::default()
    }
}

pub(crate) fn outbound_headers() -> http::HeaderMap {
    let mut headers = http::HeaderMap::new();
    for name in ["x-grok-req-id", "x-grok-session-id", "x-grok-conv-id"] {
        headers.insert(
            name,
            uuid::Uuid::new_v4()
                .to_string()
                .parse()
                .expect("UUID header"),
        );
    }
    headers
}
