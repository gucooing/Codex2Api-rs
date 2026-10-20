//! Messages requests built from a Responses, Chat or Messages request as the caller
//! sent it: alternating user and assistant turns of content blocks, tools with input
//! schemas, `max_tokens`, thinking budgets and stop sequences.
use crate::inbound::messages::{schema, system_texts};
use crate::inbound::request::{content_parts, normalize_tools};
use crate::inbound::{Error as ApiError, Inbound, InboundRequest, Result};
use serde_json::{Map, Value, json};

/// Anthropic requires `max_tokens`; callers of other protocols may leave it unset.
const DEFAULT_MAX_TOKENS: u64 = 8_192;

/// Which front-end builds the request.
pub fn build(input: &InboundRequest) -> Result<Value> {
    let mut body = match input.format {
        Inbound::Responses => from_responses(&input.body)?,
        Inbound::Chat => from_chat(&input.body)?,
        Inbound::Messages => from_messages(&input.body)?,
        Inbound::CodexClient | Inbound::GrokClient => {
            return Err(ApiError::bad_request(
                "A native client request has no conversion to the Messages wire",
            ));
        }
    };
    body["stream"] = true.into();
    Ok(body)
}

/// Messages turns; consecutive blocks of one role join one message.
#[derive(Default)]
struct Turns {
    messages: Vec<Value>,
}
impl Turns {
    fn push(&mut self, role: &str, block: Value) {
        match self.messages.last_mut() {
            Some(last) if last["role"] == role => {
                last["content"].as_array_mut().expect("blocks").push(block)
            }
            _ => self.messages.push(json!({"role":role,"content":[block]})),
        }
    }
    fn user(&mut self, block: Value) {
        self.push("user", block);
    }
    fn assistant(&mut self, block: Value) {
        self.push("assistant", block);
    }
    fn assistant_text(&mut self, text: &str) {
        if !text.is_empty() {
            self.assistant(json!({"type":"text","text":text}));
        }
    }
    fn tool_use(&mut self, id: &str, name: &str, input: Value) {
        self.assistant(json!({"type":"tool_use","id":id,"name":name,"input":input}));
    }
    fn tool_result(&mut self, id: &str, content: Value) {
        self.user(json!({"type":"tool_result","tool_use_id":id,"content":content}));
    }
    /// Signed reasoning replays as a thinking block; unsigned text cannot.
    fn thinking(&mut self, text: &str, signature: Option<&str>) {
        if let Some(signature) = signature.filter(|s| !s.is_empty()) {
            self.assistant(json!({"type":"thinking","thinking":text,"signature":signature}));
        }
    }
    fn finish(self) -> Result<Vec<Value>> {
        if self.messages.is_empty() {
            return Err(ApiError::bad_request("input must contain a message"));
        }
        Ok(self.messages)
    }
}

fn data_url(url: &str) -> Option<(String, String)> {
    let (meta, data) = url.strip_prefix("data:")?.split_once(',')?;
    let mime = meta.strip_suffix(";base64")?;
    Some((mime.to_owned(), data.to_owned()))
}

fn image_source(url: &str) -> Value {
    match data_url(url) {
        Some((media_type, data)) => json!({"type":"base64","media_type":media_type,"data":data}),
        None => json!({"type":"url","url":url}),
    }
}

/// A Responses (or normalized Chat) user content part as a Messages block.
fn user_block(part: &Value) -> Result<Value> {
    match part["type"].as_str() {
        Some("input_text") => Ok(json!({"type":"text","text":part["text"]})),
        Some("input_image") => {
            let url = part["image_url"]
                .as_str()
                .ok_or_else(|| ApiError::bad_request("input_image needs image_url"))?;
            Ok(json!({"type":"image","source":image_source(url)}))
        }
        Some("input_file") => {
            if let Some((media_type, data)) = part["file_data"].as_str().and_then(data_url) {
                let mut block = json!({"type":"document","source":{"type":"base64","media_type":media_type,"data":data}});
                if let Some(name) = part["filename"].as_str() {
                    block["title"] = name.into();
                }
                Ok(block)
            } else if let Some(url) = part["file_url"].as_str() {
                Ok(json!({"type":"document","source":{"type":"url","url":url}}))
            } else {
                Err(ApiError::bad_request(
                    "The Messages wire accepts inline or linked file data only",
                ))
            }
        }
        Some(kind) => Err(ApiError::bad_request(format!(
            "The Messages wire cannot accept {kind} content"
        ))),
        None => Err(ApiError::bad_request("Content part type is required")),
    }
}

fn arguments(text: &str) -> Value {
    serde_json::from_str::<Value>(text)
        .ok()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({"input":text}))
}

