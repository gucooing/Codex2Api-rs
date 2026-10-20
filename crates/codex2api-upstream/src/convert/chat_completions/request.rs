//! Chat Completions requests built from a Responses, Chat or Messages request as the
//! caller sent it: messages with tool calls and tool results, function tools,
//! `max_completion_tokens`, `reasoning_effort` and `response_format`. The request
//! always streams with usage so the ledger reads every generation's counts.
use crate::inbound::messages::{blocks, image_url, schema, system_texts, text as block_text};
use crate::inbound::request::normalize_tools;
use crate::inbound::{Error as ApiError, Inbound, InboundRequest, Result};
use serde_json::{Map, Value, json};

/// Which front-end builds the request.
pub fn build(input: &InboundRequest) -> Result<Value> {
    let mut body = match input.format {
        Inbound::Responses => from_responses(&input.body)?,
        Inbound::Chat => from_chat(&input.body)?,
        Inbound::Messages => from_messages(&input.body)?,
        Inbound::CodexClient | Inbound::GrokClient => {
            return Err(ApiError::bad_request(
                "A native client request has no conversion to the Chat Completions wire",
            ));
        }
    };
    let object = body.as_object_mut().expect("built object");
    for key in ["store", "n", "background", "stream_options"] {
        object.remove(key);
    }
    object.insert("stream".into(), true.into());
    object.insert("stream_options".into(), json!({"include_usage":true}));
    Ok(body)
}

/// Chat messages in creation order; tool calls join the assistant turn before them.
#[derive(Default)]
struct Messages {
    items: Vec<Value>,
    reasoning: Option<String>,
}
impl Messages {
    fn system(&mut self, text: &str) {
        if !text.trim().is_empty() {
            self.items.push(json!({"role":"system","content":text}));
        }
    }
    fn user(&mut self, parts: Vec<Value>) {
        if !parts.is_empty() {
            self.items.push(json!({"role":"user","content":parts}));
        }
    }
    fn reasoning(&mut self, text: &str) {
        if !text.is_empty() {
            match &mut self.reasoning {
                Some(buffer) => {
                    buffer.push('\n');
                    buffer.push_str(text);
                }
                None => self.reasoning = Some(text.to_owned()),
            }
        }
    }
    fn assistant(&mut self) -> &mut Map<String, Value> {
        let open = self
            .items
            .last()
            .is_some_and(|item| item["role"] == "assistant");
        if !open {
            let mut message = json!({"role":"assistant","content":null});
            if let Some(text) = self.reasoning.take() {
                message["reasoning_content"] = text.into();
            }
            self.items.push(message);
        }
        self.items
            .last_mut()
            .and_then(Value::as_object_mut)
            .expect("assistant message")
    }
    fn assistant_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let message = self.assistant();
        match message.get_mut("content") {
            Some(Value::String(existing)) => existing.push_str(text),
            _ => {
                message.insert("content".into(), text.into());
            }
        }
    }
    fn tool_call(&mut self, call: Value) {
        let message = self.assistant();
        match message.get_mut("tool_calls") {
            Some(Value::Array(calls)) => calls.push(call),
            _ => {
                message.insert("tool_calls".into(), json!([call]));
            }
        }
    }
    fn tool_result(&mut self, call_id: &str, content: Value) {
        self.items
            .push(json!({"role":"tool","tool_call_id":call_id,"content":content}));
    }
    fn finish(self) -> Result<Vec<Value>> {
        if self.items.is_empty() {
            return Err(ApiError::bad_request("input must contain a message"));
        }
        Ok(self.items)
    }
}

fn function_call(call_id: &str, name: &str, arguments: &str) -> Value {
    json!({"id":call_id,"type":"function","function":{"name":name,"arguments":arguments}})
}

