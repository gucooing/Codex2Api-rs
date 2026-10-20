//! Gemini streamGenerateContent chunks converted into the caller's protocol: Responses
//! events, Chat chunks or Messages events, each read directly from the chunks.
use crate::convert::EventConversion;
use crate::convert::messages_output::{Kind, Writer};
use crate::convert::responses::chat_output::Chunks;
use crate::inbound::{Inbound, Result};
use crate::providers::antigravity::errors;
use serde_json::{Value, json};

enum Item {
    Reasoning {
        id: String,
        text: String,
        signature: Option<String>,
    },
    Message {
        id: String,
        text: String,
        annotations: Vec<Value>,
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
            Self::Message {
                id,
                text,
                annotations,
            } => json!({"type":"message","id":id,"role":"assistant","status":"completed",
                "content":[{"type":"output_text","text":text,"annotations":annotations}]}),
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

fn item_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

/// Gemini counts cached prompt tokens inside the prompt total and thoughts outside the
/// candidate total; Responses includes both in its totals and details them separately.
fn usage(meta: &Value) -> Value {
    let input = meta["promptTokenCount"].as_u64();
    let thoughts = meta["thoughtsTokenCount"].as_u64();
    let output = meta["candidatesTokenCount"]
        .as_u64()
        .and_then(|count| count.checked_add(thoughts.unwrap_or(0)));
    let total = meta["totalTokenCount"]
        .as_u64()
        .or_else(|| input?.checked_add(output?));
    let mut value = json!({});
    for (key, count) in [
        ("input_tokens", input),
        ("output_tokens", output),
        ("total_tokens", total),
    ] {
        if let Some(count) = count {
            value[key] = count.into();
        }
    }
    if let Some(cached) = meta["cachedContentTokenCount"].as_u64() {
        value["input_tokens_details"] = json!({"cached_tokens":cached});
    }
    if let Some(thoughts) = thoughts {
        value["output_tokens_details"] = json!({"reasoning_tokens":thoughts});
    }
    value
}

pub(crate) struct Converter {
    id: String,
    model: String,
    created_at: i64,
    started: bool,
    pub(crate) finished: bool,
    items: Vec<Item>,
    open: bool,
    usage_metadata: Option<Value>,
    finish: Option<String>,
    block: Option<String>,
    pending_annotations: Vec<Value>,
}

impl Converter {
    pub(crate) fn new(model: String) -> Self {
        Self {
            id: item_id("resp"),
            model,
            created_at: chrono::Utc::now().timestamp(),
            started: false,
            finished: false,
            items: Vec::new(),
            open: false,
            usage_metadata: None,
            finish: None,
            block: None,
            pending_annotations: Vec::new(),
        }
    }

    fn response(&self, status: &str) -> Value {
        json!({"id":self.id,"object":"response","created_at":self.created_at,"status":status,"model":self.model})
    }

    fn start(&mut self, chunk: &Value, body: &Value, out: &mut Vec<Value>) {
        if self.started {
            return;
        }
        self.started = true;
        if let Some(id) = body["responseId"]
            .as_str()
            .or_else(|| chunk["responseId"].as_str())
            .filter(|id| !id.is_empty() && id.len() <= 128)
        {
            self.id = id.to_owned();
        }
        out.push(json!({"type":"response.created","response":self.response("in_progress")}));
    }

    fn index(&self) -> usize {
        self.items.len() - 1
    }

    fn open_item(&mut self, item: Item, out: &mut Vec<Value>) {
        self.close(out);
        let value = item.value();
        self.items.push(item);
        self.open = true;
        let index = self.index();
        let mut added = value;
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
            for annotation in std::mem::take(&mut self.pending_annotations) {
                self.annotate(annotation, out);
            }
        } else {
            if added["type"] == "function_call" {
                added["status"] = "in_progress".into();
                // The following arguments delta carries the payload once.
                added["arguments"] = "".into();
            }
            out.push(
                json!({"type":"response.output_item.added","output_index":index,"item":added}),
            );
        }
    }

    fn close(&mut self, out: &mut Vec<Value>) {
        if !self.open {
            return;
        }
        self.open = false;
        let index = self.index();
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

    fn open_reasoning(&self) -> bool {
        self.open && matches!(self.items.last(), Some(Item::Reasoning { .. }))
    }

    fn open_message(&self) -> bool {
        self.open && matches!(self.items.last(), Some(Item::Message { .. }))
    }

    /// Keep a non-thought signature separate from preceding thought text. The
    /// empty item binds it to the following part when the caller replays history.
    fn carry_signature(&mut self, signature: Option<String>, out: &mut Vec<Value>) {
        let Some(signature) = signature else {
            return;
        };
        self.open_item(
            Item::Reasoning {
                id: item_id("rs"),
                text: String::new(),
                signature: Some(signature),
            },
            out,
        );
        self.close(out);
    }

    fn text(&mut self, text: &str, out: &mut Vec<Value>) {
        if text.is_empty() {
            return;
        }
        if !self.open_message() {
            self.open_item(
                Item::Message {
                    id: item_id("msg"),
                    text: String::new(),
                    annotations: Vec::new(),
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

    fn annotate(&mut self, mut annotation: Value, out: &mut Vec<Value>) {
        let index = self.index();
        let Some(Item::Message {
            id,
            text,
            annotations,
        }) = self.items.last_mut()
        else {
            return;
        };
        let offset = text.chars().count();
        annotation["start_index"] = offset.into();
        annotation["end_index"] = offset.into();
        let position = annotations.len();
        annotations.push(annotation.clone());
        out.push(json!({"type":"response.output_text.annotation.added","item_id":id,"output_index":index,"content_index":0,
            "annotation_index":position,"annotation":annotation}));
    }

    fn part(&mut self, part: &Value, out: &mut Vec<Value>) {
        let signature = part["thoughtSignature"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        if let Some(call) = part.get("functionCall").filter(|c| c.is_object()) {
            self.carry_signature(signature, out);
            let arguments = call
                .get("args")
                .filter(|a| !a.is_null())
                .map_or_else(|| "{}".into(), Value::to_string);
            let call_id = call["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map_or_else(|| item_id("call"), str::to_owned);
            self.open_item(
                Item::FunctionCall {
                    id: item_id("fc"),
                    call_id,
                    name: call["name"].as_str().unwrap_or_default().to_owned(),
                    arguments: arguments.clone(),
                },
                out,
            );
            let index = self.index();
            out.push(json!({"type":"response.function_call_arguments.delta","item_id":self.items[index].id(),"output_index":index,"delta":arguments}));
            self.close(out);
            return;
        }
        if part["thought"] == true {
            if !self.open_reasoning() {
                self.open_item(
                    Item::Reasoning {
                        id: item_id("rs"),
                        text: String::new(),
                        signature: None,
                    },
                    out,
                );
            }
            let index = self.index();
            if let Some(Item::Reasoning {
                id,
                text: buffer,
                signature: slot,
            }) = self.items.last_mut()
            {
                if let Some(text) = part["text"].as_str().filter(|t| !t.is_empty()) {
                    if buffer.is_empty() {
                        out.push(json!({"type":"response.reasoning_summary_part.added","item_id":id,"output_index":index,"summary_index":0,"part":{"type":"summary_text","text":""}}));
                    }
                    buffer.push_str(text);
                    out.push(json!({"type":"response.reasoning_summary_text.delta","item_id":id,"output_index":index,"summary_index":0,"delta":text}));
                }
                if signature.is_some() {
                    *slot = signature;
                    self.close(out);
                }
            }
            return;
        }
        self.carry_signature(signature, out);
        let text = if let Some(text) = part["text"].as_str() {
            text.to_owned()
        } else if let Some(data) = part["inlineData"]["data"].as_str() {
            format!(
                "![image](data:{};base64,{data})",
                part["inlineData"]["mimeType"]
                    .as_str()
                    .unwrap_or("image/png")
            )
        } else if let Some(code) = part["executableCode"]["code"].as_str() {
            format!(
                "\n```{}\n{code}\n```\n",
                part["executableCode"]["language"]
                    .as_str()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
            )
        } else if let Some(output) = part["codeExecutionResult"]["output"].as_str() {
            format!("\n```\n{output}\n```\n")
        } else {
            return;
        };
        self.text(&text, out);
    }

    fn grounding(&mut self, metadata: &Value, out: &mut Vec<Value>) {
        for chunk in metadata["groundingChunks"].as_array().into_iter().flatten() {
            let Some(url) = chunk["web"]["uri"].as_str().filter(|u| !u.is_empty()) else {
                continue;
            };
            let annotation = json!({"type":"url_citation","url":url,"title":chunk["web"]["title"].as_str().unwrap_or(url)});
            if self.open_message() {
                self.annotate(annotation, out);
            } else {
                self.pending_annotations.push(annotation);
            }
        }
    }

    fn fail(&mut self, error: Value, mut out: Vec<Value>) -> Vec<Value> {
        self.close(&mut out);
        let mut response = self.response("failed");
        response["error"] = error;
        response["output"] = self.items.iter().map(Item::value).collect();
        if let Some(meta) = &self.usage_metadata {
            let counts = usage(meta);
            if counts.as_object().is_some_and(|counts| !counts.is_empty()) {
                response["usage"] = counts;
            }
        }
        out.push(json!({"type":"response.failed","response":response}));
        self.finished = true;
        out
    }

    /// One Gemini chunk; an in-stream error ends the generation as a failure.
    pub(crate) fn push(&mut self, chunk: Value) -> Vec<Value> {
        let mut out = Vec::new();
        if self.finished {
            return out;
        }
        let body = chunk
            .get("response")
            .filter(|v| v.is_object())
            .unwrap_or(&chunk);
        self.start(&chunk, body, &mut out);
        if let Some(meta) = body.get("usageMetadata").and_then(Value::as_object) {
            // Counters may arrive in separate chunks. Merge the raw cumulative
            // measurements before adding candidate and reasoning output tokens.
            self.usage_metadata
                .get_or_insert_with(|| json!({}))
                .as_object_mut()
                .expect("usage object")
                .extend(meta.clone());
        }
        if let Some(error) = chunk
            .get("error")
            .or_else(|| body.get("error"))
            .filter(|e| !e.is_null())
        {
            let status = error["code"]
                .as_u64()
                .and_then(|code| u16::try_from(code).ok())
                .filter(|code| (400..=599).contains(code))
                .unwrap_or(502);
            let mut normalized =
                errors::normalize(status, json!({"error":error}).to_string().as_bytes());
            normalized["error"]["model"] = self.model.as_str().into();
            if let Some(headers) = normalized.get("headers").cloned() {
                normalized["error"]["headers"] = headers;
            }
            return self.fail(normalized["error"].clone(), out);
        }
        if let Some(reason) = body["promptFeedback"]["blockReason"].as_str() {
            self.block = Some(reason.to_owned());
        }
        if let Some(candidate) = body["candidates"].as_array().and_then(|c| c.first()) {
            for part in candidate["content"]["parts"]
                .as_array()
                .into_iter()
                .flatten()
            {
                self.part(part, &mut out);
            }
            self.grounding(&candidate["groundingMetadata"], &mut out);
            if let Some(reason) = candidate["finishReason"].as_str().filter(|r| !r.is_empty()) {
                self.finish = Some(reason.to_owned());
            }
        }
        out
    }

    /// Finish once, retaining reported usage even when the stream is incomplete.
    pub(crate) fn finish(&mut self) -> Vec<Value> {
        let mut out = Vec::new();
        if !self.started || self.finished {
            return out;
        }
        self.close(&mut out);
        if let Some(reason) = self.block.take() {
            return self.fail(
                json!({"code":"prompt_blocked","type":"invalid_request_error","message":format!("The prompt was blocked: {reason}")}),
                out,
            );
        }
        let Some(reason) = self.finish.clone() else {
            return self.fail(
                json!({"code":"upstream_protocol_error","type":"server_error","message":"The upstream stream ended without a finish reason."}),
                out,
            );
        };
        let (kind, status, incomplete) = match reason.as_str() {
            "STOP" => ("response.completed", "completed", None),
            "MAX_TOKENS" => (
                "response.incomplete",
                "incomplete",
                Some("max_output_tokens"),
            ),
            "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII"
            | "IMAGE_SAFETY" | "LANGUAGE" => {
                ("response.incomplete", "incomplete", Some("content_filter"))
            }
            "MALFORMED_FUNCTION_CALL" => {
                return self.fail(
                    json!({"code":"malformed_function_call","type":"server_error","message":"The model produced a malformed function call."}),
                    out,
                );
            }
            _ => {
                return self.fail(
                    json!({"code":"upstream_protocol_error","type":"server_error","message":format!("The model stopped with an unsupported finish reason: {reason}")}),
                    out,
                );
            }
        };
        let mut response = self.response(status);
        response["output"] = self.items.iter().map(Item::value).collect();
        if let Some(reason) = incomplete {
            response["incomplete_details"] = json!({"reason":reason});
        }
        if let Some(meta) = &self.usage_metadata {
            let counts = usage(meta);
            if counts.as_object().is_some_and(|counts| !counts.is_empty()) {
                response["usage"] = counts;
            }
        }
        out.push(json!({"type":kind,"response":response}));
        self.finished = true;
        out
    }
}

/// The event conversion for a Gemini generation read by `inbound`.
pub(crate) fn conversion(
    inbound: Inbound,
    include_usage: bool,
    model: &str,
) -> Box<dyn EventConversion> {
    match inbound {
        Inbound::Chat => Box::new(ToChat::new(model.to_owned(), include_usage)),
        Inbound::Messages => Box::new(ToMessages::new(model.to_owned())),
        _ => Box::new(ToResponses(Converter::new(model.to_owned()))),
    }
}

/// Responses callers: the chunk-to-item state machine above.
struct ToResponses(Converter);
impl EventConversion for ToResponses {
    fn event(&mut self, value: Value) -> Result<Vec<Value>> {
        Ok(self.0.push(value))
    }
    fn finish(&mut self) -> Result<Vec<Value>> {
        Ok(self.0.finish())
    }
}

/// A Gemini chunk's error, normalized once, with the routed model.
fn chunk_error(chunk: &Value, body: &Value, model: &str) -> Option<Value> {
    let error = chunk
        .get("error")
        .or_else(|| body.get("error"))
        .filter(|e| !e.is_null())?;
    let status = error["code"]
        .as_u64()
        .and_then(|code| u16::try_from(code).ok())
        .filter(|code| (400..=599).contains(code))
        .unwrap_or(502);
    let mut normalized = errors::normalize(status, json!({"error":error}).to_string().as_bytes());
    normalized["error"]["model"] = model.into();
    if let Some(headers) = normalized.get("headers").cloned() {
        normalized["error"]["headers"] = headers;
    }
    Some(normalized["error"].clone())
}

/// Text of a non-thought part: text, an inline image as Markdown, or executed code.
fn part_text(part: &Value) -> Option<String> {
    if let Some(text) = part["text"].as_str() {
        Some(text.to_owned())
    } else if let Some(data) = part["inlineData"]["data"].as_str() {
        Some(format!(
            "![image](data:{};base64,{data})",
            part["inlineData"]["mimeType"]
                .as_str()
                .unwrap_or("image/png")
        ))
    } else if let Some(code) = part["executableCode"]["code"].as_str() {
        Some(format!(
            "\n```{}\n{code}\n```\n",
            part["executableCode"]["language"]
                .as_str()
                .unwrap_or_default()
                .to_ascii_lowercase()
        ))
    } else {
        part["codeExecutionResult"]["output"]
            .as_str()
            .map(|output| format!("\n```\n{output}\n```\n"))
    }
}

/// Chat finish reason for a Gemini finish reason; `None` means a failure.
fn chat_finish(reason: &str, tools: bool) -> Option<&'static str> {
    Some(match reason {
        "STOP" => {
            if tools {
                "tool_calls"
            } else {
                "stop"
            }
        }
        "MAX_TOKENS" => "length",
        "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" | "IMAGE_SAFETY"
        | "LANGUAGE" => "content_filter",
        _ => return None,
    })
}

fn chat_usage(meta: &Value) -> Value {
    let counts = usage(meta);
    let mut value = json!({});
    for (from, to) in [
        ("input_tokens", "prompt_tokens"),
        ("output_tokens", "completion_tokens"),
        ("total_tokens", "total_tokens"),
    ] {
        if let Some(count) = counts.get(from) {
            value[to] = count.clone();
        }
    }
    if let Some(cached) = counts.pointer("/input_tokens_details/cached_tokens") {
        value["prompt_tokens_details"] = json!({"cached_tokens":cached});
    }
    if let Some(reasoning) = counts.pointer("/output_tokens_details/reasoning_tokens") {
        value["completion_tokens_details"] = json!({"reasoning_tokens":reasoning});
    }
    value
}

/// Chat callers: Gemini parts as Chat chunk deltas.
struct ToChat {
    chunks: Chunks,
    model: String,
    started: bool,
    finished: bool,
    tools: usize,
    reasoning: String,
    signature: Option<String>,
    text_len: usize,
    annotations: usize,
    usage_metadata: Option<Value>,
    finish: Option<String>,
    block: Option<String>,
}
impl ToChat {
    fn new(model: String, include_usage: bool) -> Self {
        Self {
            chunks: Chunks {
                id: format!("chatcmpl_{}", uuid::Uuid::new_v4().simple()).into(),
                model: model.clone().into(),
                created: chrono::Utc::now().timestamp().into(),
                service_tier: None,
                include_usage,
            },
            model,
            started: false,
            finished: false,
            tools: 0,
            reasoning: String::new(),
            signature: None,
            text_len: 0,
            annotations: 0,
            usage_metadata: None,
            finish: None,
            block: None,
        }
    }
    fn delta(&self, delta: Value) -> Value {
        self.chunks.chunk(delta, Value::Null)
    }
    /// A reasoning segment closes when any other part follows; its signature travels
    /// in the lossless `reasoning` extension so a later turn can replay it.
    fn close_reasoning(&mut self, out: &mut Vec<Value>) {
        if self.reasoning.is_empty() && self.signature.is_none() {
            return;
        }
        let mut item = json!({"type":"reasoning","summary":[]});
        if !self.reasoning.is_empty() {
            item["summary"] = json!([{"type":"summary_text","text":self.reasoning}]);
        }
        if let Some(signature) = self.signature.take() {
            item["encrypted_content"] = signature.into();
        }
        self.reasoning.clear();
        out.push(self.delta(json!({"reasoning":[item]})));
    }
    fn fail(&mut self, error: Value, out: &mut Vec<Value>) {
        self.close_reasoning(out);
        let status = error["status"].as_u64().unwrap_or(502);
        out.push(json!({"type":"error","status":status,"error":error}));
        self.finished = true;
    }
    fn end(&mut self, out: &mut Vec<Value>) {
        self.close_reasoning(out);
        let reason = match self.finish.as_deref() {
            Some(reason) => reason,
            None => {
                let error = json!({"code":"upstream_protocol_error","type":"server_error","message":"The upstream stream ended without a finish reason."});
                self.fail(error, out);
                return;
            }
        };
        let Some(finish) = chat_finish(reason, self.tools > 0) else {
            let error = json!({"code":reason.to_ascii_lowercase(),"type":"server_error","message":format!("The model stopped with finish reason {reason}")});
            self.fail(error, out);
            return;
        };
        out.push(self.chunks.chunk(json!({}), finish.into()));
        if let Some(meta) = &self.usage_metadata
            && let Some(chunk) = self.chunks.usage(chat_usage(meta))
        {
            out.push(chunk);
        }
        self.finished = true;
    }
}
impl EventConversion for ToChat {
    fn event(&mut self, chunk: Value) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        if self.finished {
            return Ok(out);
        }
        let body = chunk
            .get("response")
            .filter(|v| v.is_object())
            .unwrap_or(&chunk)
            .clone();
        if !self.started {
            self.started = true;
            if let Some(id) = body["responseId"]
                .as_str()
                .or_else(|| chunk["responseId"].as_str())
                .filter(|id| !id.is_empty() && id.len() <= 128)
            {
                self.chunks.id = id.into();
            }
            if let Some(model) = body["modelVersion"].as_str() {
                self.chunks.model = model.into();
            }
            out.push(self.delta(json!({"role":"assistant","content":""})));
        }
        if let Some(meta) = body.get("usageMetadata").and_then(Value::as_object) {
            self.usage_metadata
                .get_or_insert_with(|| json!({}))
                .as_object_mut()
                .expect("usage object")
                .extend(meta.clone());
        }
        if let Some(error) = chunk_error(&chunk, &body, &self.model) {
            self.fail(error, &mut out);
            return Ok(out);
        }
        if let Some(reason) = body["promptFeedback"]["blockReason"].as_str() {
            self.block = Some(reason.to_owned());
        }
        if let Some(candidate) = body["candidates"].as_array().and_then(|c| c.first()) {
            for part in candidate["content"]["parts"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let signature = part["thoughtSignature"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned);
                if let Some(call) = part.get("functionCall").filter(|c| c.is_object()) {
                    self.close_reasoning(&mut out);
                    let arguments = call
                        .get("args")
                        .filter(|a| !a.is_null())
                        .map_or_else(|| "{}".into(), Value::to_string);
                    let id = call["id"].as_str().filter(|s| !s.is_empty()).map_or_else(
                        || format!("call_{}", uuid::Uuid::new_v4().simple()),
                        str::to_owned,
                    );
                    let index = self.tools;
                    self.tools += 1;
                    let mut tool = json!({"index":index,"id":id,"type":"function",
                        "function":{"name":call["name"].as_str().unwrap_or_default(),"arguments":arguments}});
                    if let Some(signature) = signature {
                        tool["extra_content"] = json!({"google":{"thought_signature":signature}});
                    }
                    out.push(self.delta(json!({"tool_calls":[tool]})));
                    continue;
                }
                if part["thought"] == true {
                    if let Some(text) = part["text"].as_str().filter(|t| !t.is_empty()) {
                        self.reasoning.push_str(text);
                        out.push(self.delta(json!({"reasoning_content":text})));
                    }
                    if signature.is_some() {
                        self.signature = signature;
                        self.close_reasoning(&mut out);
                    }
                    continue;
                }
                self.close_reasoning(&mut out);
                if signature.is_some() {
                    self.signature = signature;
                }
                self.close_reasoning(&mut out);
                if let Some(text) = part_text(part).filter(|t| !t.is_empty()) {
                    self.text_len += text.chars().count();
                    out.push(self.delta(json!({"content":text})));
                }
            }
            for grounding in candidate["groundingMetadata"]["groundingChunks"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let Some(url) = grounding["web"]["uri"].as_str().filter(|u| !u.is_empty()) else {
                    continue;
                };
                self.annotations += 1;
                out.push(self.delta(
                    json!({"annotations":[{"type":"url_citation","url_citation":{
                    "url":url,"title":grounding["web"]["title"].as_str().unwrap_or(url),
                    "start_index":self.text_len,"end_index":self.text_len}}]}),
                ));
            }
            if let Some(reason) = candidate["finishReason"].as_str().filter(|r| !r.is_empty()) {
                self.finish = Some(reason.to_owned());
            }
        }
        Ok(out)
    }
    fn finish(&mut self) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        if !self.started || self.finished {
            return Ok(out);
        }
        if let Some(reason) = self.block.take() {
            let error = json!({"code":"prompt_blocked","type":"invalid_request_error","status":400,"message":format!("The prompt was blocked: {reason}")});
            self.fail(error, &mut out);
            return Ok(out);
        }
        self.end(&mut out);
        Ok(out)
    }
}

/// Messages callers: Gemini parts as Messages content blocks, one open at a time.
struct ToMessages {
    writer: Writer,
    model: String,
    next: u64,
    open: Option<(u64, Kind)>,
    finished: bool,
    usage_metadata: Option<Value>,
    finish: Option<String>,
    block: Option<String>,
}
impl ToMessages {
    fn new(model: String) -> Self {
        Self {
            writer: Writer::default(),
            model,
            next: 0,
            open: None,
            finished: false,
            usage_metadata: None,
            finish: None,
            block: None,
        }
    }
    fn close_open(&mut self, out: &mut Vec<Value>) {
        if let Some((index, _)) = self.open.take() {
            self.writer.close((index, 0), out);
        }
    }
    fn block_of(&mut self, kind: Kind, start: Value, out: &mut Vec<Value>) -> (u64, u64) {
        if let Some((index, open)) = self.open
            && open == kind
            && kind != Kind::Tool
        {
            return (index, 0);
        }
        self.close_open(out);
        let index = self.next;
        self.next += 1;
        self.writer.ensure((index, 0), kind, start, out);
        self.open = Some((index, kind));
        (index, 0)
    }
    fn fail(&mut self, error: Value, out: &mut Vec<Value>) {
        self.close_open(out);
        let kind = match error["status"].as_u64() {
            Some(401) => "authentication_error",
            Some(403) => "permission_error",
            Some(429) => "rate_limit_error",
            Some(400..=499) => "invalid_request_error",
            _ => "api_error",
        };
        out.push(json!({"type":"error","error":{"type":kind,"message":error["message"].as_str().unwrap_or("Antigravity request failed")}}));
        self.finished = true;
    }
    fn end(&mut self, out: &mut Vec<Value>) {
        self.close_open(out);
        let Some(reason) = self.finish.clone() else {
            self.fail(
                json!({"status":502,"message":"The upstream stream ended without a finish reason."}),
                out,
            );
            return;
        };
        let stop = match reason.as_str() {
            "STOP" => {
                if self.writer.tool_use() {
                    "tool_use"
                } else {
                    "end_turn"
                }
            }
            "MAX_TOKENS" => "max_tokens",
            "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII"
            | "IMAGE_SAFETY" | "LANGUAGE" => "refusal",
            other => {
                let message = format!("The model stopped with finish reason {other}");
                self.fail(json!({"status":502,"message":message}), out);
                return;
            }
        };
        let meta = self.usage_metadata.clone().unwrap_or_else(|| json!({}));
        let thoughts = meta["thoughtsTokenCount"].as_u64().unwrap_or(0);
        let tokens = crate::convert::usage::Tokens {
            input: meta["promptTokenCount"].as_u64(),
            cached: meta["cachedContentTokenCount"].as_u64(),
            written: None,
            output: meta["candidatesTokenCount"]
                .as_u64()
                .map(|n| n.saturating_add(thoughts)),
        };
        out.extend(self.writer.finish(stop, tokens.messages()));
        self.finished = true;
    }
}
impl EventConversion for ToMessages {
    fn event(&mut self, chunk: Value) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        if self.finished {
            return Ok(out);
        }
        let body = chunk
            .get("response")
            .filter(|v| v.is_object())
            .unwrap_or(&chunk)
            .clone();
        if !self.writer.started() {
            let id = body["responseId"]
                .as_str()
                .or_else(|| chunk["responseId"].as_str())
                .filter(|id| !id.is_empty() && id.len() <= 128)
                .map(|id| json!(id))
                .unwrap_or(Value::Null);
            let model = body["modelVersion"]
                .as_str()
                .map_or_else(|| json!(self.model), |m| json!(m));
            out.extend(self.writer.start(&id, &model));
        }
        if let Some(meta) = body.get("usageMetadata").and_then(Value::as_object) {
            self.usage_metadata
                .get_or_insert_with(|| json!({}))
                .as_object_mut()
                .expect("usage object")
                .extend(meta.clone());
        }
        if let Some(error) = chunk_error(&chunk, &body, &self.model) {
            self.fail(error, &mut out);
            return Ok(out);
        }
        if let Some(reason) = body["promptFeedback"]["blockReason"].as_str() {
            self.block = Some(reason.to_owned());
        }
        if let Some(candidate) = body["candidates"].as_array().and_then(|c| c.first()) {
            for part in candidate["content"]["parts"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let signature = part["thoughtSignature"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned);
                if let Some(call) = part.get("functionCall").filter(|c| c.is_object()) {
                    if let Some(signature) = signature {
                        self.close_open(&mut out);
                        let key = self.block_of(
                            Kind::Thinking,
                            json!({"type":"thinking","thinking":""}),
                            &mut out,
                        );
                        self.writer.signature(key, &signature);
                        self.close_open(&mut out);
                    }
                    let id = call["id"].as_str().filter(|s| !s.is_empty()).map_or_else(
                        || format!("toolu_{}", uuid::Uuid::new_v4().simple()),
                        str::to_owned,
                    );
                    let key = self.block_of(
                        Kind::Tool,
                        json!({"type":"tool_use","id":id,"name":call["name"].as_str().unwrap_or_default(),"input":{}}),
                        &mut out,
                    );
                    let arguments = call
                        .get("args")
                        .filter(|a| !a.is_null())
                        .map_or_else(|| "{}".into(), Value::to_string);
                    self.writer.append(key, &arguments, &mut out)?;
                    self.close_open(&mut out);
                    continue;
                }
                if part["thought"] == true {
                    let key = self.block_of(
                        Kind::Thinking,
                        json!({"type":"thinking","thinking":""}),
                        &mut out,
                    );
                    if let Some(text) = part["text"].as_str() {
                        self.writer.append(key, text, &mut out)?;
                    }
                    if let Some(signature) = signature {
                        self.writer.signature(key, &signature);
                        self.close_open(&mut out);
                    }
                    continue;
                }
                if let Some(signature) = signature {
                    self.close_open(&mut out);
                    let key = self.block_of(
                        Kind::Thinking,
                        json!({"type":"thinking","thinking":""}),
                        &mut out,
                    );
                    self.writer.signature(key, &signature);
                    self.close_open(&mut out);
                }
                if let Some(text) = part_text(part).filter(|t| !t.is_empty()) {
                    let key = self.block_of(Kind::Text, json!({"type":"text","text":""}), &mut out);
                    self.writer.append(key, &text, &mut out)?;
                }
            }
            if let Some(reason) = candidate["finishReason"].as_str().filter(|r| !r.is_empty()) {
                self.finish = Some(reason.to_owned());
            }
        }
        Ok(out)
    }
    fn finish(&mut self) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        if !self.writer.started() || self.finished {
            return Ok(out);
        }
        if let Some(reason) = self.block.take() {
            let message = format!("The prompt was blocked: {reason}");
            self.fail(json!({"status":400,"message":message}), &mut out);
            return Ok(out);
        }
        self.end(&mut out);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(parts: Value, finish: Option<&str>, usage: Option<Value>) -> Value {
        let mut candidate = json!({"content":{"role":"model","parts":parts},"index":0});
        if let Some(finish) = finish {
            candidate["finishReason"] = finish.into();
        }
        let mut response = json!({"candidates":[candidate],"responseId":"gemini-response","modelVersion":"claude-sonnet-4-6"});
        if let Some(usage) = usage {
            response["usageMetadata"] = usage;
        }
        json!({"response":response})
    }

    #[test]
    fn thoughts_calls_text_and_signatures_become_ordered_responses_items() {
        let mut converter = Converter::new("claude-sonnet-4-6".into());
        let mut events = Vec::new();
        events.extend(converter.push(chunk(
            json!([{"text":"Let me look.","thought":true,"thoughtSignature":"sig-think"}]),
            None,
            None,
        )));
        events.extend(converter.push(chunk(
            json!([{"functionCall":{"name":"lookup","args":{"city":"Taipei"},"id":"toolu_1"}}]),
            None,
            None,
        )));
        events.extend(converter.push(chunk(json!([{"text":"Hel"}]), None, None)));
        events.extend(converter.push(chunk(
            json!([{"text":"lo"}]),
            Some("STOP"),
            Some(json!({"promptTokenCount":100,"cachedContentTokenCount":40,"candidatesTokenCount":20,"thoughtsTokenCount":5,"totalTokenCount":125})),
        )));
        events.extend(converter.finish());
        let kinds: Vec<&str> = events.iter().map(|e| e["type"].as_str().unwrap()).collect();
        assert_eq!(kinds[0], "response.created");
        assert_eq!(events[0]["response"]["id"], "gemini-response");
        assert!(kinds.contains(&"response.reasoning_summary_text.delta"));
        assert!(kinds.contains(&"response.function_call_arguments.delta"));
        assert!(kinds.contains(&"response.output_text.delta"));
        let done = events
            .iter()
            .filter(|e| e["type"] == "response.output_item.done")
            .collect::<Vec<_>>();
        assert_eq!(done[0]["item"]["type"], "reasoning");
        assert_eq!(done[0]["item"]["encrypted_content"], "sig-think");
        assert_eq!(done[0]["item"]["summary"][0]["text"], "Let me look.");
        assert_eq!(done[1]["item"]["type"], "function_call");
        assert_eq!(done[1]["item"]["call_id"], "toolu_1");
        assert_eq!(done[1]["item"]["arguments"], "{\"city\":\"Taipei\"}");
        let mut chat = crate::convert::responses::to_chat::Stream::new(true);
        let mut arguments = String::new();
        for event in &events {
            for chunk in chat.event(event).unwrap() {
                if let Some(delta) = chunk
                    .pointer("/choices/0/delta/tool_calls/0/function/arguments")
                    .and_then(Value::as_str)
                {
                    arguments.push_str(delta);
                }
            }
        }
        assert_eq!(
            serde_json::from_str::<Value>(&arguments).unwrap(),
            json!({"city":"Taipei"})
        );
        assert_eq!(done[2]["item"]["content"][0]["text"], "Hello");
        let terminal = events.last().unwrap();
        assert_eq!(terminal["type"], "response.completed");
        assert_eq!(terminal["response"]["output"].as_array().unwrap().len(), 3);
        assert_eq!(
            terminal["response"]["usage"],
            json!({"input_tokens":100,"input_tokens_details":{"cached_tokens":40},
            "output_tokens":25,"output_tokens_details":{"reasoning_tokens":5},"total_tokens":125})
        );
        assert!(converter.finished);
        assert!(
            converter
                .push(chunk(json!([{"text":"late"}]), None, None))
                .is_empty()
        );
    }

    #[test]
    fn terminal_failures_keep_usage_and_routed_model_without_inventing_measurements() {
        let mut converter = Converter::new("public-model".into());
        converter.push(chunk(
            json!([{"text":"partial"}]),
            None,
            Some(json!({"promptTokenCount":10,"candidatesTokenCount":2})),
        ));
        let events = converter.push(json!({"response":{"usageMetadata":{"thoughtsTokenCount":1},"error":{
            "code":429,"status":"RESOURCE_EXHAUSTED","message":"slow down",
            "details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"RATE_LIMIT_EXCEEDED","metadata":{"model":"variant"}},
                {"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"90s"}]
        }}}));
        let response = &events.last().unwrap()["response"];
        assert_eq!(response["status"], "failed");
        assert_eq!(response["usage"]["input_tokens"], 10);
        assert_eq!(response["usage"]["output_tokens"], 3);
        assert_eq!(response["usage"]["total_tokens"], 13);
        assert_eq!(response["error"]["model"], "public-model");
        assert_eq!(response["error"]["headers"]["retry-after"], "90");
        assert!(converter.finish().is_empty());
        assert_eq!(usage(&json!({})), json!({}));
        assert!(
            usage(&json!({"promptTokenCount":10}))
                .get("output_tokens")
                .is_none()
        );
        assert!(
            usage(&json!({"candidatesTokenCount":u64::MAX,"thoughtsTokenCount":1}))
                .get("output_tokens")
                .is_none()
        );
        for reason in [None, Some("OTHER"), Some("MALFORMED_FUNCTION_CALL")] {
            let mut converter = Converter::new("public-model".into());
            converter.push(chunk(
                json!([{"text":"partial"}]),
                reason,
                Some(json!({"promptTokenCount":10,"candidatesTokenCount":2})),
            ));
            let events = converter.finish();
            let terminal = events.last().unwrap();
            assert_eq!(terminal["type"], "response.failed");
            assert_eq!(terminal["response"]["usage"]["output_tokens"], 2);
            assert!(converter.finish().is_empty());
        }
    }

    #[test]
    fn signatures_on_calls_and_text_travel_as_reasoning_items_and_limits_are_terminal() {
        let mut converter = Converter::new("gemini-3-pro-high".into());
        let mut events = converter.push(chunk(
            json!([{"functionCall":{"name":"run","args":{}},"thoughtSignature":"sig-call"},{"text":"done","thoughtSignature":"sig-text"}]),
            Some("MAX_TOKENS"),
            None,
        ));
        events.extend(converter.finish());
        let items = &events.last().unwrap()["response"]["output"];
        assert_eq!(items[0]["type"], "reasoning");
        assert_eq!(items[0]["encrypted_content"], "sig-call");
        assert_eq!(items[1]["type"], "function_call");
        assert!(items[1]["call_id"].as_str().unwrap().starts_with("call_"));
        assert_eq!(items[2]["encrypted_content"], "sig-text");
        assert_eq!(items[3]["content"][0]["text"], "done");
        assert_eq!(events.last().unwrap()["type"], "response.incomplete");
        assert_eq!(
            events.last().unwrap()["response"]["incomplete_details"]["reason"],
            "max_output_tokens"
        );
        let mut truncated = Converter::new("gemini-3-pro-high".into());
        truncated.push(chunk(json!([{"text":"partial"}]), None, None));
        let tail = truncated.finish();
        assert!(tail.iter().all(|e| {
            !e["type"]
                .as_str()
                .unwrap()
                .starts_with("response.completed")
        }));
        let mut failed = Converter::new("gemini-3-pro-high".into());
        let events = failed
            .push(json!({"error":{"code":429,"message":"quota","status":"RESOURCE_EXHAUSTED"}}));
        assert_eq!(events.last().unwrap()["type"], "response.failed");
        assert_eq!(
            events.last().unwrap()["response"]["error"]["code"],
            "resource_exhausted"
        );
        assert!(failed.finished);
    }
}
