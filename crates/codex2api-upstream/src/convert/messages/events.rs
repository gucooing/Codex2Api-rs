//! Messages events converted into the caller's protocol: Responses events, Chat
//! chunks or the wire's own events, each read directly from the Messages stream.
use crate::convert::EventConversion;
use crate::convert::responses::chat_output::Chunks;
use crate::inbound::{Inbound, Result};
use serde_json::{Value, json};
use std::collections::HashMap;

/// The event conversion for a Messages generation read by `inbound`.
pub(crate) fn conversion(
    inbound: Inbound,
    include_usage: bool,
    model: &str,
) -> Box<dyn EventConversion> {
    match inbound {
        Inbound::Chat => Box::new(ToChat::new(model.to_owned(), include_usage)),
        Inbound::Messages => Box::new(Native),
        _ => Box::new(ToResponses::new(model.to_owned())),
    }
}

fn item_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

/// The error object of an `error` event in the Responses error shape.
fn event_error(event: &Value) -> Option<Value> {
    if event["type"] != "error" {
        return None;
    }
    let error = &event["error"];
    let kind = error["type"].as_str().unwrap_or("api_error");
    Some(json!({
        "type": kind,
        "code": kind,
        "message": error["message"].as_str().unwrap_or("The supplier request failed."),
    }))
}

/// Cumulative Messages usage: `message_start` reports the input, `message_delta`
/// the output; cache reads and writes are counted apart from the fresh input.
#[derive(Default)]
struct Usage {
    input: Option<u64>,
    output: Option<u64>,
    cache_read: Option<u64>,
    cache_write: Option<u64>,
}
impl Usage {
    fn merge(&mut self, usage: &Value) {
        for (key, slot) in [
            ("input_tokens", &mut self.input),
            ("output_tokens", &mut self.output),
            ("cache_read_input_tokens", &mut self.cache_read),
            ("cache_creation_input_tokens", &mut self.cache_write),
        ] {
            if let Some(count) = usage[key].as_u64() {
                *slot = Some(count);
            }
        }
    }
    fn total_input(&self) -> Option<u64> {
        self.input.map(|n| {
            n.saturating_add(self.cache_read.unwrap_or(0))
                .saturating_add(self.cache_write.unwrap_or(0))
        })
    }
    fn responses(&self) -> Value {
        let mut value = json!({});
        if let Some(input) = self.total_input() {
            value["input_tokens"] = input.into();
        }
        if let Some(output) = self.output {
            value["output_tokens"] = output.into();
        }
        if let (Some(input), Some(output)) = (self.total_input(), self.output) {
            value["total_tokens"] = input.saturating_add(output).into();
        }
        let mut details = json!({});
        if let Some(cached) = self.cache_read {
            details["cached_tokens"] = cached.into();
        }
        if let Some(written) = self.cache_write {
            details["cache_write_tokens"] = written.into();
        }
        if details.as_object().is_some_and(|d| !d.is_empty()) {
            value["input_tokens_details"] = details;
        }
        value
    }
    fn chat(&self) -> Value {
        let mut value = json!({});
        if let Some(input) = self.total_input() {
            value["prompt_tokens"] = input.into();
        }
        if let Some(output) = self.output {
            value["completion_tokens"] = output.into();
        }
        if let (Some(input), Some(output)) = (self.total_input(), self.output) {
            value["total_tokens"] = input.saturating_add(output).into();
        }
        if let Some(cached) = self.cache_read {
            value["prompt_tokens_details"] = json!({"cached_tokens":cached});
        }
        value
    }
}

enum Item {
    Reasoning {
        id: String,
        text: String,
        signature: Option<String>,
    },
    Message {
        id: String,
        text: String,
    },
    FunctionCall {
        id: String,
        call_id: String,
        name: String,
        arguments: String,
    },
}
impl Item {
    fn id(&self) -> &str {
        match self {
            Self::Reasoning { id, .. }
            | Self::Message { id, .. }
            | Self::FunctionCall { id, .. } => id,
        }
    }

