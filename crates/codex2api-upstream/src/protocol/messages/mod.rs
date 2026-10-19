//! Messages wire support: validation, errors, the ordered SSE block writer and
//! the synchronous collector.
pub mod responses;

use super::{Error, Result};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

pub(crate) fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| Error::bad_request(format!("{key} must be text")))
}
pub(crate) fn blocks(value: &Value) -> Result<std::borrow::Cow<'_, [Value]>> {
    match value {
        Value::String(v) => Ok(vec![json!({"type":"text","text":v})].into()),
        Value::Array(v) => Ok(v.as_slice().into()),
        _ => Err(Error::bad_request(
            "Message content must be text or content blocks",
        )),
    }
}

pub fn validate(body: &Value) -> Result<()> {
    if body["max_tokens"].as_u64().is_none_or(|n| n == 0) {
        return Err(Error::bad_request("max_tokens must be a positive integer"));
    }
    if let Some(stops) = body.get("stop_sequences")
        && !stops
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
    {
        return Err(Error::bad_request(
            "stop_sequences must be an array of strings",
        ));
    }
    for key in ["temperature", "top_p"] {
        if body
            .get(key)
            .is_some_and(|v| !v.as_f64().is_some_and(|v| (0.0..=1.0).contains(&v)))
        {
            return Err(Error::bad_request(format!("{key} must be between 0 and 1")));
        }
    }
    for field in ["thinking", "output_config", "tool_choice"] {
        if body.get(field).is_some_and(|v| !v.is_object()) {
            return Err(Error::bad_request(format!("{field} must be an object")));
        }
    }
    let messages = body["messages"]
        .as_array()
        .filter(|m| !m.is_empty())
        .ok_or_else(|| Error::bad_request("messages must be a nonempty array"))?;
    for message in messages {
        let assistant = match message["role"].as_str() {
            Some("assistant") => true,
            Some("user") => false,
            _ => {
                return Err(Error::bad_request(
                    "Messages roles must be user or assistant",
                ));
            }
        };
        for block in blocks(&message["content"])?.iter() {
            match block["type"].as_str() {
                Some("text") => {
                    text(block, "text")?;
                }
                Some("tool_use") if assistant => {
                    text(block, "id")?;
                    text(block, "name")?;
                    if !block["input"].is_object() {
                        return Err(Error::bad_request("tool_use input must be an object"));
                    }
                }
                Some("tool_result") if !assistant => {
                    text(block, "tool_use_id")?;
                    if let Some(v) = block.get("content") {
                        blocks(v)?;
                    }
                }
                Some("tool_use" | "tool_result") => {
                    return Err(Error::bad_request(
                        "tool_use belongs to assistant and tool_result to user messages",
                    ));
                }
                Some(_) => {}
                None => return Err(Error::bad_request("Content blocks require a type")),
            }
        }
    }
    if let Some(system) = body.get("system") {
        for block in blocks(system)?.iter() {
            if block["type"] != "text" {
                return Err(Error::bad_request("system accepts text blocks only"));
            }
            text(block, "text")?;
        }
    }
    for tool in body["tools"].as_array().into_iter().flatten() {
        text(tool, "name")?;
        if tool.get("type").is_none_or(|t| t == "custom") && !tool["input_schema"].is_object() {
            return Err(Error::bad_request("input_schema must be an object"));
        }
    }
    if body.get("tools").is_some_and(|v| !v.is_array()) {
        return Err(Error::bad_request("tools must be an array"));
    }
    Ok(())
}

pub fn error(status: http::StatusCode, value: &Value) -> Value {
    let kind = match status.as_u16() {
        401 => "authentication_error",
        403 => "permission_error",
        404 => "not_found_error",
        413 => "request_too_large",
        429 => "rate_limit_error",
        529 => "overloaded_error",
        400..=499 => "invalid_request_error",
        _ => "api_error",
    };
    let mut error = value
        .get("error")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({"message":"The request could not be completed."}));
    error["type"] = kind.into();
    json!({"type":"error","error":error})
}

/// System text without the client attribution block Claude Code injects.
pub(crate) fn system_texts(body: &Value) -> Result<Vec<&str>> {
    let Some(system) = body.get("system") else {
        return Ok(Vec::new());
    };
    let mut texts = Vec::new();
    for block in match system {
        Value::String(text) => {
            return Ok(vec![text.as_str()]
                .into_iter()
                .filter(|t| !t.is_empty())
                .collect());
        }
        Value::Array(blocks) => blocks,
        _ => return Err(Error::bad_request("system must be text or text blocks")),
    } {
        let text = text(block, "text")?;
        if !text.is_empty() && !text.starts_with("x-anthropic-billing-header: ") {
            texts.push(text);
        }
    }
    Ok(texts)
}

