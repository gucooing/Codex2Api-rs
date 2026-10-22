//! Requests for the Responses-family wires, built directly from each downstream
//! format. The three dialects differ in the fields the official client sends and the
//! fields the endpoint accepts, applied while the request is built.
use crate::inbound::messages::{blocks, image_url, schema, system_texts, text};
use crate::inbound::request::{content_parts, declared_tools, normalize_tools};
use crate::inbound::{Error as ApiError, Inbound, InboundRequest, Result};
use http::HeaderMap;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

/// Which official Responses endpoint the request is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    Codex,
    Xai,
    OpenAi,
}

/// Public SDK fields the ChatGPT subscription Responses endpoint does not accept.
const CODEX_UNSUPPORTED: [&str; 18] = [
    "max_output_tokens",
    "max_completion_tokens",
    "max_tokens",
    "temperature",
    "top_p",
    "frequency_penalty",
    "presence_penalty",
    "chat_template_kwargs",
    "user",
    "metadata",
    "prompt_cache_retention",
    "safety_identifier",
    "stream_options",
    "truncation",
    "stop",
    "stop_sequences",
    "seed",
    "logit_bias",
];

fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request(format!("{key} must be a nonempty string")))
}

/// The wire request body and request-scoped headers for `dialect`, built from the
/// downstream request in its own format. A native client's request is already in
/// its wire's shape; the channel binds the supplier identity.
pub fn build(input: &InboundRequest, dialect: Dialect) -> Result<(Value, HeaderMap)> {
    let mut body = match input.format {
        Inbound::CodexClient | Inbound::GrokClient => {
            return Ok((input.body.clone(), input.headers.clone()));
        }
        Inbound::Responses => from_responses(&input.body)?,
        Inbound::Chat => from_chat(&input.body)?,
        Inbound::Messages => from_messages(&input.body)?,
    };
    let headers = match dialect {
        Dialect::Codex => {
            let headers =
                crate::providers::chatgpt::api::request_context(&mut body, &input.headers)?;
            codex(body.as_object_mut().expect("validated body"))?;
            headers
        }
        Dialect::Xai => {
            xai(body.as_object_mut().expect("validated body"));
            crate::providers::grok::api::outbound_headers(&input.headers)
        }
        Dialect::OpenAi => {
            openai(body.as_object_mut().expect("validated body"));
            HeaderMap::new()
        }
    };
    Ok((body, headers))
}

/// Codex 0.163 sends system-level input with the developer role, always streams,
/// never stores, and replays reasoning only through encrypted content.
fn codex(object: &mut Map<String, Value>) -> Result<()> {
    instruction_item(object);
    for name in CODEX_UNSUPPORTED {
        object.remove(name);
    }
    for item in object
        .get_mut("input")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if item["role"] == "system" {
            item["role"] = "developer".into();
        }
        if item["type"] == "reasoning" {
            // Stateless replay never looks up rs_* IDs.
            let item = item.as_object_mut().expect("validated input item");
            item.remove("id");
            item.remove("call_id");
        }
    }
    tool_choice(object);
    object.insert("stream".into(), true.into());
    object.insert("store".into(), false.into());
    object.entry("tool_choice").or_insert_with(|| "auto".into());
    object
        .entry("parallel_tool_calls")
        .or_insert_with(|| true.into());
    object.entry("reasoning").or_insert(Value::Null);
    include_encrypted(object)?;
    Ok(())
}

/// Grok Build sends system-level input with the system role, always streams,
/// disables storage and replays encrypted reasoning.
fn xai(object: &mut Map<String, Value>) {
    instruction_item(object);
    for name in [
        "client_metadata",
        "prompt_cache_retention",
        "safety_identifier",
    ] {
        object.remove(name);
    }
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
    object.insert("store".into(), false.into());
    let _ = include_encrypted(object);
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
}

/// The OpenAI API accepts the public fields; the request streams and stays
/// stateless unless the caller continues a stored response.
fn openai(object: &mut Map<String, Value>) {
    object.remove("client_metadata");
    object.insert("stream".into(), true.into());
    let continues = object
        .get("previous_response_id")
        .is_some_and(|v| !v.is_null());
    if !continues {
        object.insert("store".into(), false.into());
        let _ = include_encrypted(object);
    }
}