fn tool_output(output: &Value) -> Value {
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

/// Responses-shaped tools (also the normalized Chat tools) as Messages tools.
fn tools(declared: &Value, out: &mut Vec<Value>) -> Result<()> {
    for tool in declared.as_array().into_iter().flatten() {
        match tool["type"].as_str() {
            Some("function") => {
                let mut declared =
                    json!({"name":tool["name"],"input_schema":schema(&tool["parameters"])});
                if let Some(description) = tool.get("description").filter(|v| !v.is_null()) {
                    declared["description"] = description.clone();
                }
                out.push(declared);
            }
            Some("web_search" | "web_search_preview" | "web_search_preview_2025_03_11") => {
                out.push(json!({"type":"web_search_20250305","name":"web_search"}));
            }
            Some("namespace") => tools(&tool["tools"], out)?,
            Some(kind) => {
                return Err(ApiError::bad_request(format!(
                    "The Messages wire cannot use {kind} tools"
                )));
            }
            None => return Err(ApiError::bad_request("Tool type is required")),
        }
    }
    Ok(())
}

/// A normalized Responses tool choice and the parallel flag as a Messages choice.
fn tool_choice(choice: Option<&Value>, parallel: Option<&Value>) -> Option<Value> {
    let mut mapped = match choice {
        Some(Value::String(mode)) => match mode.as_str() {
            "required" => Some(json!({"type":"any"})),
            "none" => Some(json!({"type":"none"})),
            _ => Some(json!({"type":"auto"})),
        },
        Some(Value::Object(object)) => match object.get("type").and_then(Value::as_str) {
            Some("function") => Some(json!({"type":"tool","name":object["name"]})),
            Some("allowed_tools") if object.get("mode") == Some(&json!("required")) => {
                Some(json!({"type":"any"}))
            }
            _ => Some(json!({"type":"auto"})),
        },
        _ => None,
    };
    if parallel == Some(&Value::Bool(false)) {
        let choice = mapped.get_or_insert_with(|| json!({"type":"auto"}));
        if choice["type"] != "none" {
            choice["disable_parallel_tool_use"] = true.into();
        }
    }
    mapped
}

fn thinking_budget(effort: &str) -> Result<Option<u64>> {
    Ok(match effort {
        "none" => None,
        "minimal" | "low" => Some(1_024),
        "medium" => Some(8_192),
        "high" => Some(24_576),
        "xhigh" => Some(32_768),
        _ => return Err(ApiError::bad_request("Invalid reasoning effort")),
    })
}

/// Output limit and thinking budget together: Anthropic requires the budget to fit.
fn limits(
    out: &mut Map<String, Value>,
    max_tokens: Option<u64>,
    effort: Option<&str>,
) -> Result<()> {
    let mut max_tokens = max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
    if let Some(effort) = effort
        && let Some(budget) = thinking_budget(effort)?
    {
        if max_tokens <= budget {
            max_tokens = budget + 1_000;
        }
        out.insert(
            "thinking".into(),
            json!({"type":"enabled","budget_tokens":budget}),
        );
    }
    out.insert("max_tokens".into(), max_tokens.into());
    Ok(())
}

fn copy(from: &Value, to: &mut Map<String, Value>, keys: &[&str]) {
    for key in keys {
        if let Some(value) = from.get(*key).filter(|v| !v.is_null()) {
            to.insert((*key).into(), value.clone());
        }
    }
}

fn finish(
    body: &Value,
    system: Vec<String>,
    turns: Turns,
    declared: Vec<Value>,
    choice: Option<Value>,
    max_tokens: Option<u64>,
    effort: Option<&str>,
) -> Result<Value> {
    let mut out = Map::new();
    out.insert("model".into(), body["model"].clone());
    let system = system
        .into_iter()
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>();
    if !system.is_empty() {
        out.insert("system".into(), system.join("\n\n").into());
    }
    out.insert("messages".into(), turns.finish()?.into());
    if !declared.is_empty() {
        out.insert("tools".into(), declared.into());
        if let Some(choice) = choice {
            out.insert("tool_choice".into(), choice);
        }
    }
    copy(body, &mut out, &["temperature", "top_p"]);
    limits(&mut out, max_tokens, effort)?;
    Ok(out.into())
}

/// A Responses request: input items into turns, Responses tools and controls.
pub fn from_responses(body: &Value) -> Result<Value> {
    if body
        .get("previous_response_id")
        .is_some_and(|v| !v.is_null())
    {
        return Err(ApiError::bad_request(
            "The Messages wire requires the full conversation input; previous_response_id is not supported",
        ));
    }
    let mut body = body.clone();
    normalize_tools(
        body.as_object_mut()
            .ok_or_else(|| ApiError::bad_request("Request body must be an object"))?,
    )?;
    let mut system = Vec::new();
    if let Some(instructions) = body["instructions"].as_str() {
        system.push(instructions.to_owned());
    }
    let mut turns = Turns::default();
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
                    Some("system" | "developer") => system.extend(
                        content
                            .iter()
                            .filter_map(|part| part["text"].as_str())
                            .map(str::to_owned),
                    ),
                    Some("assistant") => {
                        for part in &content {
                            if let Some(text) =
                                part["text"].as_str().or_else(|| part["refusal"].as_str())
                            {
                                turns.assistant_text(text);
                            }
                        }
                    }
                    _ => {
                        for part in &content {
                            turns.user(user_block(part)?);
                        }
                    }
                }
            }
            "reasoning" => turns.thinking(
                &crate::convert::responses::output::reasoning_text(item),
                item["encrypted_content"].as_str(),
            ),
            "function_call" => turns.tool_use(
                item["call_id"].as_str().unwrap_or_default(),
                item["name"].as_str().unwrap_or_default(),
                arguments(item["arguments"].as_str().unwrap_or("{}")),
            ),
            "function_call_output" => turns.tool_result(
                item["call_id"].as_str().unwrap_or_default(),
                tool_output(&item["output"]),
            ),
            kind => {
                return Err(ApiError::bad_request(format!(
                    "The Messages wire cannot accept {kind} input items"
                )));
            }
        }
    }
    let mut declared = Vec::new();
    tools(&body["tools"], &mut declared)?;
    let choice = tool_choice(body.get("tool_choice"), body.get("parallel_tool_calls"));
    finish(
        &body,
        system,
        turns,
        declared,
        choice,
        body["max_output_tokens"].as_u64(),
        body.pointer("/reasoning/effort").and_then(Value::as_str),
    )
}