/// Messages image source as a URL other protocols accept.
pub(crate) fn image_url(source: &Value) -> Result<String> {
    match source["type"].as_str() {
        Some("url") => Ok(text(source, "url")?.to_owned()),
        Some("base64") => Ok(format!(
            "data:{};base64,{}",
            text(source, "media_type")?,
            text(source, "data")?
        )),
        _ => Err(Error::bad_request("Unsupported Messages image source")),
    }
}
/// Messages input schema as function parameters; an absent schema takes no arguments.
pub(crate) fn schema(value: &Value) -> Value {
    let mut schema = match value {
        Value::Object(_) => value.clone(),
        _ => json!({"type":"object"}),
    };
    if schema["type"] == "object" && !schema["properties"].is_object() {
        schema["properties"] = json!({});
    }
    schema
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Text,
    Thinking,
    Tool,
}
struct Block {
    kind: Kind,
    start: Value,
    text: String,
    emitted: usize,
    signature: Option<String>,
    started: bool,
    complete: bool,
}

/// Emits Messages content blocks strictly one at a time, in creation order.
/// Later blocks buffer until earlier ones complete, so interleaved supplier
/// items never produce deltas for a block that is not open.
#[derive(Default)]
pub(crate) struct Writer {
    started: bool,
    blocks: Vec<Block>,
    keys: HashMap<(u64, u64), usize>,
    current: usize,
    tool_use: bool,
}
impl Writer {
    pub fn started(&self) -> bool {
        self.started
    }
    pub fn tool_use(&self) -> bool {
        self.tool_use
    }
    /// Messages reports final usage on `message_delta`; the opening counters are zero.
    pub fn start(&mut self, id: &Value, model: &Value) -> Vec<Value> {
        self.started = true;
        let id = id
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("msg_{}", uuid::Uuid::new_v4().simple()));
        vec![
            json!({"type":"message_start","message":{"id":id,"type":"message","role":"assistant","model":model,"content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":0,"output_tokens":0}}}),
        ]
    }
    pub fn has(&self, key: (u64, u64)) -> bool {
        self.keys.contains_key(&key)
    }
    pub fn ensure(&mut self, key: (u64, u64), kind: Kind, start: Value, out: &mut Vec<Value>) {
        if self.keys.contains_key(&key) {
            return;
        }
        self.tool_use |= kind == Kind::Tool;
        self.keys.insert(key, self.blocks.len());
        self.blocks.push(Block {
            kind,
            start,
            text: String::new(),
            emitted: 0,
            signature: None,
            started: false,
            complete: false,
        });
        self.flush(out);
    }
    pub fn append(&mut self, key: (u64, u64), text: &str, out: &mut Vec<Value>) -> Result<()> {
        let index = *self.keys.get(&key).ok_or_else(Error::upstream_incomplete)?;
        let block = &mut self.blocks[index];
        if !text.is_empty() {
            if block.complete {
                return Err(Error::upstream_incomplete());
            }
            block.text.push_str(text);
            self.flush(out);
        }
        Ok(())
    }
    /// Add only the part of a terminal snapshot that deltas did not deliver.
    pub fn reconcile(&mut self, key: (u64, u64), full: &str, out: &mut Vec<Value>) -> Result<()> {
        let Some(&index) = self.keys.get(&key) else {
            return Ok(());
        };
        let suffix = full
            .strip_prefix(self.blocks[index].text.as_str())
            .unwrap_or("")
            .to_owned();
        self.append(key, &suffix, out)
    }
    pub fn signature(&mut self, key: (u64, u64), signature: &str) {
        if let Some(&index) = self.keys.get(&key)
            && !signature.is_empty()
        {
            self.blocks[index].signature = Some(signature.to_owned());
        }
    }
    pub fn close(&mut self, key: (u64, u64), out: &mut Vec<Value>) {
        if let Some(&index) = self.keys.get(&key) {
            self.blocks[index].complete = true;
            self.flush(out);
        }
    }
    pub fn finish(&mut self, stop_reason: &str, usage: Value) -> Vec<Value> {
        let mut out = Vec::new();
        for block in &mut self.blocks {
            block.complete = true;
        }
        self.flush(&mut out);
        out.push(json!({"type":"message_delta","delta":{"stop_reason":stop_reason,"stop_sequence":null},"usage":usage}));
        out.push(json!({"type":"message_stop"}));
        out
    }
    fn flush(&mut self, out: &mut Vec<Value>) {
        while let Some(block) = self.blocks.get_mut(self.current) {
            let index = self.current;
            if !block.started {
                block.started = true;
                out.push(
                    json!({"type":"content_block_start","index":index,"content_block":block.start}),
                );
            }
            if block.emitted < block.text.len() {
                let suffix = &block.text[block.emitted..];
                let delta = match block.kind {
                    Kind::Text => json!({"type":"text_delta","text":suffix}),
                    Kind::Thinking => json!({"type":"thinking_delta","thinking":suffix}),
                    Kind::Tool => json!({"type":"input_json_delta","partial_json":suffix}),
                };
                out.push(json!({"type":"content_block_delta","index":index,"delta":delta}));
                block.emitted = block.text.len();
            }
            if !block.complete {
                break;
            }
            if let Some(signature) = block.signature.take() {
                out.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"signature_delta","signature":signature}}));
            }
            out.push(json!({"type":"content_block_stop","index":index}));
            self.current += 1;
        }
    }
}

