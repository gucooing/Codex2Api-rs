//! Grok requests use the native xAI protocols and never acquire Codex fields or headers.
use crate::protocol::request::declared_tools;
use crate::protocol::{
    Error as ApiError, InboundRequest, PreparedRequest, Protocol, Result, convert,
};
use http::HeaderMap;
use serde_json::{Value, json};

/// Grok Build sends official subscription models over Responses
/// (xai-grok-models/default_models.json `api_backend: responses`) and always streams.
pub(crate) fn prepare(input: &InboundRequest) -> Result<PreparedRequest> {
    if input.protocol.image() {
        let mut prepared = super::api_images::prepare(
            input.body.clone(),
            input.protocol == Protocol::ImageEdit,
            &input.headers,
        )?;
        prepared.metadata = input.metadata.clone();
        return Ok(prepared);
    }
    if !input.protocol.generation() {
        return Err(ApiError::bad_request(
            "This endpoint is not offered by the Grok subscription transport",
        ));
    }
    let mut body = convert(input.protocol, &input.body)?;
    let object = body.as_object_mut().expect("validated body");
    for name in [
        "client_metadata",
        "prompt_cache_retention",
        "safety_identifier",
    ] {
        object.remove(name);
    }
    // Grok Build sends system-level input with the system role.
    for item in object
        .get_mut("input")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if item["role"] == "developer" {
            item["role"] = "system".into();
        }
    }
    object.insert("stream".into(), true.into());
    // The API defaults store to true; Grok Build disables it and replays encrypted reasoning.
    object.insert("store".into(), false.into());
    let include = object.entry("include").or_insert_with(|| json!([]));
    if let Some(include) = include.as_array_mut()
        && !include.iter().any(|v| v == "reasoning.encrypted_content")
    {
        include.push("reasoning.encrypted_content".into());
    }
    let mut declared = Vec::new();
    declared_tools(object.get("tools").unwrap_or(&Value::Null), &mut declared);
    if declared.is_empty() {
        object.remove("tool_choice");
        object.remove("parallel_tool_calls");
    } else if let Some(name) = object
        .get("tool_choice")
        .filter(|c| c["type"] == "function")
        .map(|c| c["name"].as_str().unwrap_or_default().to_owned())
        && !declared
            .iter()
            .any(|(kind, n)| kind == "function" && *n == name)
    {
        object.remove("tool_choice");
    }
    let mut prepared = PreparedRequest::new(input, Protocol::Responses, body);
    prepared.headers = outbound_headers(&input.headers);
    Ok(prepared)
}

pub(crate) fn scheduling(headers: &HeaderMap, body: &Value) -> crate::scheduler::ScheduleRequest {
    use crate::scheduler::{body_seed, hash, header_seed, text};
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
    crate::scheduler::ScheduleRequest {
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
    fn inbound(protocol: Protocol, body: Value, headers: HeaderMap) -> InboundRequest {
        InboundRequest::new(protocol, body, headers).unwrap()
    }
    #[test]
    fn every_inbound_protocol_uses_streamed_responses_without_codex_identity() {
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
        let chat = prepare(&inbound(Protocol::ChatCompletions, json!({"model":"grok-4.6","messages":[{"role":"developer","content":"instructions"},{"role":"user","content":"Hello"}],"temperature":0.4,"max_completion_tokens":123,"client_metadata":{"x-codex-installation-id":"caller"},"metadata":{"local":"value"}}), headers.clone())).unwrap();
        let responses = prepare(&inbound(Protocol::Responses, json!({"model":"grok-4.6","instructions":"policy","input":"Hello",
            "tools":[{"type":"function","function":{"name":"lookup","parameters":{"type":"object"}}}],"tool_choice":{"type":"function","function":{"name":"missing"}}}), headers.clone())).unwrap();
        let messages = prepare(&inbound(
            Protocol::Messages,
            json!({"model":"grok-4.6","max_tokens":8,"messages":[{"role":"user","content":"hi"}]}),
            headers,
        ))
        .unwrap();
        for prepared in [&chat, &responses, &messages] {
            assert_eq!(prepared.upstream.upstream_path(), "responses");
            assert_eq!(prepared.body["stream"], true);
            assert_eq!(prepared.body["store"], false);
            assert_eq!(
                prepared.body["include"],
                json!(["reasoning.encrypted_content"])
            );
            assert!(prepared.body.get("client_metadata").is_none());
            assert_eq!(prepared.headers["x-grok-session-id"], "session");
            assert!(!prepared.headers.contains_key("session-id"));
        }
        assert_eq!(chat.body["input"][0]["role"], "system");
        assert_eq!(chat.body["max_output_tokens"], 123);
        assert_eq!(chat.body["temperature"], 0.4);
        assert!(!chat.stream);
        assert_eq!(chat.metadata, Some(json!({"local":"value"})));
        assert_eq!(
            responses.body["input"][0],
            json!({"type":"message","role":"system","content":[{"type":"input_text","text":"policy"}]})
        );
        assert!(responses.body.get("instructions").is_none());
        assert_eq!(responses.body["tools"][0]["name"], "lookup");
        assert!(responses.body.get("tool_choice").is_none());
        assert_eq!(messages.body["max_output_tokens"], 8);
        assert!(
            prepare(&inbound(
                Protocol::Search,
                json!({"model":"grok-4.6","commands":{"time":[{"utc_offset":"+08:00"}]}}),
                HeaderMap::new()
            ))
            .is_err()
        );
    }
}