fn include_encrypted(object: &mut Map<String, Value>) -> Result<()> {
    let include = object
        .entry("include")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| ApiError::bad_request("include must be an array"))?;
    if !include.iter().any(|v| v == "reasoning.encrypted_content") {
        include.push("reasoning.encrypted_content".into());
    }
    Ok(())
}

/// Native subscription clients put instructions in their leading input item.
/// Public Responses suppliers retain the caller's separate instructions field.
fn instruction_item(object: &mut Map<String, Value>) {
    if let Some(Value::String(text)) = object.remove("instructions")
        && !text.is_empty()
    {
        object
            .get_mut("input")
            .and_then(Value::as_array_mut)
            .expect("validated input")
            .insert(
                0,
                json!({"type":"message","role":"system","content":[{"type":"input_text","text":text}]}),
            );
    }
}

/// A named choice must reference a declared tool; otherwise the model chooses.
fn tool_choice(object: &mut Map<String, Value>) {
    let Some(choice) = object.get("tool_choice").filter(|c| c.is_object()) else {
        return;
    };
    let kind = choice["type"].as_str().unwrap_or_default().to_owned();
    if kind == "allowed_tools" {
        return;
    }
    let name = choice["name"].as_str().map(str::to_owned);
    let mut declared = Vec::new();
    declared_tools(object.get("tools").unwrap_or(&Value::Null), &mut declared);
    for item in object["input"].as_array().into_iter().flatten() {
        if item["type"] == "additional_tools" {
            declared_tools(&item["tools"], &mut declared);
        }
    }
    let choice = if kind == "function" {
        match name.filter(|name| declared.iter().any(|(k, n)| k == "function" && n == name)) {
            Some(name) => json!({"type":"function","name":name}),
            None => "auto".into(),
        }
    } else if declared.iter().any(|(k, _)| *k == kind) {
        return;
    } else {
        "auto".into()
    };
    object.insert("tool_choice".into(), choice);
}

/// A Responses caller's request in the wire's item shapes. Input aliases were
/// normalized during validation; supplier identity is bound by the channel.
pub fn from_responses(body: &Value) -> Result<Value> {
    let mut body = body.clone();
    let object = body
        .as_object_mut()
        .ok_or_else(|| ApiError::bad_request("Request body must be an object"))?;
    normalize_tools(object)?;
    let input = object
        .get_mut("input")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| ApiError::bad_request("input must be an array"))?;
    for item in input.iter_mut() {
        match item["type"].as_str() {
            Some("reasoning") => {
                let item = item.as_object_mut().expect("validated input item");
                // `status` is output-only; reasoning_text parts need their discriminator.
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
            Some("function_call_output" | "custom_tool_call_output")
                if item["output"].is_array() =>
            {
                item["output"] = content_parts(std::mem::take(&mut item["output"]), false)?.into();
            }
            Some("additional_tools" | "tool_search_output") => {
                normalize_tools(item.as_object_mut().expect("validated input item"))?;
            }
            _ => {}
        }
    }
    Ok(body)
}

