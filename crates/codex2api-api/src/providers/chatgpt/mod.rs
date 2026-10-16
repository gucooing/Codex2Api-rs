//! Public API conversion for the pinned Codex subscription transport.
mod chat;
pub(crate) mod images;
pub(crate) mod websocket;
use crate::{ApiError, OutputMode, PreparedRequest, Protocol, Result};
pub(crate) use chat::{ChatStream, chat_response, chat_to_responses};
use http::HeaderMap;
use serde_json::{Map, Value, json};

pub(crate) fn prepare(
    protocol: Protocol,
    body: Value,
    inbound: &HeaderMap,
) -> Result<PreparedRequest> {
    let request = crate::request::ApiRequest::parse(protocol, body)?;
    let mut body = request.body;
    let headers = request_context(&mut body, inbound)?;
    let (upstream_path, output) = match protocol {
        Protocol::Search => {
            let object = body.as_object_mut().expect("validated body");
            object.remove("client_metadata");
            object.remove("stream_options");
            if let Some(id) = object.get("id") {
                if id.as_str().is_none_or(str::is_empty) {
                    return Err(ApiError::bad_request("Search id must be a nonempty string"));
                }
            } else {
                object.insert("id".into(), uuid::Uuid::new_v4().to_string().into());
            }
            if let Some(input) = object.get("input") {
                if !input.is_string()
                    && !input
                        .as_array()
                        .is_some_and(|v| v.iter().all(Value::is_object))
                {
                    return Err(ApiError::bad_request(
                        "Search input must be text or response items",
                    ));
                }
            }
            if !object.contains_key("input") && !object.contains_key("commands") {
                return Err(ApiError::bad_request("Search requires input or commands"));
            }
            if let Some(commands) = object.get("commands").and_then(Value::as_object) {
                for (name, value) in commands {
                    if name == "response_length" {
                        if !value
                            .as_str()
                            .is_some_and(|s| ["short", "medium", "long"].contains(&s))
                        {
                            return Err(ApiError::bad_request("Invalid search response_length"));
                        }
                    } else if !value
                        .as_array()
                        .is_some_and(|values| values.iter().all(Value::is_object))
                    {
                        return Err(ApiError::bad_request(format!(
                            "commands.{name} must be an array of operations"
                        )));
                    }
                }
            }
            ("alpha/search", OutputMode::Search)
        }
        Protocol::Compact | Protocol::InputTokens => {
            // These are unary protocols, not a generation with stream=false.
            // In particular, their own instructions field is part of the endpoint contract.
            body.as_object_mut()
                .expect("validated body")
                .remove("stream_options");
            body.as_object_mut()
                .expect("validated body")
                .remove("client_metadata");
            normalize_tools(body.as_object_mut().expect("validated body"))?;
            if protocol == Protocol::Compact {
                ("responses/compact", OutputMode::Compact)
            } else {
                ("responses/input_tokens", OutputMode::InputTokens)
            }
        }
        Protocol::Responses | Protocol::ChatCompletions => {
            let output = if protocol == Protocol::ChatCompletions {
                body = chat_to_responses(&body)?;
                OutputMode::ChatFromResponses
            } else {
                OutputMode::Responses
            };
            let object = body.as_object_mut().expect("validated body");
            // The pinned subscription wire protocol does not offer these SDK sampling controls.
            for name in [
                "temperature",
                "top_p",
                "max_tokens",
                "max_completion_tokens",
                "max_output_tokens",
                "frequency_penalty",
                "presence_penalty",
                "prompt_cache_retention",
                "chat_template_kwargs",
                "truncation",
                "stop",
                "stop_sequences",
                "seed",
                "logit_bias",
            ] {
                object.remove(name);
            }
            normalize_tools(object)?;
            let instructions = object.remove("instructions");
            let input = object
                .get_mut("input")
                .and_then(Value::as_array_mut)
                .expect("normalized input");
            if let Some(Value::String(instructions)) =
                instructions.filter(|v| v.as_str().is_some_and(|s| !s.is_empty()))
            {
                input.insert(0, json!({"type":"message","role":"developer","content":[{"type":"input_text","text":instructions}]}));
            }
            for item in input {
                if item["role"] == "system" {
                    item["role"] = "developer".into();
                }
                if item["type"] == "reasoning" {
                    let item = item.as_object_mut().expect("validated input item");
                    // Stateless replay carries reasoning in encrypted_content, never by looking up rs_* IDs.
                    item.remove("id");
                    item.remove("call_id");
                    item.remove("status");
                    if item.get("summary").is_none_or(Value::is_null) {
                        item.insert("summary".into(), json!([]));
                    }
                    for part in item
                        .get_mut("content")
                        .and_then(Value::as_array_mut)
                        .into_iter()
                        .flatten()
                    {
                        if let Some(part) = part.as_object_mut() {
                            part.entry("type")
                                .or_insert_with(|| "reasoning_text".into());
                        }
                    }
                }
                if matches!(
                    item["type"].as_str(),
                    Some("function_call_output" | "custom_tool_call_output")
                ) && item["output"].is_array()
                {
                    item["output"] =
                        crate::request::content_parts(std::mem::take(&mut item["output"]), false)?
                            .into();
                }
                if item["type"] == "additional_tools" || item["type"] == "tool_search_output" {
                    normalize_tools(item.as_object_mut().expect("validated input item"))?;
                }
            }
            object.insert("stream".into(), true.into());
            object.insert("store".into(), false.into());
            object.entry("tool_choice").or_insert_with(|| "auto".into());
            object
                .entry("parallel_tool_calls")
                .or_insert_with(|| true.into());
            object.entry("reasoning").or_insert(Value::Null);
            let include = object
                .entry("include")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .expect("validated include");
            if !include.iter().any(|v| v == "reasoning.encrypted_content") {
                include.push("reasoning.encrypted_content".into());
            }
            if let Some(options) = object
                .get_mut("stream_options")
                .and_then(Value::as_object_mut)
            {
                options.remove("include_obfuscation");
                if options.is_empty() {
                    object.remove("stream_options");
                }
            }
            ("responses", output)
        }
    };
    Ok(PreparedRequest {
        body,
        headers,
        metadata: request.metadata,
        endpoint: protocol.endpoint(),
        upstream_path,
        output,
        stream: request.stream,
        include_usage: request.include_usage,
    })
}

