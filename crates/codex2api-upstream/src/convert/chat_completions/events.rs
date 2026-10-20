//! Chat Completions chunks converted into the caller's protocol: Responses events,
//! Chat chunks or Messages events, each read directly from the chunks. A chunk's
//! `delta` carries text, reasoning text and indexed tool calls; the final chunk
//! carries the finish reason and, with usage enabled, the counts.
use crate::convert::EventConversion;
use crate::convert::messages_output::{Kind, Writer};
use crate::inbound::{Inbound, Result};
use serde_json::{Value, json};
use std::collections::HashMap;

/// The event conversion for a Chat Completions generation read by `inbound`.
pub(crate) fn conversion(
    inbound: Inbound,
    include_usage: bool,
    model: &str,
) -> Box<dyn EventConversion> {
    match inbound {
        Inbound::Chat => Box::new(ToChat {
            include_usage,
            finished: false,
        }),
        Inbound::Messages => Box::new(ToMessages::new(model.to_owned())),
        _ => Box::new(ToResponses::new(model.to_owned())),
    }
}

fn item_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

/// The first choice's delta (or a synchronous reply's message).
fn delta(chunk: &Value) -> Option<&Value> {
    let choice = chunk["choices"].as_array()?.first()?;
    choice.get("delta").or_else(|| choice.get("message"))
}

fn finish_reason(chunk: &Value) -> Option<&str> {
    chunk["choices"]
        .as_array()?
        .first()?
        .get("finish_reason")?
        .as_str()
}

fn reasoning_delta(delta: &Value) -> Option<&str> {
    delta["reasoning_content"]
        .as_str()
        .or_else(|| delta["reasoning"].as_str())
        .filter(|text| !text.is_empty())
}

fn text_delta(delta: &Value) -> Option<&str> {
    delta["content"]
        .as_str()
        .or_else(|| delta["refusal"].as_str())
        .filter(|text| !text.is_empty())
}

/// A chunk's error object, with the HTTP-like status compatible services add.
fn chunk_error(chunk: &Value) -> Option<Value> {
    let error = chunk.get("error").filter(|e| !e.is_null())?;
    let mut error = match error {
        Value::Object(_) => error.clone(),
        other => json!({"message":other.as_str().unwrap_or("The supplier request failed.")}),
    };
    if error.get("type").is_none() {
        error["type"] = "server_error".into();
    }
    Some(error)
}

fn error_status(error: &Value) -> u64 {
    error["status"]
        .as_u64()
        .or_else(|| error["code"].as_u64())
        .filter(|status| (400..=599).contains(status))
        .unwrap_or(502)
}

/// Chat usage in the Responses vocabulary.
fn responses_usage(usage: &Value) -> Value {
    let mut value = json!({});
    for (from, to) in [
        ("prompt_tokens", "input_tokens"),
        ("completion_tokens", "output_tokens"),
        ("total_tokens", "total_tokens"),
    ] {
        if let Some(count) = usage.get(from).filter(|v| v.is_number()) {
            value[to] = count.clone();
        }
    }
    if value.get("total_tokens").is_none()
        && let (Some(input), Some(output)) = (
            value["input_tokens"].as_u64(),
            value["output_tokens"].as_u64(),
        )
    {
        value["total_tokens"] = input.saturating_add(output).into();
    }
    if let Some(cached) = usage.pointer("/prompt_tokens_details/cached_tokens") {
        value["input_tokens_details"] = json!({"cached_tokens":cached});
    }
    if let Some(reasoning) = usage.pointer("/completion_tokens_details/reasoning_tokens") {
        value["output_tokens_details"] = json!({"reasoning_tokens":reasoning});
    }
    value
}