/// A Chat request: messages into turns, Chat tools and controls.
pub fn from_chat(body: &Value) -> Result<Value> {
    let mut body = body.clone();
    normalize_tools(
        body.as_object_mut()
            .ok_or_else(|| ApiError::bad_request("Request body must be an object"))?,
    )?;
    let mut system = Vec::new();
    let mut turns = Turns::default();
    for message in body["messages"].as_array().into_iter().flatten() {
        match message["role"].as_str().unwrap_or_default() {
            "system" | "developer" => {
                for part in content_parts(message["content"].clone(), false)? {
                    if let Some(text) = part["text"].as_str() {
                        system.push(text.to_owned());
                    }
                }
            }
            "user" => {
                for part in content_parts(message["content"].clone(), false)? {
                    turns.user(user_block(&part)?);
                }
            }
            "assistant" => {
                if let Some(items) = message.get("reasoning").filter(|v| !v.is_null()) {
                    let items = match items {
                        Value::Array(items) => items.clone(),
                        other => vec![other.clone()],
                    };
                    for item in items {
                        turns.thinking(
                            &crate::convert::responses::output::reasoning_text(&item),
                            item["encrypted_content"].as_str(),
                        );
                    }
                } else if let Some(text) = message["reasoning_content"].as_str() {
                    turns.thinking(text, message["encrypted_content"].as_str());
                }
                if !message["content"].is_null() {
                    for part in content_parts(message["content"].clone(), true)? {
                        if let Some(text) = part["text"].as_str() {
                            turns.assistant_text(text);
                        }
                    }
                }
                if let Some(refusal) = message["refusal"].as_str() {
                    turns.assistant_text(refusal);
                }
                for call in message["tool_calls"].as_array().into_iter().flatten() {
                    let payload = call.get("function").or_else(|| call.get("custom"));
                    let name = payload
                        .and_then(|p| p["name"].as_str())
                        .ok_or_else(|| ApiError::bad_request("Tool calls require a name"))?;
                    let input = payload
                        .and_then(|p| p["arguments"].as_str().or_else(|| p["input"].as_str()))
                        .unwrap_or("{}");
                    turns.tool_use(
                        call["id"].as_str().unwrap_or_default(),
                        name,
                        arguments(input),
                    );
                }
            }
            "tool" => {
                let output = match &message["content"] {
                    Value::Array(_) => tool_output(&Value::Array(content_parts(
                        message["content"].clone(),
                        false,
                    )?)),
                    other => tool_output(other),
                };
                turns.tool_result(message["tool_call_id"].as_str().unwrap_or_default(), output);
            }
            _ => return Err(ApiError::bad_request("Unsupported message role")),
        }
    }
    let mut declared = Vec::new();
    tools(&body["tools"], &mut declared)?;
    if body.get("web_search_options").is_some() {
        declared.push(json!({"type":"web_search_20250305","name":"web_search"}));
    }
    let choice = tool_choice(body.get("tool_choice"), body.get("parallel_tool_calls"));
    let mut out = finish(
        &body,
        system,
        turns,
        declared,
        choice,
        body["max_completion_tokens"]
            .as_u64()
            .or_else(|| body["max_tokens"].as_u64()),
        body["reasoning_effort"]
            .as_str()
            .or_else(|| body.pointer("/reasoning/effort").and_then(Value::as_str)),
    )?;
    if let Some(stop) = body.get("stop").filter(|v| !v.is_null()) {
        out["stop_sequences"] = match stop {
            Value::String(text) => json!([text]),
            other => other.clone(),
        };
    }
    Ok(out)
}