#[derive(Default)]
struct Collected {
    block: Value,
    text: String,
    json: String,
}

/// Assembles a Messages response from its stream events.
#[derive(Default)]
pub struct Output {
    message: Value,
    blocks: BTreeMap<u64, Collected>,
}
impl Output {
    pub(crate) fn push(&mut self, mut event: Value) -> Result<()> {
        let index = event["index"].as_u64().unwrap_or(0);
        match event["type"].as_str() {
            Some("message_start") => self.message = event["message"].take(),
            Some("content_block_start") => {
                self.blocks.insert(
                    index,
                    Collected {
                        block: event["content_block"].take(),
                        ..Default::default()
                    },
                );
            }
            Some("content_block_delta") => {
                let block = self
                    .blocks
                    .get_mut(&index)
                    .ok_or_else(Error::upstream_incomplete)?;
                let delta = &mut event["delta"];
                match delta["type"].as_str() {
                    Some("text_delta") => block.text.push_str(delta["text"].as_str().unwrap_or("")),
                    Some("thinking_delta") => block
                        .text
                        .push_str(delta["thinking"].as_str().unwrap_or("")),
                    Some("input_json_delta") => block
                        .json
                        .push_str(delta["partial_json"].as_str().unwrap_or("")),
                    Some("signature_delta") => block.block["signature"] = delta["signature"].take(),
                    Some("citations_delta") => {
                        let citation = delta["citation"].take();
                        match block.block["citations"].as_array_mut() {
                            Some(citations) => citations.push(citation),
                            None => block.block["citations"] = json!([citation]),
                        }
                    }
                    _ => {}
                }
            }
            Some("message_delta") => {
                for key in ["stop_reason", "stop_sequence"] {
                    if let Some(value) = event["delta"].get_mut(key) {
                        self.message[key] = value.take();
                    }
                }
                if let Some(usage) = event["usage"].as_object_mut() {
                    if !self.message["usage"].is_object() {
                        self.message["usage"] = json!({});
                    }
                    for (key, value) in std::mem::take(usage) {
                        if !value.is_null() {
                            self.message["usage"][key] = value;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(crate) fn finish(mut self) -> Result<Value> {
        if !self.message.is_object() || self.message["stop_reason"].is_null() {
            return Err(Error::upstream_incomplete());
        }
        let mut content = Vec::with_capacity(self.blocks.len());
        for mut collected in std::mem::take(&mut self.blocks).into_values() {
            let block = &mut collected.block;
            match block["type"].as_str() {
                Some("text") => {
                    let mut text = block["text"].as_str().unwrap_or("").to_owned();
                    text.push_str(&collected.text);
                    block["text"] = text.into();
                }
                Some("thinking") => {
                    let mut text = block["thinking"].as_str().unwrap_or("").to_owned();
                    text.push_str(&collected.text);
                    block["thinking"] = text.into();
                }
                Some("tool_use" | "server_tool_use") if !collected.json.is_empty() => {
                    block["input"] = serde_json::from_str(&collected.json)
                        .map_err(|_| Error::upstream_incomplete())?;
                }
                _ => {}
            }
            content.push(collected.block);
        }
        self.message["content"] = content.into();
        Ok(self.message)
    }
}
