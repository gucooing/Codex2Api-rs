//! Public request validation and shared content normalization.
use super::{Error as ApiError, Protocol, Result};
use serde_json::{Map, Value, json};

pub(crate) struct Parsed {
    pub body: Value,
    pub metadata: Option<Value>,
    pub stream: bool,
    pub include_usage: bool,
}

/// Supplier identity and entitlement controls are never caller-owned.
pub fn strip_client_fields(body: &mut Value) {
    if let Some(object) = body.as_object_mut() {
        for name in ["metadata", "user", "safety_identifier", "access_programs"] {
            object.remove(name);
        }
    }
}

/// Validate a public request once and separate local response preferences.
pub(crate) fn parse(protocol: Protocol, mut body: Value) -> Result<Parsed> {
    let object = body
        .as_object_mut()
        .ok_or_else(|| ApiError::bad_request("Request body must be an object"))?;
    let metadata = object.remove("metadata");
    if metadata.as_ref().is_some_and(|v| !v.is_object()) {
        return Err(ApiError::bad_request("metadata must be an object"));
    }
    if protocol.image() {
        strip_client_fields(&mut body);
        body.as_object_mut()
            .expect("object")
            .remove("client_metadata");
        return Ok(Parsed {
            body,
            metadata,
            stream: false,
            include_usage: false,
        });
    }
    if protocol == Protocol::Messages {
        super::messages::validate(&body)?;
    }
    let object = body.as_object_mut().expect("object");
    for field in ["functions", "function_call"] {
        if object.contains_key(field) {
            return Err(ApiError::bad_request(format!(
                "Unsupported request field: {field}"
            )));
        }
    }
    if protocol == Protocol::ChatCompletions {
        for message in object
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if message.get("function_call").is_some() || message["role"] == "function" {
                return Err(ApiError::bad_request(
                    "Legacy function messages are not supported",
                ));
            }
        }
    }
    object.retain(|_, value| !value.is_null());
    let stream = super::boolean(&body, "stream", false)?;
    if stream
        && matches!(
            protocol,
            Protocol::Search | Protocol::Compact | Protocol::InputTokens
        )
    {
        return Err(ApiError::bad_request(
            "This endpoint returns JSON and does not stream",
        ));
    }
    let mut include_usage = false;
    if let Some(options) = body
        .as_object_mut()
        .expect("object")
        .remove("stream_options")
    {
        let options = options.as_object().filter(|_| stream).ok_or_else(|| {
            ApiError::bad_request("stream_options requires stream=true and an object")
        })?;
        for name in ["include_usage", "include_obfuscation"] {
            if let Some(value) = options.get(name) {
                let enabled = value.as_bool().ok_or_else(|| {
                    ApiError::bad_request(format!("stream_options.{name} must be a boolean"))
                })?;
                if name == "include_usage" {
                    include_usage = enabled;
                }
            }
        }
    }
    if super::boolean(&body, "background", false)? {
        return Err(ApiError::bad_request(
            "The subscription upstream does not support background jobs",
        ));
    }
    super::boolean(&body, "store", false)?;
    if body.get("n").is_some_and(|v| v.as_u64() != Some(1)) {
        return Err(ApiError::bad_request(
            "This generation endpoint supports n=1",
        ));
    }
    strip_client_fields(&mut body);
    let object = body.as_object_mut().expect("object");
    for name in ["stream", "store", "background", "n"] {
        object.remove(name);
    }
    let model = object
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| codex2api_common::valid_model(model))
        .ok_or_else(|| ApiError::bad_request("model must name a configured model"))?
        .to_owned();
    object.insert("model".into(), model.into());
    for (name, value) in object.iter() {
        let valid = match name.as_str() {
            "instructions"
            | "service_tier"
            | "prompt_cache_key"
            | "previous_response_id"
            | "reasoning_effort"
            | "verbosity" => value.is_string(),
            "parallel_tool_calls" | "logprobs" | "generate" => value.is_boolean(),
            "reasoning" | "text" | "response_format" | "search_parameters"
            | "web_search_options" | "logit_bias" | "client_metadata" | "settings" | "commands" => {
                value.is_object()
            }
            "tools" | "messages" => value.is_array(),
            "include" => value
                .as_array()
                .is_some_and(|items| items.iter().all(Value::is_string)),
            "temperature" => value.as_f64().is_some_and(|n| (0.0..=2.0).contains(&n)),
            "top_p" => value.as_f64().is_some_and(|n| (0.0..=1.0).contains(&n)),
            "frequency_penalty" | "presence_penalty" => {
                value.as_f64().is_some_and(|n| (-2.0..=2.0).contains(&n))
            }
            "max_tokens" | "max_completion_tokens" | "max_output_tokens" | "max_tool_calls" => {
                value.as_u64().is_some_and(|n| n > 0)
            }
            _ => true,
        };
        if !valid {
            return Err(ApiError::bad_request(format!("Invalid {name}")));
        }
    }
    match protocol {
        Protocol::Responses | Protocol::Compact | Protocol::InputTokens => {
            if protocol == Protocol::InputTokens {
                object.entry("input").or_insert_with(|| json!([]));
            }
            normalize_input(object)?;
        }
        Protocol::ChatCompletions => {
            if object
                .get("messages")
                .and_then(Value::as_array)
                .is_none_or(|messages| {
                    messages.is_empty() || messages.iter().any(|m| !m.is_object())
                })
            {
                return Err(ApiError::bad_request(
                    "messages must be a nonempty array of objects",
                ));
            }
        }
        Protocol::Messages | Protocol::Search => {}
        Protocol::ImageGeneration | Protocol::ImageEdit => unreachable!("images return early"),
    }
    Ok(Parsed {
        body,
        metadata,
        stream,
        include_usage,
    })
}