/// A Chat caller's request as Responses input items and controls.
pub fn from_chat(body: &Value) -> Result<Value> {
    let messages = body["messages"]
        .as_array()
        .ok_or_else(|| ApiError::bad_request("messages must be a nonempty array"))?;
    let mut input = Vec::new();
    let mut custom_calls = BTreeSet::new();
    for message in messages {
        let role = required(message, "role")?;
        if role == "tool" {
            let call_id = required(message, "tool_call_id")?;
            let output = match &message["content"] {
                Value::String(_) => message["content"].clone(),
                Value::Array(_) => content_parts(message["content"].clone(), false)?.into(),
                _ => {
                    return Err(ApiError::bad_request(
                        "Tool output must be text or content parts",
                    ));
                }
            };
            let kind = if custom_calls.contains(call_id) {
                "custom_tool_call_output"
            } else {
                "function_call_output"
            };
            input.push(json!({"type":kind,"call_id":call_id,"output":output}));
            continue;
        }
        if !matches!(role, "system" | "developer" | "user" | "assistant") {
            return Err(ApiError::bad_request("Unsupported message role"));
        }
        if role == "assistant" {
            reasoning(message, &mut input)?;
        }
        let mut content = match &message["content"] {
            Value::Null if role == "assistant" => Vec::new(),
            value => content_parts(value.clone(), role == "assistant")?,
        };
        if let Some(refusal) = message.get("refusal").filter(|v| !v.is_null()) {
            if role != "assistant" || !refusal.is_string() {
                return Err(ApiError::bad_request("refusal must be assistant text"));
            }
            if !content.iter().any(|part| part["type"] == "refusal") {
                content.push(json!({"type":"refusal","refusal":refusal}));
            }
        }
        if !content.is_empty() {
            let mut item = json!({"type":"message","role":role,"content":content});
            if let Some(phase) = message.get("phase").filter(|_| role == "assistant") {
                item["phase"] = phase.clone();
            }
            input.push(item);
        }
        let Some(calls) = message.get("tool_calls").filter(|v| !v.is_null()) else {
            continue;
        };
        if role != "assistant" {
            return Err(ApiError::bad_request(
                "tool_calls require an assistant message",
            ));
        }
        for call in calls
            .as_array()
            .ok_or_else(|| ApiError::bad_request("tool_calls must be an array"))?
        {
            let call_id = required(call, "id")?;
            let (kind, payload, field) = match call["type"].as_str() {
                Some("function") => ("function_call", &call["function"], "arguments"),
                Some("custom") => ("custom_tool_call", &call["custom"], "input"),
                _ => {
                    return Err(ApiError::bad_request(
                        "Chat tool calls must be function or custom",
                    ));
                }
            };
            let name = required(payload, "name")?;
            let value = payload[field]
                .as_str()
                .ok_or_else(|| ApiError::bad_request(format!("Tool {field} must be a string")))?;
            let mut item = json!({"type":kind,"call_id":call_id,"name":name,field:value});
            for key in ["namespace", "encrypted_function_args"] {
                if let Some(value) = payload
                    .get(key)
                    .or_else(|| call.get(key))
                    .filter(|v| !v.is_null())
                {
                    item[key] = value.clone();
                }
            }
            if kind == "custom_tool_call" {
                custom_calls.insert(call_id);
            }
            input.push(item);
        }
    }
    let mut out = Map::new();
    out.insert("model".into(), body["model"].clone());
    out.insert("input".into(), input.into());
    for key in [
        "tools",
        "tool_choice",
        "parallel_tool_calls",
        "temperature",
        "top_p",
        "top_logprobs",
        "service_tier",
        "prompt_cache_key",
        "prompt_cache_retention",
        "client_metadata",
        "reasoning",
    ] {
        if let Some(value) = body.get(key) {
            out.insert(key.into(), value.clone());
        }
    }
    if let Some(limit) = body
        .get("max_completion_tokens")
        .or_else(|| body.get("max_tokens"))
    {
        out.insert("max_output_tokens".into(), limit.clone());
    }
    if let Some(effort) = body.get("reasoning_effort") {
        out.entry("reasoning")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| ApiError::bad_request("reasoning must be an object"))?
            .entry("effort")
            .or_insert(effort.clone());
    }
    let mut text = Map::new();
    if let Some(format) = body.get("response_format") {
        text.insert(
            "format".into(),
            match format["type"].as_str() {
                Some("text" | "json_object") => format.clone(),
                Some("json_schema") => {
                    let mut schema =
                        format["json_schema"].as_object().cloned().ok_or_else(|| {
                            ApiError::bad_request("response_format.json_schema must be an object")
                        })?;
                    schema.insert("type".into(), "json_schema".into());
                    schema.into()
                }
                _ => return Err(ApiError::bad_request("Invalid response_format type")),
            },
        );
    }
    if let Some(verbosity) = body.get("verbosity") {
        text.insert("verbosity".into(), verbosity.clone());
    }
    if !text.is_empty() {
        out.insert("text".into(), text.into());
    }
    if let Some(search) = body.get("web_search_options") {
        let mut search = search
            .as_object()
            .cloned()
            .ok_or_else(|| ApiError::bad_request("web_search_options must be an object"))?;
        search.insert("type".into(), "web_search".into());
        out.entry("tools")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| ApiError::bad_request("tools must be an array"))?
            .push(search.into());
    }
    if body["logprobs"] == true {
        out.insert("include".into(), json!(["message.output_text.logprobs"]));
    }
    normalize_tools(&mut out)?;
    Ok(out.into())
}

