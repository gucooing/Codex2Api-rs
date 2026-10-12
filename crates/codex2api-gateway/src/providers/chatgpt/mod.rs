//! OpenAI protocol conversion for the official Codex Responses transport.
mod chat;
pub(crate) mod images;
use crate::{GatewayError, OutputMode, PreparedRequest, Protocol, Result};
pub(crate) use chat::{ChatStream, chat_response, chat_to_responses};
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
            return Err(GatewayError::bad_request(
                "Responses stream_options is not supported by this platform",
            ));
        }
        false
    };
    let output = match protocol {
        Protocol::Responses => {
            validate_responses(&mut body)?;
            body.as_object_mut()
                .expect("decoded request")
                .remove("stream_options");
            if !body
                .get("input")
                .is_some_and(|v| v.is_string() || v.is_array())
            {
                return Err(GatewayError::bad_request("input must be a string or array"));
            }
            if body
                .get("store")
                .is_some_and(|value| value != false && !value.is_null())
            {
                return Err(GatewayError::bad_request(
                    "This platform supports only store=false",
                ));
            }
            if let Some(input) = body["input"].as_str() {
                body["input"] =
                    json!([{"role":"user","content":[{"type":"input_text","text":input}]}]);
            }
            OutputMode::Responses
        }
        Protocol::ChatCompletions => {
            body = chat_to_responses(&body)?;
            OutputMode::ChatFromResponses
        }
    };
    body["stream"] = true.into();
    body["store"] = false.into();
    // Pinned core/client.rs builds these fields for a normal Responses generation.
    let object = body.as_object_mut().expect("decoded request");
    if object.get("instructions").is_some_and(|value| value == "") {
        object.remove("instructions");
    }
    object.entry("tool_choice").or_insert_with(|| "auto".into());
    object
        .entry("parallel_tool_calls")
        .or_insert_with(|| true.into());
    object.entry("reasoning").or_insert(Value::Null);
    object
        .entry("include")
        .or_insert_with(|| json!(["reasoning.encrypted_content"]));
    Ok(PreparedRequest {
        body,
        endpoint: protocol.endpoint(),
        upstream_path: "responses",
        output,
        stream,
        include_usage,
    })
}

fn validate_responses(body: &mut Value) -> Result<()> {
    let object = body.as_object_mut().expect("decoded request");
    // Only the public business controls supported by the pinned Codex request are accepted.
    // Installation metadata and access-program controls belong to the supplier transport.
    for (name, value) in object.iter() {
        if !matches!(
            name.as_str(),
            "model"
                | "instructions"
                | "input"
                | "tools"
                | "tool_choice"
                | "parallel_tool_calls"
                | "reasoning"
                | "store"
                | "stream"
                | "stream_options"
                | "include"
                | "service_tier"
                | "prompt_cache_key"
                | "text"
                | "previous_response_id"
        ) && !value.is_null()
        {
            return Err(GatewayError::bad_request(format!(
                "Unsupported Responses parameter for this platform: {name}"
            )));
        }
    }
    object.retain(|_, value| !value.is_null());
    for name in [
        "model",
        "instructions",
        "service_tier",
        "prompt_cache_key",
        "previous_response_id",
    ] {
        if object.get(name).is_some_and(|value| !value.is_string()) {
            return Err(GatewayError::bad_request(format!(
                "{name} must be a string"
            )));
        }
    }
    for name in ["parallel_tool_calls", "store", "stream"] {
        if object.get(name).is_some_and(|value| !value.is_boolean()) {
            return Err(GatewayError::bad_request(format!(
                "{name} must be a boolean"
            )));
        }
    }
    for name in ["reasoning", "text"] {
        if object.get(name).is_some_and(|value| !value.is_object()) {
            return Err(GatewayError::bad_request(format!(
                "{name} must be an object"
            )));
        }
    }
    if object.get("tools").is_some_and(|value| !value.is_array()) {
        return Err(GatewayError::bad_request("tools must be an array"));
    }
    if object.get("include").is_some_and(|value| {
        value
            .as_array()
            .is_none_or(|values| values.iter().any(|value| !value.is_string()))
    }) {
        return Err(GatewayError::bad_request(
            "include must be an array of strings",
        ));
    }
    Ok(())
}

pub(crate) fn outbound_headers(inbound: &http::HeaderMap, images: bool) -> http::HeaderMap {
    let mut headers = codex2api_upstream::RequestHeaders::new_thread().to_header_map();
    // Only our image handler creates this request-scoped ID; incoming image values are replaced.
    if images && let Some(value) = inbound.get("x-codex-image-turn-id") {
        headers.insert("x-codex-image-turn-id", value.clone());
    }
    headers
}

pub(crate) fn scheduling(
    headers: &http::HeaderMap,
    body: &Value,
) -> codex2api_scheduler::ScheduleRequest {
    use codex2api_scheduler::{body_seed, hash, header_seed, text};
    let seed = header_seed(headers)
        .or_else(|| text(body, "prompt_cache_key"))
        .or_else(|| body_seed(body));
    codex2api_scheduler::ScheduleRequest {
        model: text(body, "model"),
        session_hash: seed.map(|seed| hash(&seed)),
        previous_response: text(body, "previous_response_id"),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn responses_use_pinned_defaults_and_reject_unsupported_controls() {
        let prepared = prepare(
            Protocol::Responses,
            json!({"model":"custom-model","input":"Hello","instructions":"","stream":false}),
        )
        .unwrap();
        assert!(!prepared.stream);
        assert_eq!(prepared.body["stream"], true);
        assert_eq!(prepared.body["store"], false);
        assert_eq!(prepared.body["tool_choice"], "auto");
        assert_eq!(prepared.body["parallel_tool_calls"], true);
        assert_eq!(
            prepared.body["include"],
            json!(["reasoning.encrypted_content"])
        );
        assert_eq!(prepared.body["input"][0]["content"][0]["text"], "Hello");
        assert!(prepared.body.get("instructions").is_none());
        for (name, value) in [
            ("temperature", json!(0.5)),
            ("top_p", json!(0.9)),
            ("max_output_tokens", json!(42)),
            ("client_metadata", json!({"supplier":"caller"})),
            ("access_programs", json!({})),
            ("stream_options", json!({"include_usage":true})),
            ("instructions", json!([])),
        ] {
            let mut body = json!({"model":"custom-model","input":"Hello"});
            body[name] = value;
            assert!(prepare(Protocol::Responses, body).is_err(), "{name}");
        }
        let mut inbound = http::HeaderMap::new();
        inbound.insert(
            "x-codex-image-turn-id",
            "caller-controlled".parse().unwrap(),
        );
        assert!(!outbound_headers(&inbound, false).contains_key("x-codex-image-turn-id"));
    }
}