enum Item {
    Reasoning {
        id: String,
        text: String,
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
            Self::Reasoning { id, text } => {
                let mut item = json!({"type":"reasoning","id":id,"summary":[]});
                if !text.is_empty() {
                    item["summary"] = json!([{"type":"summary_text","text":text}]);
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

/// Responses callers: chunks as a Responses item stream.
struct ToResponses {
    id: String,
    model: String,
    created_at: i64,
    started: bool,
    finished: bool,
    items: Vec<Item>,
    open: Option<usize>,
    /// Tool call index of the chunk stream to the item it feeds.
    tools: HashMap<u64, usize>,
    usage: Option<Value>,
    finish: Option<String>,
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
            open: None,
            tools: HashMap::new(),
            usage: None,
            finish: None,
        }
    }
    fn response(&self, status: &str) -> Value {
        json!({"id":self.id,"object":"response","created_at":self.created_at,"status":status,"model":self.model})
    }
    fn index(&self) -> usize {
        self.items.len() - 1
    }
    fn open_item(&mut self, item: Item, out: &mut Vec<Value>) {
        self.close(out);
        let mut added = item.value();
        self.items.push(item);
        let index = self.index();
        if added["type"] != "function_call" {
            self.open = Some(index);
        }
        if added["type"] == "message" {
            added["status"] = "in_progress".into();
            added["content"] = json!([]);
            out.push(
                json!({"type":"response.output_item.added","output_index":index,"item":added}),
            );
            out.push(
                json!({"type":"response.content_part.added","item_id":self.items[index].id(),"output_index":index,"content_index":0,
                "part":{"type":"output_text","text":"","annotations":[]}}),
            );
        } else {
            if added["type"] == "function_call" {
                added["status"] = "in_progress".into();
                added["arguments"] = "".into();
            }
            out.push(
                json!({"type":"response.output_item.added","output_index":index,"item":added}),
            );
        }
    }
    fn close(&mut self, out: &mut Vec<Value>) {
        let Some(index) = self.open.take() else {
            return;
        };
        self.close_item(index, out);
    }
    fn close_tools(&mut self, out: &mut Vec<Value>) {
        let mut indices: Vec<_> = std::mem::take(&mut self.tools).into_values().collect();
        indices.sort_unstable();
        for index in indices {
            self.close_item(index, out);
        }
    }
    fn close_item(&self, index: usize, out: &mut Vec<Value>) {
        let item = &self.items[index];
        match item {
            Item::Reasoning { id, text, .. } => {
                if !text.is_empty() {
                    out.push(json!({"type":"response.reasoning_summary_text.done","item_id":id,"output_index":index,"summary_index":0,"text":text}));
                    out.push(json!({"type":"response.reasoning_summary_part.done","item_id":id,"output_index":index,"summary_index":0,"part":{"type":"summary_text","text":text}}));
                }
            }
            Item::Message { id, text, .. } => {
                out.push(json!({"type":"response.output_text.done","item_id":id,"output_index":index,"content_index":0,"text":text,"logprobs":[]}));
                out.push(json!({"type":"response.content_part.done","item_id":id,"output_index":index,"content_index":0,
                    "part":item.value()["content"][0]}));
            }
            Item::FunctionCall { id, arguments, .. } => {
                out.push(json!({"type":"response.function_call_arguments.done","item_id":id,"output_index":index,"arguments":arguments}));
            }
        }
        out.push(
            json!({"type":"response.output_item.done","output_index":index,"item":item.value()}),
        );
    }
    fn open_kind(&self, reasoning: bool) -> bool {
        self.open.is_some()
            && match self.items.last() {
                Some(Item::Reasoning { .. }) => reasoning,
                Some(Item::Message { .. }) => !reasoning,
                _ => false,
            }
    }
    fn reasoning(&mut self, text: &str, out: &mut Vec<Value>) {
        if text.is_empty() {
            return;
        }
        if !self.open_kind(true) {
            self.open_item(
                Item::Reasoning {
                    id: item_id("rs"),
                    text: String::new(),
                },
                out,
            );
        }
        let index = self.index();
        if let Some(Item::Reasoning { id, text: buffer }) = self.items.last_mut() {
            if buffer.is_empty() {
                out.push(json!({"type":"response.reasoning_summary_part.added","item_id":id,"output_index":index,"summary_index":0,"part":{"type":"summary_text","text":""}}));
            }
            buffer.push_str(text);
        }
        out.push(json!({"type":"response.reasoning_summary_text.delta","item_id":self.items[index].id(),"output_index":index,"summary_index":0,"delta":text}));
    }
    fn text(&mut self, text: &str, out: &mut Vec<Value>) {
        if !self.open_kind(false) {
            self.open_item(
                Item::Message {
                    id: item_id("msg"),
                    text: String::new(),
                },
                out,
            );
        }
        let index = self.index();
        if let Some(Item::Message { text: buffer, .. }) = self.items.last_mut() {
            buffer.push_str(text);
        }
        out.push(json!({"type":"response.output_text.delta","item_id":self.items[index].id(),"output_index":index,"content_index":0,"delta":text,"logprobs":[]}));
    }
    fn tool_call(&mut self, call: &Value, position: u64, out: &mut Vec<Value>) {
        let index = call["index"].as_u64().unwrap_or(position);
        let payload = call.get("function").or_else(|| call.get("custom"));
        let arguments = payload
            .and_then(|p| p["arguments"].as_str().or_else(|| p["input"].as_str()))
            .unwrap_or_default();
        let item = match self.tools.get(&index) {
            Some(&item) => item,
            None => {
                let name = payload
                    .and_then(|p| p["name"].as_str())
                    .unwrap_or_default()
                    .to_owned();
                let call_id = call["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map_or_else(|| item_id("call"), str::to_owned);
                self.open_item(
                    Item::FunctionCall {
                        id: item_id("fc"),
                        call_id,
                        name,
                        arguments: String::new(),
                    },
                    out,
                );
                let item = self.index();
                self.tools.insert(index, item);
                item
            }
        };
        if arguments.is_empty() {
            return;
        }
        if let Item::FunctionCall {
            arguments: buffer, ..
        } = &mut self.items[item]
        {
            buffer.push_str(arguments);
        }
        out.push(json!({"type":"response.function_call_arguments.delta","item_id":self.items[item].id(),"output_index":item,"delta":arguments}));
    }
    fn fail(&mut self, error: Value, mut out: Vec<Value>) -> Vec<Value> {
        self.close(&mut out);
        self.close_tools(&mut out);
        let mut response = self.response("failed");
        response["error"] = error;
        response["output"] = self.items.iter().map(Item::value).collect();
        if let Some(usage) = &self.usage {
            response["usage"] = responses_usage(usage);
        }
        out.push(json!({"type":"response.failed","response":response}));
        self.finished = true;
        out
    }
}
impl EventConversion for ToResponses {
    fn event(&mut self, chunk: Value) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        if self.finished {
            return Ok(out);
        }
        if !self.started {
            self.started = true;
            if let Some(id) = chunk["id"]
                .as_str()
                .filter(|id| !id.is_empty() && id.len() <= 128)
            {
                self.id = id.to_owned();
            }
            if let Some(model) = chunk["model"].as_str().filter(|m| !m.is_empty()) {
                self.model = model.to_owned();
            }
            out.push(json!({"type":"response.created","response":self.response("in_progress")}));
        }
        if let Some(usage) = chunk.get("usage").filter(|u| u.is_object()) {
            self.usage = Some(usage.clone());
        }
        if let Some(error) = chunk_error(&chunk) {
            return Ok(self.fail(error, out));
        }
        if let Some(delta) = delta(&chunk) {
            if let Some(text) = reasoning_delta(delta) {
                self.reasoning(text, &mut out);
            }
            if let Some(text) = text_delta(delta) {
                self.text(text, &mut out);
            }
            for (position, call) in delta["tool_calls"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                self.tool_call(call, position as u64, &mut out);
            }
        }
        if let Some(reason) = finish_reason(&chunk) {
            self.finish = Some(reason.to_owned());
            self.close(&mut out);
            self.close_tools(&mut out);
        }
        Ok(out)
    }
    fn finish(&mut self) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        if !self.started || self.finished {
            return Ok(out);
        }
        self.close(&mut out);
        self.close_tools(&mut out);
        let reason = match self.finish.as_deref() {
            Some(reason) => reason,
            // Some services end the stream without a finish reason once output exists.
            None if !self.items.is_empty() => "stop",
            None => {
                return Ok(self.fail(
                    json!({"code":"upstream_protocol_error","type":"server_error","message":"The upstream stream ended without a finish reason."}),
                    out,
                ));
            }
        };
        let (kind, status, incomplete) = match reason {
            "length" => (
                "response.incomplete",
                "incomplete",
                Some("max_output_tokens"),
            ),
            "content_filter" => ("response.incomplete", "incomplete", Some("content_filter")),
            _ => ("response.completed", "completed", None),
        };
        let mut response = self.response(status);
        response["output"] = self.items.iter().map(Item::value).collect();
        if let Some(reason) = incomplete {
            response["incomplete_details"] = json!({"reason":reason});
        }
        if let Some(usage) = &self.usage {
            response["usage"] = responses_usage(usage);
        }
        out.push(json!({"type":kind,"response":response}));
        self.finished = true;
        Ok(out)
    }
}

/// Chat callers: the wire's own chunks, with provider spellings of reasoning text
/// unified and usage chunks the caller did not ask for dropped.
struct ToChat {
    include_usage: bool,
    finished: bool,
}
impl EventConversion for ToChat {
    fn event(&mut self, mut chunk: Value) -> Result<Vec<Value>> {
        if self.finished {
            return Ok(Vec::new());
        }
        if let Some(error) = chunk_error(&chunk) {
            self.finished = true;
            let status = error_status(&error);
            return Ok(vec![json!({"type":"error","status":status,"error":error})]);
        }
        let Some(object) = chunk.as_object_mut() else {
            return Ok(Vec::new());
        };
        object.insert("object".into(), "chat.completion.chunk".into());
        let has_choices = object["choices"]
            .as_array()
            .is_some_and(|choices| !choices.is_empty());
        if !self.include_usage {
            if !has_choices {
                return Ok(Vec::new());
            }
            object.remove("usage");
        }
        for choice in object
            .get_mut("choices")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            if let Some(delta) = choice.get_mut("delta").and_then(Value::as_object_mut)
                && delta.get("reasoning").is_some_and(Value::is_string)
            {
                let reasoning = delta.remove("reasoning").expect("reasoning string");
                delta.entry("reasoning_content").or_insert(reasoning);
            }
        }
        Ok(vec![chunk])
    }
}