/// Preserve caller-scoped protocol context; account identity is supplied by the upstream transport.
pub(crate) fn request_context(body: &mut Value, inbound: &HeaderMap) -> Result<HeaderMap> {
    let supplied = body
        .as_object_mut()
        .expect("validated body")
        .remove("client_metadata")
        .unwrap_or_else(|| json!({}));
    let mut headers = HeaderMap::new();
    let mut metadata = Map::new();
    let mut inbound = inbound.clone();
    codex2api_upstream::strip_hop_by_hop_headers(&mut inbound);
    for (key, header) in [
        ("session_id", "session-id"),
        ("thread_id", "thread-id"),
        ("x-codex-window-id", "x-codex-window-id"),
        ("x-codex-turn-state", "x-codex-turn-state"),
        ("x-codex-parent-thread-id", "x-codex-parent-thread-id"),
        ("x-openai-subagent", "x-openai-subagent"),
        ("traceparent", "traceparent"),
        ("tracestate", "tracestate"),
    ] {
        let value = inbound
            .get(header)
            .map(|v| {
                v.to_str()
                    .map_err(|_| ApiError::bad_request(format!("Invalid {header}")))
            })
            .transpose()?
            .or_else(|| supplied.get(key).and_then(Value::as_str));
        if supplied.get(key).is_some_and(|v| !v.is_string()) {
            return Err(ApiError::bad_request(format!(
                "client_metadata.{key} must be a string"
            )));
        }
        if let Some(value) = value.filter(|v| !v.is_empty()) {
            let value_header = value
                .parse()
                .map_err(|_| ApiError::bad_request(format!("Invalid {header}")))?;
            headers.insert(header, value_header);
            metadata.insert(key.into(), value.into());
        }
    }
    for key in ["turn_id", "parent_turn_id", "root_turn_id"] {
        if let Some(value) = supplied.get(key) {
            let text = value.as_str().ok_or_else(|| {
                ApiError::bad_request(format!("client_metadata.{key} must be a string"))
            })?;
            metadata.insert(key.into(), text.into());
        }
    }
    // A client request ID identifies this request. It is not a synthetic session or cache key.
    if let Some(value) = inbound
        .get("x-client-request-id")
        .or_else(|| headers.get("thread-id"))
    {
        headers.insert("x-client-request-id", value.clone());
    }
    if !metadata.is_empty() {
        body["client_metadata"] = metadata.into();
    }
    Ok(headers)
}