    fn value(&self) -> Value {
        match self {
            Self::Reasoning {
                id,
                text,
                signature,
            } => {
                let mut item = json!({"type":"reasoning","id":id,"summary":[]});
                if !text.is_empty() {
                    item["summary"] = json!([{"type":"summary_text","text":text}]);
                }
                if let Some(signature) = signature {
                    item["encrypted_content"] = signature.as_str().into();
                }
                item
            }
            Self::Message { id, text } => {
                json!({"type":"message","id":id,"role":"assistant","status":"completed",
                "content":[{"type":"output_text","text":text,"annotations":[]}]})
            }
            Self::FunctionCall {
                id,
                call_id,
                name,
                arguments,
            } => {
                json!({"type":"function_call","id":id,"call_id":call_id,"name":name,"arguments":arguments,"status":"completed"})
            }
        }
    }
}

/// Responses callers: content blocks as Responses items.
struct ToResponses {
    id: String,
    model: String,
    created_at: i64,
    started: bool,
    finished: bool,
    items: Vec<Item>,
    /// Block index of the stream to the item it feeds.
    blocks: HashMap<u64, usize>,
    usage: Usage,
    stop: Option<String>,
}
impl ToResponses {
    fn new(model: String) -> Self {
        Self {
            id: item_id("resp"),
            model,
            created_at: chrono::Utc::now().timestamp(),
            started: false,
            finished: false,
            items: Vec::new(),
            blocks: HashMap::new(),
            usage: Usage::default(),
            stop: None,
        }
    }
    fn response(&self, status: &str) -> Value {
        json!({"id":self.id,"object":"response","created_at":self.created_at,"status":status,"model":self.model})
    }
    fn start(&mut self, message: &Value, out: &mut Vec<Value>) {
        if self.started {
            return;
        }
        self.started = true;
        if let Some(id) = message["id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 128)
        {
            self.id = id.to_owned();
        }
        if let Some(model) = message["model"].as_str().filter(|m| !m.is_empty()) {
            self.model = model.to_owned();
        }
        self.usage.merge(&message["usage"]);
        out.push(json!({"type":"response.created","response":self.response("in_progress")}));
    }
    fn open(&mut self, index: u64, block: &Value, out: &mut Vec<Value>) {
        let item = match block["type"].as_str() {
            Some("text") => Item::Message {
                id: item_id("msg"),
                text: String::new(),
            },
            Some("thinking") => Item::Reasoning {
                id: item_id("rs"),
                text: String::new(),
                signature: None,
            },
            Some("redacted_thinking") => Item::Reasoning {
                id: item_id("rs"),
                text: String::new(),
                signature: block["data"].as_str().map(str::to_owned),
            },
            Some("tool_use") => Item::FunctionCall {
                id: item_id("fc"),
                call_id: block["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map_or_else(|| item_id("call"), str::to_owned),
                name: block["name"].as_str().unwrap_or_default().to_owned(),
                arguments: String::new(),
            },
            // Server tool activity has no Responses item; its text follows in a text block.
            _ => return,
        };
        let mut added = item.value();
        self.items.push(item);
        let position = self.items.len() - 1;
        self.blocks.insert(index, position);
        if added["type"] == "message" {
            added["status"] = "in_progress".into();
            added["content"] = json!([]);
            out.push(
                json!({"type":"response.output_item.added","output_index":position,"item":added}),
            );
            out.push(json!({"type":"response.content_part.added","item_id":self.items[position].id(),"output_index":position,"content_index":0,
                "part":{"type":"output_text","text":"","annotations":[]}}));
        } else {
            if added["type"] == "function_call" {
                added["status"] = "in_progress".into();
                added["arguments"] = "".into();
            }
            out.push(
                json!({"type":"response.output_item.added","output_index":position,"item":added}),
            );
        }
        // A block can carry its first content before any delta arrives. Emit it
        // once so streaming readers and the completed item see the same content.
        for (field, kind) in [("text", "text_delta"), ("thinking", "thinking_delta")] {
            if let Some(text) = block[field].as_str().filter(|text| !text.is_empty()) {
                self.delta(index, &json!({"type":kind,field:text}), out);
            }
        }
        if let Some(input) = block
            .get("input")
            .and_then(Value::as_object)
            .filter(|input| !input.is_empty())
        {
            self.delta(index, &json!({"type":"input_json_delta","partial_json":Value::Object(input.clone()).to_string()}), out);
        }
    }
    fn delta(&mut self, index: u64, delta: &Value, out: &mut Vec<Value>) {
        let Some(&position) = self.blocks.get(&index) else {
            return;
        };
        match (&mut self.items[position], delta["type"].as_str()) {
            (Item::Message { id, text, .. }, Some("text_delta")) => {
                let piece = delta["text"].as_str().unwrap_or_default();
                text.push_str(piece);
                out.push(json!({"type":"response.output_text.delta","item_id":id,"output_index":position,"content_index":0,"delta":piece,"logprobs":[]}));
            }
            (Item::Reasoning { id, text, .. }, Some("thinking_delta")) => {
                let piece = delta["thinking"].as_str().unwrap_or_default();
                if piece.is_empty() {
                    return;
                }
                if text.is_empty() {
                    out.push(json!({"type":"response.reasoning_summary_part.added","item_id":id,"output_index":position,"summary_index":0,"part":{"type":"summary_text","text":""}}));
                }
                text.push_str(piece);
                out.push(json!({"type":"response.reasoning_summary_text.delta","item_id":id,"output_index":position,"summary_index":0,"delta":piece}));
            }
            (Item::Reasoning { signature, .. }, Some("signature_delta")) => {
                *signature = delta["signature"].as_str().map(str::to_owned);
            }
            (Item::FunctionCall { id, arguments, .. }, Some("input_json_delta")) => {
                let piece = delta["partial_json"].as_str().unwrap_or_default();
                arguments.push_str(piece);
                out.push(json!({"type":"response.function_call_arguments.delta","item_id":id,"output_index":position,"delta":piece}));
            }
            _ => {}
        }
    }
    fn close(&mut self, index: u64, out: &mut Vec<Value>) {
        let Some(position) = self.blocks.remove(&index) else {
            return;
        };
        let item = &mut self.items[position];
        match item {
            Item::Reasoning { id, text, .. } => {
                if !text.is_empty() {
                    out.push(json!({"type":"response.reasoning_summary_text.done","item_id":id,"output_index":position,"summary_index":0,"text":text}));
                    out.push(json!({"type":"response.reasoning_summary_part.done","item_id":id,"output_index":position,"summary_index":0,"part":{"type":"summary_text","text":text}}));
                }
            }
            Item::Message { id, text, .. } => {
                out.push(json!({"type":"response.output_text.done","item_id":id,"output_index":position,"content_index":0,"text":text,"logprobs":[]}));
                out.push(json!({"type":"response.content_part.done","item_id":id,"output_index":position,"content_index":0,
                    "part":item.value()["content"][0]}));
            }
            Item::FunctionCall { id, arguments, .. } => {
                if arguments.is_empty() {
                    *arguments = "{}".into();
                }
                out.push(json!({"type":"response.function_call_arguments.done","item_id":id,"output_index":position,"arguments":arguments}));
            }
        }
        out.push(
            json!({"type":"response.output_item.done","output_index":position,"item":item.value()}),
        );
    }
    fn fail(&mut self, error: Value, mut out: Vec<Value>) -> Vec<Value> {
        let mut response = self.response("failed");
        response["error"] = error;
        response["output"] = self.items.iter().map(Item::value).collect();
        response["usage"] = self.usage.responses();
        out.push(json!({"type":"response.failed","response":response}));
        self.finished = true;
        out
    }
}
impl EventConversion for ToResponses {
    fn event(&mut self, event: Value) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        if self.finished {
            return Ok(out);
        }
        if let Some(error) = event_error(&event) {
            if !self.started {
                self.started = true;
                out.push(
                    json!({"type":"response.created","response":self.response("in_progress")}),
                );
            }
            return Ok(self.fail(error, out));
        }
        let index = event["index"].as_u64().unwrap_or(0);
        match event["type"].as_str() {
            Some("message_start") => self.start(&event["message"], &mut out),
            Some("content_block_start") => self.open(index, &event["content_block"], &mut out),
            Some("content_block_delta") => self.delta(index, &event["delta"], &mut out),
            Some("content_block_stop") => self.close(index, &mut out),
            Some("message_delta") => {
                self.usage.merge(&event["usage"]);
                if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                    self.stop = Some(reason.to_owned());
                }
            }
            Some("message") => {
                // A synchronous reply: one message with its blocks and usage.
                self.start(&event, &mut out);
                for (position, block) in event["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    let position = position as u64;
                    self.open(position, block, &mut out);
                    match block["type"].as_str() {
                        Some("thinking") => {
                            if let Some(signature) = block["signature"].as_str() {
                                self.delta(
                                    position,
                                    &json!({"type":"signature_delta","signature":signature}),
                                    &mut out,
                                );
                            }
                        }
                        _ => {}
                    }
                    self.close(position, &mut out);
                }
                if let Some(reason) = event["stop_reason"].as_str() {
                    self.stop = Some(reason.to_owned());
                }
                out.extend(self.finish()?);
            }
            _ => {}
        }
        Ok(out)
    }
    fn finish(&mut self) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        if !self.started || self.finished {
            return Ok(out);
        }
        let (kind, status, incomplete) = match self.stop.as_deref() {
            Some("max_tokens") => (
                "response.incomplete",
                "incomplete",
                Some("max_output_tokens"),
            ),
            Some("refusal") => ("response.incomplete", "incomplete", Some("content_filter")),
            Some(_) => ("response.completed", "completed", None),
            None if !self.items.is_empty() => ("response.completed", "completed", None),
            None => {
                return Ok(self.fail(
                    json!({"code":"upstream_protocol_error","type":"server_error","message":"The upstream stream ended without a stop reason."}),
                    out,
                ));
            }
        };
        let mut response = self.response(status);
        response["output"] = self.items.iter().map(Item::value).collect();
        if let Some(reason) = incomplete {
            response["incomplete_details"] = json!({"reason":reason});
        }
        response["usage"] = self.usage.responses();
        out.push(json!({"type":kind,"response":response}));
        self.finished = true;
        Ok(out)
    }
}