/// Messages callers: chunks as ordered Messages content blocks.
struct ToMessages {
    writer: Writer,
    model: String,
    finished: bool,
    sequence: u64,
    current: Option<(u64, u64)>,
    /// Tool call index of the chunk stream to its block key.
    tools: HashMap<u64, (u64, u64)>,
    kinds: HashMap<(u64, u64), Kind>,
    usage: Option<Value>,
    finish: Option<String>,
}
impl ToMessages {
    fn new(model: String) -> Self {
        Self {
            writer: Writer::default(),
            model,
            finished: false,
            sequence: 0,
            current: None,
            tools: HashMap::new(),
            kinds: HashMap::new(),
            usage: None,
            finish: None,
        }
    }
    fn close_current(&mut self, out: &mut Vec<Value>) {
        if let Some(key) = self.current.take() {
            self.writer.close(key, out);
        }
    }
    fn block(&mut self, kind: Kind, out: &mut Vec<Value>) -> (u64, u64) {
        if let Some(key) = self.current
            && self.writer.has(key)
            && self.kind_of(key) == Some(kind)
        {
            return key;
        }
        self.close_current(out);
        self.sequence += 1;
        let key = (0, self.sequence);
        let start = match kind {
            Kind::Text => json!({"type":"text","text":""}),
            Kind::Thinking => json!({"type":"thinking","thinking":""}),
            Kind::Tool => unreachable!("tool blocks open by index"),
        };
        self.writer.ensure(key, kind, start, out);
        self.current = Some(key);
        self.kinds.insert(key, kind);
        key
    }
    fn kind_of(&self, key: (u64, u64)) -> Option<Kind> {
        self.kinds.get(&key).copied()
    }
    fn stop_reason(&self) -> &'static str {
        match self.finish.as_deref() {
            Some("tool_calls" | "function_call") => "tool_use",
            Some("length") => "max_tokens",
            Some("content_filter") => "refusal",
            _ => {
                if self.writer.tool_use() {
                    "tool_use"
                } else {
                    "end_turn"
                }
            }
        }
    }
    fn usage(&self) -> Value {
        let usage = self.usage.as_ref();
        let prompt = usage.and_then(|u| u["prompt_tokens"].as_u64());
        let cached = usage
            .and_then(|u| u.pointer("/prompt_tokens_details/cached_tokens")?.as_u64())
            .unwrap_or(0);
        let mut value = json!({
            "input_tokens": prompt.map(|n| n.saturating_sub(cached)).unwrap_or(0),
            "output_tokens": usage.and_then(|u| u["completion_tokens"].as_u64()).unwrap_or(0),
        });
        if cached > 0 {
            value["cache_read_input_tokens"] = cached.into();
        }
        value
    }
    fn fail(&mut self, error: Value) -> Vec<Value> {
        self.finished = true;
        let status = http::StatusCode::from_u16(error_status(&error) as u16)
            .unwrap_or(http::StatusCode::BAD_GATEWAY);
        vec![crate::inbound::messages::error(
            status,
            &json!({"error":error}),
        )]
    }
}
impl EventConversion for ToMessages {
    fn event(&mut self, chunk: Value) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        if self.finished {
            return Ok(out);
        }
        if let Some(usage) = chunk.get("usage").filter(|u| u.is_object()) {
            self.usage = Some(usage.clone());
        }
        if let Some(error) = chunk_error(&chunk) {
            return Ok(self.fail(error));
        }
        if !self.writer.started() {
            let model = chunk["model"]
                .as_str()
                .filter(|m| !m.is_empty())
                .unwrap_or(&self.model)
                .to_owned();
            self.model = model;
            out.extend(self.writer.start(&chunk["id"], &json!(self.model)));
        }
        if let Some(delta) = delta(&chunk) {
            if let Some(text) = reasoning_delta(delta) {
                let key = self.block(Kind::Thinking, &mut out);
                self.writer.append(key, text, &mut out)?;
            }
            if let Some(text) = text_delta(delta) {
                let key = self.block(Kind::Text, &mut out);
                self.writer.append(key, text, &mut out)?;
            }
            for (position, call) in delta["tool_calls"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                let index = call["index"].as_u64().unwrap_or(position as u64);
                let payload = call.get("function").or_else(|| call.get("custom"));
                let key = match self.tools.get(&index) {
                    Some(key) => *key,
                    None => {
                        self.close_current(&mut out);
                        let key = (1, index);
                        let id = call["id"]
                            .as_str()
                            .filter(|id| !id.is_empty())
                            .map_or_else(|| item_id("toolu"), str::to_owned);
                        let name = payload.and_then(|p| p["name"].as_str()).unwrap_or_default();
                        self.writer.ensure(
                            key,
                            Kind::Tool,
                            json!({"type":"tool_use","id":id,"name":name,"input":{}}),
                            &mut out,
                        );
                        self.tools.insert(index, key);
                        self.kinds.insert(key, Kind::Tool);
                        key
                    }
                };
                if let Some(arguments) = payload
                    .and_then(|p| p["arguments"].as_str().or_else(|| p["input"].as_str()))
                    .filter(|a| !a.is_empty())
                {
                    self.writer.append(key, arguments, &mut out)?;
                }
            }
        }
        if let Some(reason) = finish_reason(&chunk) {
            self.finish = Some(reason.to_owned());
            self.close_current(&mut out);
            // No argument fragments follow the wire's finish reason. Release
            // queued parallel blocks now, without waiting for the usage tail.
            let mut tools: Vec<_> = std::mem::take(&mut self.tools).into_values().collect();
            tools.sort_unstable();
            for key in tools {
                self.writer.close(key, &mut out);
            }
        }
        Ok(out)
    }
    fn finish(&mut self) -> Result<Vec<Value>> {
        if self.finished || !self.writer.started() {
            return Ok(Vec::new());
        }
        self.finished = true;
        let mut out = Vec::new();
        self.close_current(&mut out);
        let usage = self.usage();
        let stop = self.stop_reason();
        out.extend(self.writer.finish(stop, usage));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunks() -> Vec<Value> {
        vec![
            json!({"id":"chatcmpl_1","model":"deepseek","choices":[{"index":0,"delta":{"role":"assistant","reasoning_content":"think"},"finish_reason":null}]}),
            json!({"id":"chatcmpl_1","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}),
            json!({"id":"chatcmpl_1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"get","arguments":"{\"a\""}}]},"finish_reason":null}]}),
            json!({"id":"chatcmpl_1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":":1}"}}]},"finish_reason":"tool_calls"}]}),
            json!({"id":"chatcmpl_1","choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5,"prompt_tokens_details":{"cached_tokens":4}}}),
        ]
    }

    #[test]
    fn chunks_become_responses_items_with_usage() {
        let mut conversion = conversion(Inbound::Responses, true, "deepseek");
        let mut events = Vec::new();
        for chunk in chunks() {
            events.extend(conversion.event(chunk).unwrap());
        }
        events.extend(conversion.finish().unwrap());
        assert_eq!(events[0]["type"], "response.created");
        assert_eq!(events[0]["response"]["id"], "chatcmpl_1");
        let completed = events.last().unwrap();
        assert_eq!(completed["type"], "response.completed");
        let output = completed["response"]["output"].as_array().unwrap();
        assert_eq!(output[0]["type"], "reasoning");
        assert_eq!(output[1]["content"][0]["text"], "Hello");
        assert_eq!(output[2]["type"], "function_call");
        assert_eq!(output[2]["arguments"], "{\"a\":1}");
        assert_eq!(output[2]["call_id"], "call_1");
        assert_eq!(completed["response"]["usage"]["input_tokens"], 10);
        assert_eq!(
            completed["response"]["usage"]["input_tokens_details"]["cached_tokens"],
            4
        );
        assert!(
            events
                .iter()
                .any(|e| e["type"] == "response.function_call_arguments.delta")
        );
    }

    #[test]
    fn chunks_become_messages_blocks_and_chat_chunks_keep_their_shape() {
        let mut conversion = conversion(Inbound::Messages, true, "deepseek");
        let mut events = Vec::new();
        for chunk in chunks() {
            events.extend(conversion.event(chunk).unwrap());
        }
        events.extend(conversion.finish().unwrap());
        assert_eq!(events[0]["type"], "message_start");
        let starts: Vec<_> = events
            .iter()
            .filter(|e| e["type"] == "content_block_start")
            .map(|e| e["content_block"]["type"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(starts, ["thinking", "text", "tool_use"]);
        let delta = events
            .iter()
            .find(|e| e["type"] == "message_delta")
            .unwrap();
        assert_eq!(delta["delta"]["stop_reason"], "tool_use");
        assert_eq!(delta["usage"]["input_tokens"], 6);
        assert_eq!(delta["usage"]["cache_read_input_tokens"], 4);
        assert_eq!(events.last().unwrap()["type"], "message_stop");
        let mut native = super::conversion(Inbound::Chat, false, "deepseek");
        let mut passed = Vec::new();
        for chunk in chunks() {
            passed.extend(native.event(chunk).unwrap());
        }
        assert_eq!(passed.len(), 4, "the usage-only chunk is dropped");
        assert_eq!(
            passed[0]["choices"][0]["delta"]["reasoning_content"],
            "think"
        );
        let error = native
            .event(json!({"error":{"message":"boom","code":"server_error"}}))
            .unwrap();
        assert_eq!(error[0]["type"], "error");
        assert_eq!(error[0]["status"], 502);
    }
}