/// The lossless `reasoning` extension wins; `reasoning_content` is the common text view.
fn reasoning(message: &Value, input: &mut Vec<Value>) -> Result<()> {
    if let Some(reasoning) = message.get("reasoning").filter(|v| !v.is_null()) {
        let items = match reasoning {
            Value::Array(items) => items.as_slice(),
            Value::Object(_) => std::slice::from_ref(reasoning),
            _ => {
                return Err(ApiError::bad_request(
                    "Message reasoning must be an object or array",
                ));
            }
        };
        for item in items {
            if !item.is_object() {
                return Err(ApiError::bad_request("reasoning items must be objects"));
            }
            let mut item = item.clone();
            let fields = item.as_object_mut().expect("reasoning object");
            fields.insert("type".into(), "reasoning".into());
            fields.remove("status");
            fields.entry("summary").or_insert_with(|| json!([]));
            input.push(item);
        }
        return Ok(());
    }
    let text = message.get("reasoning_content").filter(|v| !v.is_null());
    let encrypted = message.get("encrypted_content").filter(|v| !v.is_null());
    if text.is_none() && encrypted.is_none() {
        return Ok(());
    }
    let mut item = json!({"type":"reasoning","summary":[]});
    if let Some(text) = text {
        if !text.is_string() {
            return Err(ApiError::bad_request("reasoning_content must be a string"));
        }
        item["summary"] = json!([{"type":"summary_text","text":text}]);
    }
    if let Some(encrypted) = encrypted {
        item["encrypted_content"] = encrypted.clone();
    }
    input.push(item);
    Ok(())
}

fn part(block: &Value) -> Result<Value> {
    let source = &block["source"];
    match block["type"].as_str() {
        Some("text") => Ok(json!({"type":"input_text","text":text(block, "text")?})),
        Some("image") => Ok(json!({"type":"input_image","image_url":image_url(source)?})),
        Some("document") => match source["type"].as_str() {
            Some("text") => Ok(json!({"type":"input_text","text":text(source, "data")?})),
            Some("base64") => Ok(json!({"type":"input_file",
                "filename":block["title"].as_str().unwrap_or("document.pdf"),
                "file_data":format!("data:{};base64,{}", text(source, "media_type")?, text(source, "data")?)})),
            Some("url") => Ok(json!({"type":"input_file","file_url":text(source, "url")?})),
            _ => Err(ApiError::bad_request(
                "Unsupported Messages document source",
            )),
        },
        _ => Err(ApiError::bad_request(format!(
            "Messages content block cannot be converted: {}",
            block["type"]
        ))),
    }
}

fn tool_output(block: &Value) -> Result<Value> {
    let content = match block.get("content") {
        None | Some(Value::Null) => return Ok("(empty)".into()),
        Some(content) => content,
    };
    if let Some(text) = content.as_str() {
        return Ok(if text.is_empty() { "(empty)" } else { text }.into());
    }
    let parts = blocks(content)?
        .iter()
        .filter(|b| b["type"] != "text" || b["text"].as_str().is_some_and(|t| !t.is_empty()))
        .map(part)
        .collect::<Result<Vec<_>>>()?;
    Ok(match parts.as_slice() {
        [] => "(empty)".into(),
        _ if parts.iter().all(|p| p["type"] == "input_text") => parts
            .iter()
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
            .into(),
        _ => parts.into(),
    })
}