/// Chat callers: content blocks as Chat chunk deltas; signed thinking travels in the
/// lossless `reasoning` extension so a later turn can replay it.
struct ToChat {
    chunks: Chunks,
    started: bool,
    finished: bool,
    tools: HashMap<u64, u64>,
    next_tool: u64,
    thinking: HashMap<u64, (String, Option<String>)>,
    usage: Usage,
    stop: Option<String>,
}
impl ToChat {
    fn new(model: String, include_usage: bool) -> Self {
        Self {
            chunks: Chunks {
                id: format!("chatcmpl_{}", uuid::Uuid::new_v4().simple()).into(),
                model: model.into(),
                created: chrono::Utc::now().timestamp().into(),
                service_tier: None,
                include_usage,
            },
            started: false,
            finished: false,
            tools: HashMap::new(),
            next_tool: 0,
            thinking: HashMap::new(),
            usage: Usage::default(),
            stop: None,
        }
    }
    fn delta(&self, delta: Value) -> Value {
        self.chunks.chunk(delta, Value::Null)
    }
    fn fail(&mut self, error: Value, out: &mut Vec<Value>) {
        let status = match error["type"].as_str() {
            Some("authentication_error") => 401,
            Some("permission_error") => 403,
            Some("not_found_error") => 404,
            Some("rate_limit_error") => 429,
            Some("invalid_request_error") => 400,
            Some("overloaded_error") => 529,
            _ => 502,
        };
        out.push(json!({"type":"error","status":status,"error":error}));
        self.finished = true;
    }
    fn end(&mut self, out: &mut Vec<Value>) {
        let finish = match self.stop.as_deref() {
            Some("tool_use") => "tool_calls",
            Some("max_tokens") => "length",
            Some("refusal") => "content_filter",
            _ => {
                if self.tools.is_empty() {
                    "stop"
                } else {
                    "tool_calls"
                }
            }
        };
        out.push(self.chunks.chunk(json!({}), finish.into()));
        if let Some(chunk) = self.chunks.usage(self.usage.chat()) {
            out.push(chunk);
        }
        self.finished = true;
    }
}
impl EventConversion for ToChat {
    fn event(&mut self, event: Value) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        if self.finished {
            return Ok(out);
        }
        if let Some(error) = event_error(&event) {
            self.fail(error, &mut out);
            return Ok(out);
        }
        let index = event["index"].as_u64().unwrap_or(0);
        match event["type"].as_str() {
            Some("message_start") => {
                let message = &event["message"];
                if !self.started {
                    self.started = true;
                    if let Some(id) = message["id"].as_str().filter(|id| !id.is_empty()) {
                        self.chunks.id = id.into();
                    }
                    if let Some(model) = message["model"].as_str().filter(|m| !m.is_empty()) {
                        self.chunks.model = model.into();
                    }
                    out.push(self.delta(json!({"role":"assistant","content":""})));
                }
                self.usage.merge(&message["usage"]);
            }
            Some("content_block_start") => {
                let block = &event["content_block"];
                match block["type"].as_str() {
                    Some("tool_use") => {
                        let position = self.next_tool;
                        self.next_tool += 1;
                        self.tools.insert(index, position);
                        let arguments = block
                            .get("input")
                            .and_then(Value::as_object)
                            .filter(|input| !input.is_empty())
                            .map(|input| Value::Object(input.clone()).to_string())
                            .unwrap_or_default();
                        out.push(self.delta(json!({"tool_calls":[{"index":position,"id":block["id"],"type":"function",
                            "function":{"name":block["name"],"arguments":arguments}}]})));
                    }
                    Some("thinking") => {
                        if let Some(text) =
                            block["thinking"].as_str().filter(|text| !text.is_empty())
                        {
                            out.push(self.delta(json!({"reasoning_content":text})));
                        }
                        self.thinking.insert(
                            index,
                            (
                                block["thinking"].as_str().unwrap_or_default().to_owned(),
                                None,
                            ),
                        );
                    }
                    Some("redacted_thinking") => {
                        self.thinking.insert(
                            index,
                            (String::new(), block["data"].as_str().map(str::to_owned)),
                        );
                    }
                    Some("text") => {
                        if let Some(text) = block["text"].as_str().filter(|t| !t.is_empty()) {
                            out.push(self.delta(json!({"content":text})));
                        }
                    }
                    _ => {}
                }
            }
            Some("content_block_delta") => {
                let delta = &event["delta"];
                match delta["type"].as_str() {
                    Some("text_delta") => {
                        out.push(self.delta(json!({"content":delta["text"]})));
                    }
                    Some("thinking_delta") => {
                        let piece = delta["thinking"].as_str().unwrap_or_default();
                        if let Some((text, _)) = self.thinking.get_mut(&index) {
                            text.push_str(piece);
                        }
                        out.push(self.delta(json!({"reasoning_content":piece})));
                    }
                    Some("signature_delta") => {
                        if let Some((_, signature)) = self.thinking.get_mut(&index) {
                            *signature = delta["signature"].as_str().map(str::to_owned);
                        }
                    }
                    Some("input_json_delta") => {
                        if let Some(&position) = self.tools.get(&index) {
                            out.push(self.delta(json!({"tool_calls":[{"index":position,
                                "function":{"arguments":delta["partial_json"]}}]})));
                        }
                    }
                    _ => {}
                }
            }
            Some("content_block_stop") => {
                if let Some((text, signature)) = self.thinking.remove(&index)
                    && let Some(signature) = signature
                {
                    let mut item =
                        json!({"type":"reasoning","summary":[],"encrypted_content":signature});
                    if !text.is_empty() {
                        item["summary"] = json!([{"type":"summary_text","text":text}]);
                    }
                    out.push(self.delta(json!({"reasoning":[item]})));
                }
            }
            Some("message_delta") => {
                self.usage.merge(&event["usage"]);
                if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                    self.stop = Some(reason.to_owned());
                }
            }
            Some("message_stop") => self.end(&mut out),
            _ => {}
        }
        Ok(out)
    }
    fn finish(&mut self) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        if self.started && !self.finished {
            self.end(&mut out);
        }
        Ok(out)
    }
}