/// A Responses user content part as a Chat content part.
fn user_part(part: &Value) -> Result<Value> {
    match part["type"].as_str() {
        Some("input_text") => Ok(json!({"type":"text","text":part["text"]})),
        Some("input_image") => {
            let mut image = json!({"url":part["image_url"]});
            if let Some(detail) = part.get("detail").filter(|v| !v.is_null()) {
                image["detail"] = detail.clone();
            }
            Ok(json!({"type":"image_url","image_url":image}))
        }
        Some("input_file") => {
            let mut file = Map::new();
            for key in ["file_data", "file_id", "file_url", "filename"] {
                if let Some(value) = part.get(key).filter(|v| !v.is_null()) {
                    file.insert(key.into(), value.clone());
                }
            }
            if file.is_empty() {
                return Err(ApiError::bad_request("input_file needs file data"));
            }
            Ok(json!({"type":"file","file":file}))
        }
        Some(kind) => Err(ApiError::bad_request(format!(
            "The Chat Completions wire cannot accept {kind} content"
        ))),
        None => Err(ApiError::bad_request("Content part type is required")),
    }
}

fn output_text(output: &Value) -> Value {
    match output {
        Value::String(text) => text.clone().into(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n")
            .into(),
        Value::Null => "".into(),
        other => other.to_string().into(),
    }
}

/// Responses-shaped tools (also the normalized Chat tools) as Chat tools.
fn tools(declared: &Value, out: &mut Vec<Value>, search: &mut bool) -> Result<()> {
    for tool in declared.as_array().into_iter().flatten() {
        match tool["type"].as_str() {
            Some("function") => {
                let mut function = json!({"name":tool["name"]});
                for key in ["description", "parameters", "strict"] {
                    if let Some(value) = tool.get(key).filter(|v| !v.is_null()) {
                        function[key] = value.clone();
                    }
                }
                out.push(json!({"type":"function","function":function}));
            }
            Some("custom") => {
                let mut custom = json!({"name":tool["name"]});
                for key in ["description", "format"] {
                    if let Some(value) = tool.get(key).filter(|v| !v.is_null()) {
                        custom[key] = value.clone();
                    }
                }
                out.push(json!({"type":"custom","custom":custom}));
            }
            Some("web_search" | "web_search_preview" | "web_search_preview_2025_03_11") => {
                *search = true;
            }
            Some("namespace") => tools(&tool["tools"], out, search)?,
            Some(kind) => {
                return Err(ApiError::bad_request(format!(
                    "The Chat Completions wire cannot use {kind} tools"
                )));
            }
            None => return Err(ApiError::bad_request("Tool type is required")),
        }
    }
    Ok(())
}

/// A normalized Responses tool choice as a Chat tool choice.
fn tool_choice(choice: &Value) -> Value {
    match choice {
        Value::String(_) => choice.clone(),
        Value::Object(object) => match object.get("type").and_then(Value::as_str) {
            Some("function") => json!({"type":"function","function":{"name":object["name"]}}),
            Some("custom") => json!({"type":"custom","custom":{"name":object["name"]}}),
            Some("allowed_tools") if object.get("mode") == Some(&json!("required")) => {
                "required".into()
            }
            _ => "auto".into(),
        },
        _ => "auto".into(),
    }
}

fn response_format(format: &Value) -> Result<Option<Value>> {
    Ok(match format["type"].as_str() {
        None => None,
        Some("text") => Some(json!({"type":"text"})),
        Some("json_object") => Some(json!({"type":"json_object"})),
        Some("json_schema") => {
            let mut schema = json!({"name":format["name"].as_str().unwrap_or("response"),"schema":format["schema"]});
            for key in ["description", "strict"] {
                if let Some(value) = format.get(key).filter(|v| !v.is_null()) {
                    schema[key] = value.clone();
                }
            }
            Some(json!({"type":"json_schema","json_schema":schema}))
        }
        Some(_) => return Err(ApiError::bad_request("Invalid text.format type")),
    })
}

/// Chat and Responses reasoning efforts share one vocabulary.
fn effort(value: Option<&Value>) -> Result<Option<Value>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(effort))
            if matches!(
                effort.as_str(),
                "none" | "minimal" | "low" | "medium" | "high" | "xhigh"
            ) =>
        {
            Ok(Some(effort.as_str().into()))
        }
        _ => Err(ApiError::bad_request("Invalid reasoning effort")),
    }
}

