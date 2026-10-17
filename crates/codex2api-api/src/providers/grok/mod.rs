//! Grok requests use the native xAI protocols and never acquire Codex fields or headers.
pub(crate) mod images;
use crate::{ApiError, OutputMode, PreparedRequest, Protocol, Result};
use http::HeaderMap;
use serde_json::{Value, json};

pub(crate) fn prepare(
    protocol: Protocol,
    body: Value,
    inbound: &HeaderMap,
) -> Result<PreparedRequest> {
    if !matches!(protocol, Protocol::Responses | Protocol::ChatCompletions) {
        return Err(ApiError::bad_request(
            "This endpoint is not offered by the Grok subscription transport",
        ));
    }
    let request = crate::request::ApiRequest::parse(protocol, body)?;
    let mut body = request.body;
    body.as_object_mut()
        .expect("validated body")
        .remove("client_metadata");
    let (upstream_path, output) = match protocol {
        Protocol::Responses => {
            body["store"] = false.into();
            let includes = body
                .as_object_mut()
                .expect("validated body")
                .entry("include")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .expect("validated include");
            if !includes.iter().any(|v| v == "reasoning.encrypted_content") {
                includes.push("reasoning.encrypted_content".into());
            }
            for item in body["input"].as_array_mut().into_iter().flatten() {
                if item["type"] == "reasoning" {
                    item.as_object_mut().expect("input item").remove("status");
                    for part in item["content"].as_array_mut().into_iter().flatten() {
                        if let Some(part) = part.as_object_mut() {
                            part.entry("type")
                                .or_insert_with(|| "reasoning_text".into());
                        }
                    }
                }
            }
            ("responses", OutputMode::Responses)
        }
        Protocol::ChatCompletions => {
            if let Some(limit) = body
                .as_object_mut()
                .expect("validated body")
                .remove("max_completion_tokens")
            {
                body["max_tokens"] = limit;
            }
            for message in body["messages"].as_array_mut().expect("validated messages") {
                if message["role"] == "developer" {
                    message["role"] = "system".into();
                }
                if !message["role"].as_str().is_some_and(|role| {
                    ["system", "user", "assistant", "tool", "function"].contains(&role)
                }) {
                    return Err(ApiError::bad_request("Unsupported message role"));
                }
                if message["content"].is_null()
                    && message["role"] == "assistant"
                    && (message["tool_calls"].is_array() || message["function_call"].is_object())
                {
                    message["content"] = "".into();
                }
                if !message["content"].is_string() && !message["content"].is_array() {
                    return Err(ApiError::bad_request(
                        "Message content must be text or an array",
                    ));
                }
            }
            if request.stream {
                body.as_object_mut()
                    .expect("validated body")
                    .entry("stream_options")
                    .or_insert_with(|| json!({}))["include_usage"] = true.into();
            }
            ("chat/completions", OutputMode::ChatCompletions)
        }
        _ => unreachable!("endpoint checked above"),
    };
    body["stream"] = request.stream.into();
    Ok(PreparedRequest {
        body,
        metadata: request.metadata,
        headers: outbound_headers(inbound),
        endpoint: protocol.endpoint(),
        upstream_path,
        output,
        stream: request.stream,
        include_usage: request.include_usage,
    })
}

pub(crate) use codex2api_upstream::grok::prelude;

pub(crate) fn scheduling(
    headers: &HeaderMap,
    body: &Value,
) -> codex2api_upstream::scheduler::ScheduleRequest {
    use codex2api_upstream::scheduler::{body_seed, hash, header_seed, text};
    let seed = header_seed(headers)
        .or_else(|| {
            headers
                .get("x-grok-conv-id")
                .and_then(|v| v.to_str().ok())
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        })
        .or_else(|| text(body, "prompt_cache_key"))
        .or_else(|| text(body, "previous_response_id"))
        .or_else(|| body_seed(body));
    codex2api_upstream::scheduler::ScheduleRequest {
        model: text(body, "model"),
        session_hash: seed.map(|seed| hash(&seed)),
        previous_response: text(body, "previous_response_id"),
        previous_can_move: false,
        ..Default::default()
    }
}

pub(crate) fn outbound_headers(inbound: &HeaderMap) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for name in ["x-grok-req-id", "x-grok-session-id", "x-grok-conv-id"] {
        if let Some(value) = inbound.get(name) {
            headers.insert(name, value.clone());
        }
    }
    headers.entry("x-grok-req-id").or_insert_with(|| {
        uuid::Uuid::new_v4()
            .to_string()
            .parse()
            .expect("UUID header")
    });
    headers
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn api_rewrite_native_grok_retains_its_api_fields_and_client_context_without_codex_identity() {
        let headers = HeaderMap::from_iter([
            (
                "x-grok-session-id".parse().unwrap(),
                "session".parse().unwrap(),
            ),
            (
                "x-grok-conv-id".parse().unwrap(),
                "conversation".parse().unwrap(),
            ),
        ]);
        let prepared = prepare(Protocol::ChatCompletions, json!({"model":"custom-grok","stream":true,"messages":[{"role":"developer","content":"instructions"},{"role":"user","content":"Hello"}],"temperature":0.4,"max_completion_tokens":123,"parallel_tool_calls":false,"stop":["STOP"],"client_metadata":{"x-codex-installation-id":"caller"},"metadata":{"local":"value"},"stream_options":{"include_usage":false}}), &headers).unwrap();
        assert_eq!(prepared.upstream_path, "chat/completions");
        assert_eq!(prepared.body["max_tokens"], 123);
        assert_eq!(prepared.body["messages"][0]["role"], "system");
        assert_eq!(prepared.body["parallel_tool_calls"], false);
        assert_eq!(prepared.body["stop"], json!(["STOP"]));
        assert_eq!(prepared.body["stream_options"]["include_usage"], true);
        assert_eq!(prepared.headers["x-grok-session-id"], "session");
        assert!(prepared.body.get("client_metadata").is_none());
        assert!(!prepared.headers.contains_key("session-id"));
        assert_eq!(prepared.metadata, Some(json!({"local":"value"})));
        assert!(
            prepare(
                Protocol::Search,
                json!({"model":"custom","commands":{"time":[{"utc_offset":"+08:00"}]}}),
                &headers
            )
            .is_err()
        );
    }
}