/// Messages callers read the wire's own events.
struct Native;
impl EventConversion for Native {
    fn event(&mut self, value: Value) -> Result<Vec<Value>> {
        Ok(vec![value])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events() -> Vec<Value> {
        vec![
            json!({"type":"message_start","message":{"id":"msg_1","model":"claude","usage":{"input_tokens":10,"cache_read_input_tokens":4,"cache_creation_input_tokens":1}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"why"}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Hello"}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"toolu_1","name":"get","input":{}}}),
            json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"a\":1}"}}),
            json!({"type":"content_block_stop","index":2}),
            json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":7}}),
            json!({"type":"message_stop"}),
        ]
    }

    #[test]
    fn messages_events_become_responses_items_with_combined_usage() {
        let mut conversion = conversion(Inbound::Responses, true, "claude");
        let mut out = Vec::new();
        for event in events() {
            out.extend(conversion.event(event).unwrap());
        }
        out.extend(conversion.finish().unwrap());
        assert_eq!(out[0]["type"], "response.created");
        assert_eq!(out[0]["response"]["id"], "msg_1");
        let completed = out.last().unwrap();
        assert_eq!(completed["type"], "response.completed");
        let output = completed["response"]["output"].as_array().unwrap();
        assert_eq!(output[0]["type"], "reasoning");
        assert_eq!(output[0]["encrypted_content"], "sig");
        assert_eq!(output[1]["content"][0]["text"], "Hello");
        assert_eq!(output[2]["call_id"], "toolu_1");
        assert_eq!(output[2]["arguments"], "{\"a\":1}");
        let usage = &completed["response"]["usage"];
        assert_eq!(usage["input_tokens"], 15);
        assert_eq!(usage["output_tokens"], 7);
        assert_eq!(usage["input_tokens_details"]["cached_tokens"], 4);
    }