fn copy(from: &Value, to: &mut Map<String, Value>, keys: &[&str]) {
    for key in keys {
        if let Some(value) = from.get(*key).filter(|v| !v.is_null()) {
            to.insert((*key).into(), value.clone());
        }
    }
}

/// A Responses request: input items into messages, Responses tools and controls.
pub fn from_responses(body: &Value) -> Result<Value> {
    if body
        .get("previous_response_id")
        .is_some_and(|v| !v.is_null())
    {
        return Err(ApiError::bad_request(
            "The Chat Completions wire requires the full conversation input; previous_response_id is not supported",
        ));
    }
    let mut body = body.clone();
    normalize_tools(
        body.as_object_mut()
            .ok_or_else(|| ApiError::bad_request("Request body must be an object"))?,
    )?;
    let mut messages = Messages::default();
    if let Some(instructions) = body["instructions"].as_str() {
        messages.system(instructions);
    }
    let mut input = body["input"].clone();
    if input.is_string() {
        let text = std::mem::take(&mut input);
        input =
            json!([{"type":"message","role":"user","content":[{"type":"input_text","text":text}]}]);
    }
    for item in input.as_array().into_iter().flatten() {
        match item["type"].as_str().unwrap_or("message") {
            "message" => {
                let content = match &item["content"] {
                    Value::String(text) => vec![
                        json!({"type":if item["role"]=="assistant" {"output_text"} else {"input_text"},"text":text}),
                    ],
                    Value::Array(parts) => parts.clone(),
                    _ => Vec::new(),
                };
                match item["role"].as_str() {
                    Some("system" | "developer") => {
                        let text = content
                            .iter()
                            .filter_map(|part| part["text"].as_str())
                            .collect::<Vec<_>>()
                            .join("\n");
                        messages.system(&text);
                    }
                    Some("assistant") => {
                        for part in &content {
                            if let Some(text) =
                                part["text"].as_str().or_else(|| part["refusal"].as_str())
                            {
                                messages.assistant_text(text);
                            }
                        }
                    }
                    _ => {
                        let parts = content.iter().map(user_part).collect::<Result<Vec<_>>>()?;
                        messages.user(parts);
                    }
                }
            }
            "reasoning" => {
                messages.reasoning(&crate::convert::responses::output::reasoning_text(item));
            }
            "function_call" => messages.tool_call(function_call(
                item["call_id"].as_str().unwrap_or_default(),
                item["name"].as_str().unwrap_or_default(),
                item["arguments"].as_str().unwrap_or("{}"),
            )),
            "custom_tool_call" => {
                messages.tool_call(json!({"id":item["call_id"],"type":"custom",
                "custom":{"name":item["name"],"input":item["input"].as_str().unwrap_or_default()}}))
            }
            "function_call_output" | "custom_tool_call_output" => messages.tool_result(
                item["call_id"].as_str().unwrap_or_default(),
                output_text(&item["output"]),
            ),
            kind => {
                return Err(ApiError::bad_request(format!(
                    "The Chat Completions wire cannot accept {kind} input items"
                )));
            }
        }
    }
    let mut out = Map::new();
    out.insert("model".into(), body["model"].clone());
    out.insert("messages".into(), messages.finish()?.into());
    let mut declared = Vec::new();
    let mut search = false;
    tools(&body["tools"], &mut declared, &mut search)?;
    if !declared.is_empty() {
        out.insert("tools".into(), declared.into());
        if let Some(choice) = body.get("tool_choice") {
            out.insert("tool_choice".into(), tool_choice(choice));
        }
        copy(&body, &mut out, &["parallel_tool_calls"]);
    }
    if search {
        out.insert("web_search_options".into(), json!({}));
    }
    copy(
        &body,
        &mut out,
        &[
            "temperature",
            "top_p",
            "top_logprobs",
            "service_tier",
            "prompt_cache_key",
        ],
    );
    if out.contains_key("top_logprobs") {
        out.insert("logprobs".into(), true.into());
    }
    if let Some(limit) = body.get("max_output_tokens").filter(|v| !v.is_null()) {
        out.insert("max_completion_tokens".into(), limit.clone());
    }
    if let Some(effort) = effort(body.pointer("/reasoning/effort"))? {
        out.insert("reasoning_effort".into(), effort);
    }
    if let Some(format) = response_format(&body["text"]["format"])? {
        out.insert("response_format".into(), format);
    }
    copy(&body["text"], &mut out, &["verbosity"]);
    Ok(out.into())
}