/// A Messages caller's request as Responses input items and controls.
pub fn from_messages(body: &Value) -> Result<Value> {
    for field in ["container", "context_management", "mcp_servers"] {
        if body.get(field).is_some() {
            return Err(ApiError::bad_request(format!(
                "Messages {field} cannot be represented by the target protocol"
            )));
        }
    }
    let mut input = Vec::new();
    let system = system_texts(body)?;
    if !system.is_empty() {
        let content = system
            .iter()
            .map(|text| json!({"type":"input_text","text":text}))
            .collect::<Vec<_>>();
        input.push(json!({"type":"message","role":"system","content":content}));
    }
    for message in body["messages"].as_array().into_iter().flatten() {
        let assistant = message["role"] == "assistant";
        let mut content = Vec::new();
        let flush = |content: &mut Vec<Value>, input: &mut Vec<Value>| {
            if !content.is_empty() {
                input.push(json!({"type":"message","role":message["role"],"content":std::mem::take(content)}));
            }
        };
        for block in blocks(&message["content"])?.iter() {
            match block["type"].as_str() {
                Some("text") => {
                    let text = text(block, "text")?;
                    if !text.is_empty() {
                        let kind = if assistant {
                            "output_text"
                        } else {
                            "input_text"
                        };
                        content.push(json!({"type":kind,"text":text}));
                    }
                }
                Some("tool_use") => {
                    flush(&mut content, &mut input);
                    input.push(json!({"type":"function_call","call_id":block["id"],"name":block["name"],"arguments":block["input"].to_string()}));
                }
                Some("tool_result") => {
                    flush(&mut content, &mut input);
                    input.push(json!({"type":"function_call_output","call_id":block["tool_use_id"],"output":tool_output(block)?}));
                }
                // The signature is the supplier's encrypted reasoning; unsigned text cannot be replayed.
                Some("thinking") => {
                    let Some(signature) = block["signature"].as_str().filter(|s| !s.is_empty())
                    else {
                        continue;
                    };
                    flush(&mut content, &mut input);
                    let mut item =
                        json!({"type":"reasoning","summary":[],"encrypted_content":signature});
                    if let Some(text) = block["thinking"].as_str().filter(|t| !t.is_empty()) {
                        item["summary"] = json!([{"type":"summary_text","text":text}]);
                    }
                    input.push(item);
                }
                Some("redacted_thinking" | "server_tool_use") => {}
                Some(kind) if kind.ends_with("_tool_result") => {}
                _ if !assistant => content.push(part(block)?),
                _ => {
                    return Err(ApiError::bad_request(format!(
                        "Messages content block cannot be converted: {}",
                        block["type"]
                    )));
                }
            }
        }
        flush(&mut content, &mut input);
    }
    let mut out = Map::new();
    out.insert("model".into(), body["model"].clone());
    out.insert("input".into(), input.into());
    out.insert("max_output_tokens".into(), body["max_tokens"].clone());
    for key in ["temperature", "top_p", "client_metadata"] {
        if let Some(value) = body.get(key) {
            out.insert(key.into(), value.clone());
        }
    }
    if let Some(tools) = body["tools"].as_array() {
        let tools = tools
            .iter()
            .map(|tool| {
                if tool["type"].as_str().is_some_and(|t| t.starts_with("web_search")) {
                    return json!({"type":"web_search"});
                }
                let mut function = json!({"type":"function","name":tool["name"],"parameters":schema(&tool["input_schema"]),"strict":false});
                if let Some(description) = tool.get("description") {
                    function["description"] = description.clone();
                }
                function
            })
            .collect::<Vec<_>>();
        out.insert("tools".into(), tools.into());
    }
    if let Some(choice) = body.get("tool_choice") {
        let choice = match choice["type"].as_str() {
            Some("auto") => json!("auto"),
            Some("any") => json!("required"),
            Some("none") => json!("none"),
            Some("tool") => json!({"type":"function","name":choice["name"]}),
            _ => choice.clone(),
        };
        out.insert("tool_choice".into(), choice);
    }
    let disable_parallel = match body.pointer("/tool_choice/disable_parallel_tool_use") {
        None => false,
        Some(Value::Bool(disabled)) => *disabled,
        Some(_) => {
            return Err(ApiError::bad_request(
                "disable_parallel_tool_use must be a boolean",
            ));
        }
    };
    out.insert("parallel_tool_calls".into(), (!disable_parallel).into());
    let effort = if body.pointer("/thinking/type") == Some(&json!("disabled")) {
        json!({"effort":"none"})
    } else {
        let effort = body
            .pointer("/output_config/effort")
            .cloned()
            .unwrap_or_else(|| "medium".into());
        json!({"effort":effort,"summary":"auto"})
    };
    out.insert("reasoning".into(), effort);
    if let Some(format) = body.pointer("/output_config/format") {
        if format["type"] != "json_schema" {
            return Err(ApiError::bad_request("Unsupported output format"));
        }
        out.insert(
            "text".into(),
            json!({"format":{"type":"json_schema","name":"output","schema":format["schema"],"strict":true}}),
        );
    }
    Ok(out.into())
}