/// A Messages request: the caller's own shape without local extensions.
pub fn from_messages(body: &Value) -> Result<Value> {
    let mut body = body.clone();
    let object = body
        .as_object_mut()
        .ok_or_else(|| ApiError::bad_request("Request body must be an object"))?;
    for key in ["client_metadata", "stream_options"] {
        object.remove(key);
    }
    // The client attribution block is prompt metadata of another service.
    if object.get("system").is_some_and(Value::is_array) {
        let texts = system_texts(&body)?
            .into_iter()
            .map(|text| json!({"type":"text","text":text}))
            .collect::<Vec<_>>();
        let object = body.as_object_mut().expect("object");
        if texts.is_empty() {
            object.remove("system");
        } else {
            object.insert("system".into(), texts.into());
        }
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_items_become_messages_turns_with_tools_and_thinking_budget() {
        let body = json!({"model":"claude","instructions":"Be brief.","reasoning":{"effort":"medium"},
        "parallel_tool_calls":false,
        "tools":[{"type":"function","name":"lookup","description":"d","parameters":{"type":"object","properties":{"q":{"type":"string"}}}}],
        "tool_choice":"required",
        "input":[
            {"type":"message","role":"user","content":[{"type":"input_text","text":"hi"},{"type":"input_image","image_url":"data:image/png;base64,AA=="}]},
            {"type":"reasoning","summary":[{"type":"summary_text","text":"think"}],"encrypted_content":"sig"},
            {"type":"function_call","call_id":"toolu_1","name":"lookup","arguments":"{\"q\":\"x\"}"},
            {"type":"function_call_output","call_id":"toolu_1","output":"found"},
            {"type":"message","role":"assistant","content":[{"type":"output_text","text":"done"}]}
        ]});
        let out = from_responses(&body).unwrap();
        assert_eq!(out["system"], "Be brief.");
        let messages = out["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 4);
        assert_eq!(
            messages[0]["content"][1]["source"]["media_type"],
            "image/png"
        );
        assert_eq!(messages[1]["content"][0]["type"], "thinking");
        assert_eq!(messages[1]["content"][0]["signature"], "sig");
        assert_eq!(messages[1]["content"][1]["input"]["q"], "x");
        assert_eq!(messages[2]["content"][0]["tool_use_id"], "toolu_1");
        assert_eq!(messages[3]["content"][0]["text"], "done");
        assert_eq!(
            out["tools"][0]["input_schema"]["properties"]["q"]["type"],
            "string"
        );
        assert_eq!(out["tool_choice"]["type"], "any");
        assert_eq!(out["tool_choice"]["disable_parallel_tool_use"], true);
        assert_eq!(out["thinking"]["budget_tokens"], 8192);
        assert_eq!(out["max_tokens"], 9192);
    }

    #[test]
    fn chat_messages_become_turns_and_native_messages_lose_only_local_fields() {
        let body = json!({"model":"claude","max_tokens":300,"stop":"END",
            "messages":[{"role":"system","content":"sys"},{"role":"user","content":"q"},
                {"role":"assistant","content":null,"tool_calls":[{"id":"t1","type":"function","function":{"name":"f","arguments":"{}"}}]},
                {"role":"tool","tool_call_id":"t1","content":"r"}],
            "tools":[{"type":"function","function":{"name":"f","parameters":{"type":"object"}}}],
            "tool_choice":{"type":"function","function":{"name":"f"}}});
        let out = from_chat(&body).unwrap();
        assert_eq!(out["system"], "sys");
        assert_eq!(out["messages"][1]["content"][0]["type"], "tool_use");
        assert_eq!(out["messages"][2]["content"][0]["type"], "tool_result");
        assert_eq!(out["tool_choice"], json!({"type":"tool","name":"f"}));
        assert_eq!(out["max_tokens"], 300);
        assert_eq!(out["stop_sequences"], json!(["END"]));
        let native = from_messages(&json!({"model":"claude","max_tokens":5,"client_metadata":{"x":1},
            "system":[{"type":"text","text":"x-anthropic-billing-header: cc"},{"type":"text","text":"keep"}],
            "messages":[{"role":"user","content":"hi"}]}))
        .unwrap();
        assert!(native.get("client_metadata").is_none());
        assert_eq!(native["system"], json!([{"type":"text","text":"keep"}]));
    }
}