    #[test]
    fn messages_events_become_chat_chunks_with_replayable_reasoning() {
        let mut conversion = conversion(Inbound::Chat, true, "claude");
        let mut out = Vec::new();
        for event in events() {
            out.extend(conversion.event(event).unwrap());
        }
        assert_eq!(out[0]["choices"][0]["delta"]["role"], "assistant");
        assert!(
            out.iter()
                .any(|c| c["choices"][0]["delta"]["reasoning_content"] == "why")
        );
        let reasoning = out
            .iter()
            .find(|c| c["choices"][0]["delta"]["reasoning"].is_array())
            .unwrap();
        assert_eq!(
            reasoning["choices"][0]["delta"]["reasoning"][0]["encrypted_content"],
            "sig"
        );
        assert!(
            out.iter()
                .any(|c| c["choices"][0]["delta"]["content"] == "Hello")
        );
        let call = out
            .iter()
            .find(|c| c["choices"][0]["delta"]["tool_calls"][0]["id"] == "toolu_1")
            .unwrap();
        assert_eq!(
            call["choices"][0]["delta"]["tool_calls"][0]["function"]["name"],
            "get"
        );
        let finish = out
            .iter()
            .find(|c| c["choices"][0]["finish_reason"].is_string())
            .unwrap();
        assert_eq!(finish["choices"][0]["finish_reason"], "tool_calls");
        let usage = out.last().unwrap();
        assert_eq!(usage["usage"]["prompt_tokens"], 15);
        assert_eq!(usage["usage"]["completion_tokens"], 7);
        let mut failing = super::conversion(Inbound::Chat, false, "claude");
        let error = failing
            .event(json!({"type":"error","error":{"type":"overloaded_error","message":"busy"}}))
            .unwrap();
        assert_eq!(error[0]["status"], 529);
    }
}