/// A Chat request: the caller's own shape without local extensions.
pub fn from_chat(body: &Value) -> Result<Value> {
    let mut body = body.clone();
    let object = body
        .as_object_mut()
        .ok_or_else(|| ApiError::bad_request("Request body must be an object"))?;
    for key in [
        "client_metadata",
        "prompt_cache_retention",
        "safety_identifier",
        "settings",
        "commands",
        "search_parameters",
        "generate",
    ] {
        object.remove(key);
    }
    if let Some(choice) = object.get("tool_choice")
        && choice
            .as_str()
            .is_some_and(|v| !["auto", "none", "required"].contains(&v))
    {
        return Err(ApiError::bad_request("Invalid tool_choice"));
    }
    for message in object
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let Some(fields) = message.as_object_mut() else {
            return Err(ApiError::bad_request("messages must contain objects"));
        };
        if fields.get("role") != Some(&json!("assistant")) {
            continue;
        }
        // The lossless reasoning extension becomes the common text view.
        if let Some(reasoning) = fields.remove("reasoning") {
            let items = match reasoning {
                Value::Array(items) => items,
                other => vec![other],
            };
            let text = items
                .iter()
                .map(crate::convert::responses::output::reasoning_text)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if !text.is_empty() && !fields.contains_key("reasoning_content") {
                fields.insert("reasoning_content".into(), text.into());
            }
        }
        for key in ["encrypted_content", "phase"] {
            fields.remove(key);
        }
    }
    Ok(body)
}

fn messages_tool_output(block: &Value) -> Result<Value> {
    let content = match block.get("content") {
        None | Some(Value::Null) => return Ok("(empty)".into()),
        Some(content) => content,
    };
    if let Some(text) = content.as_str() {
        return Ok(if text.is_empty() { "(empty)" } else { text }.into());
    }
    let owned = blocks(content)?;
    let texts = owned
        .iter()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>();
    Ok(if texts.is_empty() {
        "(empty)".into()
    } else {
        texts.join("\n\n").into()
    })
}

/// A Messages thinking budget as a reasoning effort.
fn budget_effort(budget: Option<u64>) -> &'static str {
    match budget {
        Some(n) if n < 4_096 => "low",
        Some(n) if n < 16_384 => "medium",
        _ => "high",
    }
}