/// A single recursive tool conversion covers Responses, Chat, namespaces and discovered tool lists.
fn normalize_tools(object: &mut Map<String, Value>) -> Result<()> {
    if let Some(tools) = object.get_mut("tools") {
        for tool in tools
            .as_array_mut()
            .ok_or_else(|| ApiError::bad_request("tools must be an array"))?
        {
            let tool = tool
                .as_object_mut()
                .ok_or_else(|| ApiError::bad_request("Each tool must be an object"))?;
            let kind = tool
                .get("type")
                .and_then(Value::as_str)
                .ok_or_else(|| ApiError::bad_request("Tool type is required"))?
                .to_owned();
            if matches!(kind.as_str(), "function" | "custom") {
                if let Some(nested) = tool.remove(&kind) {
                    let nested = nested.as_object().ok_or_else(|| {
                        ApiError::bad_request(format!("Tool {kind} must be an object"))
                    })?;
                    for (key, value) in nested {
                        tool.entry(key.clone()).or_insert_with(|| value.clone());
                    }
                }
                if !tool
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| !name.is_empty())
                {
                    return Err(ApiError::bad_request(
                        "Function and custom tools require a name",
                    ));
                }
                tool.insert("type".into(), kind.into());
            } else if kind == "namespace" {
                normalize_tools(tool)?;
            }
        }
    }
    if let Some(choice) = object.get_mut("tool_choice") {
        if let Some(choice) = choice.as_object_mut() {
            for kind in ["function", "custom"] {
                if let Some(nested) = choice.remove(kind) {
                    let nested = nested.as_object().ok_or_else(|| {
                        ApiError::bad_request("Named tool_choice must be an object")
                    })?;
                    for (key, value) in nested {
                        choice.entry(key.clone()).or_insert_with(|| value.clone());
                    }
                    choice.insert("type".into(), kind.into());
                }
            }
            if let Some(allowed) = choice.remove("allowed_tools") {
                let allowed = allowed
                    .as_object()
                    .ok_or_else(|| ApiError::bad_request("allowed_tools must be an object"))?;
                choice.extend(allowed.clone());
            }
            if choice.get("type").is_some_and(|v| v == "allowed_tools") {
                normalize_tools(choice)?;
            }
        } else if !choice
            .as_str()
            .is_some_and(|v| ["auto", "none", "required"].contains(&v))
        {
            return Err(ApiError::bad_request("Invalid tool_choice"));
        }
    }
    Ok(())
}

pub(crate) fn image_headers(inbound: &HeaderMap) -> Result<HeaderMap> {
    let mut headers = request_context(&mut json!({}), inbound)?;
    headers.insert(
        "x-codex-image-turn-id",
        inbound
            .get("x-codex-image-turn-id")
            .cloned()
            .unwrap_or_else(|| {
                uuid::Uuid::new_v4()
                    .to_string()
                    .parse()
                    .expect("UUID header")
            }),
    );
    Ok(headers)
}