pub(crate) fn normalize_input(object: &mut Map<String, Value>) -> Result<()> {
    if !object.contains_key("input") {
        if object.contains_key("previous_response_id")
            || object.contains_key("prompt")
            || object
                .get("instructions")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty())
            || object.get("generate") == Some(&Value::Bool(false))
        {
            object.insert("input".into(), json!([]));
        } else {
            return Err(ApiError::bad_request("input is required"));
        }
    }
    let input = object.get_mut("input").expect("input inserted");
    if input.is_string() {
        let text = std::mem::take(input);
        *input =
            json!([{"type":"message","role":"user","content":[{"type":"input_text","text":text}]}]);
    }
    for item in input
        .as_array_mut()
        .ok_or_else(|| ApiError::bad_request("input must be a string or array"))?
    {
        let item = item
            .as_object_mut()
            .ok_or_else(|| ApiError::bad_request("input items must be objects"))?;
        if item.get("type").is_some_and(|kind| kind != "message") {
            continue;
        }
        let role = item
            .get("role")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError::bad_request("Input message role is required"))?;
        if !matches!(role, "system" | "developer" | "user" | "assistant") {
            return Err(ApiError::bad_request("Invalid input message role"));
        }
        let assistant = role == "assistant";
        let content = item
            .get_mut("content")
            .ok_or_else(|| ApiError::bad_request("Message content is required"))?;
        *content = content_parts(std::mem::take(content), assistant)?.into();
        item.insert("type".into(), "message".into());
    }
    Ok(())
}

/// Convert Chat content aliases to Responses parts without dropping multimodal parts.
pub(crate) fn content_parts(content: Value, assistant: bool) -> Result<Vec<Value>> {
    let text_type = if assistant {
        "output_text"
    } else {
        "input_text"
    };
    let mut parts = match content {
        Value::String(text) => return Ok(vec![json!({"type":text_type,"text":text})]),
        Value::Array(parts) => parts,
        _ => {
            return Err(ApiError::bad_request(
                "Message content must be text or an array",
            ));
        }
    };
    for part in &mut parts {
        let object = part
            .as_object_mut()
            .ok_or_else(|| ApiError::bad_request("Content parts must be objects"))?;
        match object.get("type").and_then(Value::as_str) {
            Some("text" | "input_text" | "output_text") => {
                if !object.get("text").is_some_and(Value::is_string) {
                    return Err(ApiError::bad_request("Text content must be a string"));
                }
                object.insert("type".into(), text_type.into());
            }
            Some("image_url") => {
                let image = object
                    .remove("image_url")
                    .ok_or_else(|| ApiError::bad_request("image_url is required"))?;
                let url = image
                    .as_str()
                    .or_else(|| image["url"].as_str())
                    .ok_or_else(|| ApiError::bad_request("image_url.url is required"))?;
                object.insert("type".into(), "input_image".into());
                object.insert("image_url".into(), url.into());
                if let Some(detail) = image.get("detail") {
                    object.insert("detail".into(), detail.clone());
                }
            }
            Some("file") => {
                let file = object
                    .remove("file")
                    .and_then(|v| v.as_object().cloned())
                    .ok_or_else(|| ApiError::bad_request("file must be an object"))?;
                object.extend(file);
                object.insert("type".into(), "input_file".into());
            }
            Some("input_audio") if object.contains_key("input_audio") => {
                let audio = object.remove("input_audio").expect("present audio");
                let data = audio["data"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| ApiError::bad_request("input_audio.data is required"))?;
                let mime = match audio["format"].as_str() {
                    Some("wav") => "audio/wav",
                    Some("mp3") => "audio/mpeg",
                    _ => {
                        return Err(ApiError::bad_request(
                            "input_audio.format must be wav or mp3",
                        ));
                    }
                };
                object.insert(
                    "audio_url".into(),
                    format!("data:{mime};base64,{data}").into(),
                );
            }
            Some(_) => {}
            None => return Err(ApiError::bad_request("Content part type is required")),
        }
    }
    Ok(parts)
}

/// One recursive tool normalization covers Responses, Chat, namespaces and discovered tools.
pub(crate) fn normalize_tools(object: &mut Map<String, Value>) -> Result<()> {
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
                if tool
                    .get("name")
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty)
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

/// Function tools declared in a normalized Responses tool list, including namespaces.
pub(crate) fn declared_tools(tools: &Value, out: &mut Vec<(String, String)>) {
    for tool in tools.as_array().into_iter().flatten() {
        let kind = tool["type"].as_str().unwrap_or_default();
        if kind == "namespace" {
            declared_tools(&tool["tools"], out);
        }
        out.push((
            kind.to_owned(),
            tool["name"].as_str().unwrap_or_default().to_owned(),
        ));
    }
}