/// A Messages request: content blocks into messages, Messages tools and controls.
pub fn from_messages(body: &Value) -> Result<Value> {
    for field in ["container", "context_management", "mcp_servers"] {
        if body.get(field).is_some() {
            return Err(ApiError::bad_request(format!(
                "Messages {field} cannot be represented by the target protocol"
            )));
        }
    }
    let mut messages = Messages::default();
    let system = system_texts(body)?;
    if !system.is_empty() {
        messages.system(&system.join("\n\n"));
    }
    for message in body["messages"].as_array().into_iter().flatten() {
        let assistant = message["role"] == "assistant";
        let mut parts = Vec::new();
        for block in blocks(&message["content"])?.iter() {
            match block["type"].as_str() {
                Some("text") => {
                    let text = block_text(block, "text")?;
                    if assistant {
                        messages.assistant_text(text);
                    } else {
                        parts.push(json!({"type":"text","text":text}));
                    }
                }
                Some("image") if !assistant => {
                    parts.push(json!({"type":"image_url","image_url":{"url":image_url(&block["source"])?}}));
                }
                Some("document") if !assistant => {
                    let source = &block["source"];
                    match source["type"].as_str() {
                        Some("text") => parts.push(json!({"type":"text","text":block_text(source, "data")?})),
                        Some("base64") => parts.push(json!({"type":"file","file":{
                            "filename":block["title"].as_str().unwrap_or("document.pdf"),
                            "file_data":format!("data:{};base64,{}", block_text(source, "media_type")?, block_text(source, "data")?)}})),
                        _ => {
                            return Err(ApiError::bad_request(
                                "Unsupported Messages document source",
                            ));
                        }
                    }
                }
                Some("tool_use") if assistant => messages.tool_call(function_call(
                    block["id"].as_str().unwrap_or_default(),
                    block["name"].as_str().unwrap_or_default(),
                    &block["input"].to_string(),
                )),
                Some("tool_result") if !assistant => {
                    if !parts.is_empty() {
                        messages.user(std::mem::take(&mut parts));
                    }
                    messages.tool_result(
                        block["tool_use_id"].as_str().unwrap_or_default(),
                        messages_tool_output(block)?,
                    );
                }
                Some("thinking") if assistant => {
                    messages.reasoning(block["thinking"].as_str().unwrap_or_default());
                }
                Some("redacted_thinking" | "server_tool_use") => {}
                Some(kind) if kind.ends_with("_tool_result") => {}
                Some(kind) => {
                    return Err(ApiError::bad_request(format!(
                        "Messages content block cannot be converted: {kind}"
                    )));
                }
                None => return Err(ApiError::bad_request("Content blocks require a type")),
            }
        }
        if !parts.is_empty() {
            messages.user(parts);
        }
    }
    let mut out = Map::new();
    out.insert("model".into(), body["model"].clone());
    out.insert("messages".into(), messages.finish()?.into());
    let mut declared = Vec::new();
    let mut search = false;
    for tool in body["tools"].as_array().into_iter().flatten() {
        if tool["type"]
            .as_str()
            .is_some_and(|t| t.starts_with("web_search"))
        {
            search = true;
            continue;
        }
        let mut function = json!({"name":tool["name"],"parameters":schema(&tool["input_schema"])});
        if let Some(description) = tool.get("description").filter(|v| !v.is_null()) {
            function["description"] = description.clone();
        }
        declared.push(json!({"type":"function","function":function}));
    }
    if !declared.is_empty() {
        out.insert("tools".into(), declared.into());
        if let Some(choice) = body.get("tool_choice") {
            let mapped = match choice["type"].as_str() {
                Some("any") => json!("required"),
                Some("none") => json!("none"),
                Some("tool") => json!({"type":"function","function":{"name":choice["name"]}}),
                _ => json!("auto"),
            };
            out.insert("tool_choice".into(), mapped);
            if choice["disable_parallel_tool_use"] == true {
                out.insert("parallel_tool_calls".into(), false.into());
            }
        }
    }
    if search {
        out.insert("web_search_options".into(), json!({}));
    }
    out.insert("max_completion_tokens".into(), body["max_tokens"].clone());
    copy(body, &mut out, &["temperature", "top_p"]);
    if let Some(stops) = body.get("stop_sequences").filter(|v| !v.is_null()) {
        out.insert("stop".into(), stops.clone());
    }
    if let Some(effort) = body
        .pointer("/output_config/effort")
        .and_then(Value::as_str)
    {
        out.insert("reasoning_effort".into(), effort.into());
    } else if body.pointer("/thinking/type") == Some(&json!("enabled")) {
        out.insert(
            "reasoning_effort".into(),
            budget_effort(
                body.pointer("/thinking/budget_tokens")
                    .and_then(Value::as_u64),
            )
            .into(),
        );
    }
    Ok(out.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_items_become_chat_messages_with_tool_calls_and_results() {
        let body = json!({"model":"gpt-5","instructions":"Be brief.","max_output_tokens":500,
        "reasoning":{"effort":"low"},"text":{"format":{"type":"json_schema","name":"answer","schema":{"type":"object"}}},
        "tools":[{"type":"function","name":"lookup","parameters":{"type":"object","properties":{}}},{"type":"web_search"}],
        "tool_choice":{"type":"function","name":"lookup"},
        "input":[
            {"type":"message","role":"user","content":[{"type":"input_text","text":"hi"},{"type":"input_image","image_url":"data:image/png;base64,AA=="}]},
            {"type":"reasoning","summary":[{"type":"summary_text","text":"think"}]},
            {"type":"function_call","call_id":"call_1","name":"lookup","arguments":"{\"q\":1}"},
            {"type":"function_call_output","call_id":"call_1","output":"found"},
            {"type":"message","role":"assistant","content":[{"type":"output_text","text":"done"}]}
        ]});
        let out = from_responses(&body).unwrap();
        let messages = out["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["content"][1]["type"], "image_url");
        assert_eq!(messages[2]["reasoning_content"], "think");
        assert_eq!(messages[2]["tool_calls"][0]["function"]["name"], "lookup");
        assert_eq!(messages[3]["role"], "tool");
        assert_eq!(messages[4]["content"], "done");
        assert_eq!(out["max_completion_tokens"], 500);
        assert_eq!(out["reasoning_effort"], "low");
        assert_eq!(out["response_format"]["json_schema"]["name"], "answer");
        assert_eq!(out["tool_choice"]["function"]["name"], "lookup");
        assert_eq!(out["tools"].as_array().unwrap().len(), 1);
        assert!(out["web_search_options"].is_object());
        assert!(
            from_responses(&json!({"model":"m","previous_response_id":"r","input":[]})).is_err()
        );
    }

    #[test]
    fn messages_blocks_become_chat_messages_and_the_wire_always_streams_with_usage() {
        let body = json!({"model":"claude","max_tokens":100,"system":"sys","stop_sequences":["x"],
        "thinking":{"type":"enabled","budget_tokens":10000},
        "tools":[{"name":"get","input_schema":{"type":"object"}}],"tool_choice":{"type":"any","disable_parallel_tool_use":true},
        "messages":[
            {"role":"user","content":[{"type":"text","text":"q"}]},
            {"role":"assistant","content":[{"type":"thinking","thinking":"t","signature":"s"},{"type":"tool_use","id":"tu_1","name":"get","input":{"a":1}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"tu_1","content":"r"},{"type":"text","text":"next"}]}
        ]});
        let out = from_messages(&body).unwrap();
        let messages = out["messages"].as_array().unwrap();
        assert_eq!(messages[0]["content"], "sys");
        assert_eq!(messages[2]["reasoning_content"], "t");
        assert_eq!(
            messages[2]["tool_calls"][0]["function"]["arguments"],
            "{\"a\":1}"
        );
        assert_eq!(messages[3]["role"], "tool");
        assert_eq!(messages[4]["content"][0]["text"], "next");
        assert_eq!(out["reasoning_effort"], "medium");
        assert_eq!(out["tool_choice"], "required");
        assert_eq!(out["parallel_tool_calls"], false);
        assert_eq!(out["stop"], json!(["x"]));
        let input = InboundRequest::new(
            crate::inbound::Protocol::ChatCompletions,
            json!({"model":"m","messages":[{"role":"assistant","content":"a","reasoning":[{"type":"reasoning","summary":[{"type":"summary_text","text":"why"}]}]},{"role":"user","content":"hi"}]}),
            http::HeaderMap::new(),
        )
        .unwrap();
        let built = build(&input).unwrap();
        assert_eq!(built["stream"], true);
        assert_eq!(built["stream_options"]["include_usage"], true);
        assert_eq!(built["messages"][0]["reasoning_content"], "why");
        assert!(built["messages"][0].get("reasoning").is_none());
    }
}