pub(crate) fn scheduling(
    headers: &HeaderMap,
    body: &Value,
) -> codex2api_upstream::scheduler::ScheduleRequest {
    use codex2api_upstream::scheduler::{body_seed, hash, header_seed, text};
    let seed = header_seed(headers)
        .or_else(|| text(body, "prompt_cache_key"))
        .or_else(|| body_seed(body));
    codex2api_upstream::scheduler::ScheduleRequest {
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
    fn api_rewrite_responses_preserve_api_semantics_and_only_forward_dynamic_client_context() {
        let headers = HeaderMap::from_iter([(
            "session-id".parse().unwrap(),
            "client-session".parse().unwrap(),
        )]);
        let prepared = prepare(Protocol::Responses, json!({"model":"custom-model","input":[
            {"role":"system","content":"system"},
            {"type":"reasoning","id":"rs_old","encrypted_content":"ciphertext"},
            {"type":"custom_tool_call","call_id":"ctc_original","name":"code","namespace":"tools","input":"print(1)"},
            {"type":"custom_tool_call_output","call_id":"ctc_original","output":[{"type":"text","text":"ok"},{"type":"input_image","file_id":"file_1"}]}],
            "instructions":"base","tools":[{"type":"namespace","name":"tools","tools":[{"type":"custom","custom":{"name":"code","format":{"type":"text"}}}]},{"type":"web_search"},{"type":"image_generation"}],
            "include":["web_search_call.action.sources"],"prompt_cache_key":" caller key ",
            "client_metadata":{"thread_id":"client-thread","turn_id":"client-turn","session_id":"body-session","x-codex-installation-id":"attacker","access_programs":"attacker"},
            "metadata":{"order":"local"},"access_programs":{"cyber":true},"temperature":0.5}), &headers).unwrap();
        assert_eq!(prepared.body["input"][0]["role"], "developer");
        assert_eq!(prepared.body["input"][1]["role"], "developer");
        assert_eq!(prepared.body["input"][2]["encrypted_content"], "ciphertext");
        assert_eq!(prepared.body["input"][2]["summary"], json!([]));
        assert!(prepared.body["input"][2].get("id").is_none());
        assert_eq!(prepared.body["input"][4]["output"][0]["type"], "input_text");
        assert_eq!(prepared.body["tools"][0]["tools"][0]["name"], "code");
        assert_eq!(
            prepared.body["include"],
            json!([
                "web_search_call.action.sources",
                "reasoning.encrypted_content"
            ])
        );
        assert_eq!(prepared.body["prompt_cache_key"], " caller key ");
        assert_eq!(prepared.headers["session-id"], "client-session");
        assert_eq!(prepared.headers["thread-id"], "client-thread");
        assert_eq!(prepared.body["client_metadata"]["turn_id"], "client-turn");
        assert!(
            prepared.body["client_metadata"]
                .get("x-codex-installation-id")
                .is_none()
        );
        assert!(prepared.body.get("access_programs").is_none());
        assert!(prepared.body.get("instructions").is_none());
        assert_eq!(prepared.metadata, Some(json!({"order":"local"})));
    }

    #[test]
    fn api_rewrite_unary_protocols_keep_their_own_wire_contract_and_client_ids_are_not_invented() {
        let search = prepare(Protocol::Search, json!({"model":"custom-model","id":"search-session","commands":{"search_query":[{"q":"query","recency":7}],"image_query":[{"q":"image"}],"open":[{"ref_id":"turn0search0"}],"response_length":"long"},"settings":{"external_web_access":"live"},"max_output_tokens":123}), &HeaderMap::new()).unwrap();
        assert_eq!(search.upstream_path, "alpha/search");
        assert_eq!(search.body["id"], "search-session");
        assert_eq!(search.body["max_output_tokens"], 123);
        for field in [
            "input",
            "tools",
            "stream",
            "store",
            "include",
            "client_metadata",
        ] {
            assert!(search.body.get(field).is_none(), "{field}");
        }
        assert!(!search.headers.contains_key("session-id"));
        for protocol in [Protocol::Compact, Protocol::InputTokens] {
            let request = prepare(
                protocol,
                json!({"model":"custom","input":"hello","instructions":"count this"}),
                &HeaderMap::new(),
            )
            .unwrap();
            assert_eq!(request.body["instructions"], "count this");
            assert!(!request.stream);
            assert!(request.body.get("store").is_none());
            assert!(request.body.get("stream").is_none());
        }
    }
}
