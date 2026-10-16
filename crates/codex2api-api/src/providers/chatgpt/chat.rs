use crate::{ApiError, Result, request::content_parts};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request(format!("{key} must be a nonempty string")))
}

pub(crate) fn chat_to_responses(body: &Value) -> Result<Value> {
    let mut output = body
        .as_object()
        .cloned()
        .ok_or_else(|| ApiError::bad_request("Request body must be an object"))?;
    let messages = output
        .remove("messages")
        .ok_or_else(|| ApiError::bad_request("messages is required"))?;
    let messages = messages
        .as_array()
        .filter(|items| !items.is_empty())
        .ok_or_else(|| ApiError::bad_request("messages must be a nonempty array"))?;
    let mut input = Vec::new();
    let mut calls = BTreeMap::<String, bool>::new();
    for message in messages {
        let role = required(message, "role")?;
        if role == "tool" {
            let call_id = required(message, "tool_call_id")?.to_owned();
            let content = match &message["content"] {
                Value::String(_) => message["content"].clone(),
                Value::Array(_) => content_parts(message["content"].clone(), false)?.into(),
                _ => {
                    return Err(ApiError::bad_request(
                        "Tool output must be text or content parts",
                    ));
                }
            };
            let custom = calls.get(&call_id).copied().unwrap_or(false);
            input.push(json!({"type":if custom {"custom_tool_call_output"} else {"function_call_output"},"call_id":call_id,"output":content}));
            continue;
        }
        if !matches!(role, "system" | "developer" | "user" | "assistant") {
            return Err(ApiError::bad_request("Unsupported message role"));
        }
        if role == "assistant" {
            // The reasoning extension is lossless; reasoning_content is the widely used text view.
            if let Some(reasoning) = message.get("reasoning").filter(|v| !v.is_null()) {
                match reasoning {
                    Value::Array(items) => {
                        for item in items {
                            if !item.is_object() {
                                return Err(ApiError::bad_request(
                                    "reasoning items must be objects",
                                ));
                            }
                            let mut item = item.clone();
                            item["type"] = "reasoning".into();
                            input.push(item);
                        }
                    }
                    Value::Object(_) => {
                        let mut item = reasoning.clone();
                        item["type"] = "reasoning".into();
                        input.push(item);
                    }
                    _ => {
                        return Err(ApiError::bad_request(
                            "Message reasoning must be an object or array",
                        ));
                    }
                }
            } else if message
                .get("reasoning_content")
                .is_some_and(|v| !v.is_null())
                || message
                    .get("encrypted_content")
                    .is_some_and(|v| !v.is_null())
            {
                let mut reasoning = json!({"type":"reasoning","summary":[]});
                if let Some(text) = message.get("reasoning_content").filter(|v| !v.is_null()) {
                    if !text.is_string() {
                        return Err(ApiError::bad_request("reasoning_content must be a string"));
                    }
                    reasoning["summary"] = json!([{"type":"summary_text","text":text}]);
                }
                if let Some(encrypted) = message.get("encrypted_content").filter(|v| !v.is_null()) {
                    reasoning["encrypted_content"] = encrypted.clone();
                }
                input.push(reasoning);
            }
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
            for field in ["name", "phase", "audio"] {
                if let Some(value) = message.get(field).filter(|v| !v.is_null()) {
                    item[field] = value.clone();
                }
            }
            input.push(item);
        }
        if let Some(tool_calls) = message.get("tool_calls").filter(|v| !v.is_null()) {
            if role != "assistant" {
                return Err(ApiError::bad_request(
                    "tool_calls require an assistant message",
                ));
            }
            for call in tool_calls
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
                let value = payload[field].as_str().ok_or_else(|| {
                    ApiError::bad_request(format!("Tool {field} must be a string"))
                })?;
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
                calls.insert(call_id.into(), kind == "custom_tool_call");
                input.push(item);
            }
        }
    }
    output.insert("input".into(), input.into());
    if let Some(effort) = output.remove("reasoning_effort") {
        let reasoning = output
            .entry("reasoning")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| ApiError::bad_request("reasoning must be an object"))?;
        reasoning.entry("effort").or_insert(effort);
    }
    let format = output.remove("response_format");
    let verbosity = output.remove("verbosity");
    if format.is_some() || verbosity.is_some() {
        let text = output
            .entry("text")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| ApiError::bad_request("text must be an object"))?;
        if let Some(format) = format {
            let format = match format["type"].as_str() {
                Some("text" | "json_object") => format,
                Some("json_schema") => {
                    let mut schema =
                        format["json_schema"].as_object().cloned().ok_or_else(|| {
                            ApiError::bad_request("response_format.json_schema must be an object")
                        })?;
                    schema.insert("type".into(), "json_schema".into());
                    schema.into()
                }
                _ => return Err(ApiError::bad_request("Invalid response_format type")),
            };
            text.insert("format".into(), format);
        }
        if let Some(verbosity) = verbosity {
            text.insert("verbosity".into(), verbosity);
        }
    }
    if let Some(search) = output.remove("web_search_options") {
        let mut search = search
            .as_object()
            .cloned()
            .ok_or_else(|| ApiError::bad_request("web_search_options must be an object"))?;
        search.insert("type".into(), "web_search".into());
        output
            .entry("tools")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| ApiError::bad_request("tools must be an array"))?
            .push(search.into());
    }
    if output.remove("logprobs").is_some_and(|v| v == true) {
        let include = output
            .entry("include")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| ApiError::bad_request("include must be an array"))?;
        if !include.iter().any(|v| v == "message.output_text.logprobs") {
            include.push("message.output_text.logprobs".into());
        }
    }
    Ok(output.into())
}

pub(crate) fn chat_usage(usage: &Value) -> Value {
    let Some(usage) = usage.as_object() else {
        return Value::Null;
    };
    let mut result = usage.clone();
    for (responses, chat) in [
        ("input_tokens", "prompt_tokens"),
        ("output_tokens", "completion_tokens"),
        ("input_tokens_details", "prompt_tokens_details"),
        ("output_tokens_details", "completion_tokens_details"),
    ] {
        if let Some(value) = result.remove(responses) {
            result.insert(chat.into(), value);
        }
    }
    if !result.contains_key("total_tokens") {
        if let (Some(input), Some(output)) = (
            result.get("prompt_tokens").and_then(Value::as_u64),
            result.get("completion_tokens").and_then(Value::as_u64),
        ) {
            result.insert("total_tokens".into(), input.saturating_add(output).into());
        }
    }
    result.into()
}

fn finish_reason(response: &Value, tools: bool) -> Result<&'static str> {
    match response["status"].as_str() {
        Some("completed") => Ok(if tools { "tool_calls" } else { "stop" }),
        Some("incomplete") => match response["incomplete_details"]["reason"].as_str() {
            Some("max_output_tokens") => Ok("length"),
            Some("content_filter") => Ok("content_filter"),
            _ => Err(ApiError::upstream_incomplete()),
        },
        _ => Err(ApiError::upstream_value(response)),
    }
}

fn tool_call(item: &Value) -> Option<Value> {
    let (kind, field) = match item["type"].as_str()? {
        "function_call" => ("function", "arguments"),
        "custom_tool_call" => ("custom", "input"),
        _ => return None,
    };
    let mut payload = json!({"name":item["name"],field:item[field]});
    for key in ["namespace", "encrypted_function_args"] {
        if let Some(value) = item.get(key) {
            payload[key] = value.clone();
        }
    }
    Some(json!({"id":item["call_id"],"type":kind,kind:payload}))
}

fn annotation(value: &Value, offset: usize) -> Value {
    if value["type"] != "url_citation" || value["url_citation"].is_object() {
        return value.clone();
    }
    let mut citation = value.as_object().cloned().unwrap_or_default();
    citation.remove("type");
    for field in ["start_index", "end_index"] {
        if let Some(index) = citation.get(field).and_then(Value::as_u64) {
            citation.insert(field.into(), index.saturating_add(offset as u64).into());
        }
    }
    json!({"type":"url_citation","url_citation":citation})
}

pub(crate) fn chat_response(response: &Value) -> Result<Value> {
    let mut text = String::new();
    let mut refusal = String::new();
    let mut reasoning_text = String::new();
    let mut reasoning = Vec::new();
    let mut tools = Vec::new();
    let mut annotations = Vec::new();
    let mut logprobs = Vec::new();
    let mut audio = Value::Null;
    for item in response["output"].as_array().into_iter().flatten() {
        if let Some(call) = tool_call(item) {
            tools.push(call);
        }
        if item["type"] == "reasoning" {
            for part in item["summary"].as_array().into_iter().flatten() {
                reasoning_text.push_str(part["text"].as_str().unwrap_or(""));
            }
            for part in item["content"].as_array().into_iter().flatten() {
                reasoning_text.push_str(part["text"].as_str().unwrap_or(""));
            }
            reasoning.push(item.clone());
        }
        for part in item["content"].as_array().into_iter().flatten() {
            match part["type"].as_str() {
                Some("output_text") => {
                    let offset = text.chars().count();
                    annotations.extend(
                        part["annotations"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|v| annotation(v, offset)),
                    );
                    logprobs.extend(part["logprobs"].as_array().into_iter().flatten().cloned());
                    text.push_str(part["text"].as_str().unwrap_or(""));
                }
                Some("refusal") => refusal.push_str(part["refusal"].as_str().unwrap_or("")),
                Some("audio" | "output_audio") => {
                    audio = part.clone();
                    audio.as_object_mut().expect("audio part").remove("type");
                }
                _ => {}
            }
        }
    }
    let finish = finish_reason(response, !tools.is_empty())?;
    let mut message = json!({"role":"assistant","content":if text.is_empty() {Value::Null} else {text.into()},"refusal":if refusal.is_empty() {Value::Null} else {refusal.into()}});
    if !tools.is_empty() {
        message["tool_calls"] = tools.into();
    }
    if !reasoning.is_empty() {
        message["reasoning"] = reasoning.into();
    }
    if !reasoning_text.is_empty() {
        message["reasoning_content"] = reasoning_text.into();
    }
    if !annotations.is_empty() {
        message["annotations"] = annotations.into();
    }
    if !audio.is_null() {
        message["audio"] = audio;
    }
    let logprobs = if logprobs.is_empty() {
        Value::Null
    } else {
        json!({"content":logprobs,"refusal":null})
    };
    let mut result = json!({"id":response["id"],"object":"chat.completion","created":response["created_at"],"model":response["model"],"choices":[{"index":0,"message":message,"finish_reason":finish,"logprobs":logprobs}],"usage":chat_usage(&response["usage"])});
    for field in ["service_tier", "system_fingerprint", "metadata"] {
        if let Some(value) = response.get(field) {
            result[field] = value.clone();
        }
    }
    Ok(result)
}

#[derive(Default)]
struct StreamTool {
    index: usize,
    custom: bool,
    text: String,
}

/// Only tracks emitted fields. Execution owns response accumulation and terminal usage settlement.
#[derive(Default)]
pub(crate) struct ChatStream {
    id: Value,
    model: Value,
    created: Value,
    service_tier: Option<Value>,
    role_sent: bool,
    tools: BTreeMap<u64, StreamTool>,
    parts: BTreeMap<(u64, u64, &'static str), String>,
    logprob_counts: BTreeMap<(u64, u64), usize>,
    annotations: BTreeSet<(u64, u64, u64)>,
    reasoning_done: BTreeSet<u64>,
    audio_metadata_sent: bool,
    finished: bool,
    pub include_usage: bool,
}

impl ChatStream {
    pub(crate) fn new(include_usage: bool) -> Self {
        Self {
            include_usage,
            ..Default::default()
        }
    }

    fn chunk(&self, delta: Value, finish: Value) -> Value {
        let mut value = json!({"id":self.id,"object":"chat.completion.chunk","created":self.created,"model":self.model,"choices":[{"index":0,"delta":delta,"finish_reason":finish,"logprobs":null}]});
        if self.include_usage {
            value["usage"] = Value::Null;
        }
        if let Some(tier) = &self.service_tier {
            value["service_tier"] = tier.clone();
        }
        value
    }

    fn text(
        &mut self,
        output: u64,
        part: u64,
        field: &'static str,
        text: &str,
        done: bool,
    ) -> Result<Option<Value>> {
        let emitted = self.parts.entry((output, part, field)).or_default();
        let suffix = if done {
            text.strip_prefix(emitted.as_str())
                .ok_or_else(ApiError::upstream_incomplete)?
        } else {
            text
        };
        if suffix.is_empty() {
            return Ok(None);
        }
        emitted.push_str(suffix);
        let delta = match field {
            "audio_data" => json!({"audio":{"data":suffix}}),
            "audio_transcript" => json!({"audio":{"transcript":suffix}}),
            _ => json!({field:suffix}),
        };
        Ok(Some(self.chunk(delta, Value::Null)))
    }

    fn item(&mut self, output: u64, item: &Value, complete: bool) -> Result<Vec<Value>> {
        let mut chunks = Vec::new();
        if let Some(mut call) = tool_call(item) {
            let custom = item["type"] == "custom_tool_call";
            let field = if custom { "input" } else { "arguments" };
            let kind = if custom { "custom" } else { "function" };
            let full = item[field].as_str().unwrap_or("");
            if !self.tools.contains_key(&output) {
                let index = self.tools.len();
                self.tools.insert(
                    output,
                    StreamTool {
                        index,
                        custom,
                        text: full.into(),
                    },
                );
                call["index"] = index.into();
                call[kind][field] = full.into();
                chunks.push(self.chunk(json!({"tool_calls":[call]}), Value::Null));
            } else if complete {
                let tool = self.tools.get_mut(&output).expect("existing tool");
                let suffix = full
                    .strip_prefix(&tool.text)
                    .ok_or_else(ApiError::upstream_incomplete)?
                    .to_owned();
                tool.text.push_str(&suffix);
                let index = tool.index;
                if !suffix.is_empty() {
                    chunks.push(self.chunk(
                        json!({"tool_calls":[{"index":index,kind:{field:suffix}}]}),
                        Value::Null,
                    ));
                }
            }
        }
        if complete && item["type"] == "reasoning" {
            for (index, part) in item["summary"].as_array().into_iter().flatten().enumerate() {
                if let Some(chunk) = self.text(
                    output,
                    index as u64,
                    "reasoning_content",
                    part["text"].as_str().unwrap_or(""),
                    true,
                )? {
                    chunks.push(chunk);
                }
            }
            for (index, part) in item["content"].as_array().into_iter().flatten().enumerate() {
                if let Some(chunk) = self.text(
                    output,
                    index as u64 + (1 << 32),
                    "reasoning_content",
                    part["text"].as_str().unwrap_or(""),
                    true,
                )? {
                    chunks.push(chunk);
                }
            }
            if self.reasoning_done.insert(output) {
                chunks.push(self.chunk(json!({"reasoning":[item]}), Value::Null));
            }
        }
        if complete && item["type"] == "message" {
            for (index, part) in item["content"].as_array().into_iter().flatten().enumerate() {
                chunks.extend(self.content_part(output, index as u64, part)?);
            }
        }
        Ok(chunks)
    }

    fn logprobs(
        &mut self,
        output: u64,
        index: u64,
        value: &Value,
        complete: bool,
    ) -> Option<Value> {
        let values = value.as_array()?;
        let count = self.logprob_counts.entry((output, index)).or_default();
        let offset = if complete {
            (*count).min(values.len())
        } else {
            0
        };
        let suffix = &values[offset..];
        *count += suffix.len();
        (!suffix.is_empty()).then(|| json!({"content":suffix,"refusal":null}))
    }

    fn content_part(&mut self, output: u64, index: u64, part: &Value) -> Result<Vec<Value>> {
        let mut chunks = Vec::new();
        if matches!(part["type"].as_str(), Some("audio" | "output_audio")) {
            for (key, field) in [("data", "audio_data"), ("transcript", "audio_transcript")] {
                if let Some(text) = part[key].as_str() {
                    chunks.extend(self.text(0, 0, field, text, true)?);
                }
            }
            if !self.audio_metadata_sent {
                let mut audio = part.as_object().cloned().unwrap_or_default();
                for key in ["type", "data", "transcript"] {
                    audio.remove(key);
                }
                if !audio.is_empty() {
                    self.audio_metadata_sent = true;
                    chunks.push(self.chunk(json!({"audio":audio}), Value::Null));
                }
            }
        }
        let field = match part["type"].as_str() {
            Some("output_text") => Some(("content", "text")),
            Some("refusal") => Some(("refusal", "refusal")),
            _ => None,
        };
        if let Some((field, key)) = field {
            let mut chunk =
                self.text(output, index, field, part[key].as_str().unwrap_or(""), true)?;
            if let Some(logprobs) = self.logprobs(output, index, &part["logprobs"], true) {
                let chunk = chunk.get_or_insert_with(|| self.chunk(json!({}), Value::Null));
                chunk["choices"][0]["logprobs"] = logprobs;
            }
            chunks.extend(chunk);
        }
        for (annotation_index, value) in part["annotations"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            if self
                .annotations
                .insert((output, index, annotation_index as u64))
            {
                chunks.push(self.chunk(
                    json!({"annotations":[annotation(value, self.text_offset(output, index))]}),
                    Value::Null,
                ));
            }
        }
        Ok(chunks)
    }

    fn text_offset(&self, output: u64, index: u64) -> usize {
        self.parts
            .iter()
            .filter(|((item, part, field), _)| {
                *field == "content" && (*item, *part) < (output, index)
            })
            .map(|(_, text)| text.chars().count())
            .sum()
    }

    pub(crate) fn event(&mut self, event: &Value) -> Result<Vec<Value>> {
        if self.finished {
            return Ok(Vec::new());
        }
        let response = &event["response"];
        for (target, key) in [
            (&mut self.id, "id"),
            (&mut self.model, "model"),
            (&mut self.created, "created_at"),
        ] {
            if let Some(value) = response.get(key) {
                *target = value.clone();
            }
        }
        if let Some(tier) = response.get("service_tier") {
            self.service_tier = Some(tier.clone());
        }
        let mut chunks = Vec::new();
        if !self.role_sent && !self.id.is_null() {
            self.role_sent = true;
            chunks.push(self.chunk(json!({"role":"assistant","content":""}), Value::Null));
        }
        let output = event["output_index"].as_u64().unwrap_or(0);
        let part = event["content_index"].as_u64().unwrap_or(0);
        match event["type"].as_str() {
            Some(
                "response.output_text.delta"
                | "response.output_text.done"
                | "response.refusal.delta"
                | "response.refusal.done"
                | "response.reasoning_summary_text.delta"
                | "response.reasoning_summary_text.done"
                | "response.reasoning_text.delta"
                | "response.reasoning_text.done",
            ) => {
                let kind = event["type"].as_str().expect("matched type");
                let done = kind.ends_with(".done");
                let (field, part) = if kind.contains("reasoning_summary") {
                    (
                        "reasoning_content",
                        event["summary_index"].as_u64().unwrap_or(0),
                    )
                } else if kind.contains("reasoning_text") {
                    ("reasoning_content", part + (1 << 32))
                } else if kind.contains("refusal") {
                    ("refusal", part)
                } else {
                    ("content", part)
                };
                let key = if !done {
                    "delta"
                } else if field == "refusal" {
                    "refusal"
                } else {
                    "text"
                };
                let text = event[key]
                    .as_str()
                    .ok_or_else(ApiError::upstream_incomplete)?;
                let mut chunk = self.text(output, part, field, text, done)?;
                if let Some(logprobs) = self.logprobs(output, part, &event["logprobs"], done) {
                    let chunk = chunk.get_or_insert_with(|| self.chunk(json!({}), Value::Null));
                    chunk["choices"][0]["logprobs"] = logprobs;
                }
                chunks.extend(chunk);
            }
            Some("response.output_text.annotation.added") => {
                let index = event["annotation_index"]
                    .as_u64()
                    .unwrap_or(self.annotations.len() as u64);
                if self.annotations.insert((output, part, index)) {
                    chunks.push(self.chunk(json!({"annotations":[annotation(&event["annotation"], self.text_offset(output, part))]}), Value::Null));
                }
            }
            Some("response.output_item.added" | "response.output_item.done") => {
                chunks.extend(self.item(
                    output,
                    &event["item"],
                    event["type"] == "response.output_item.done",
                )?);
            }
            Some("response.content_part.done") => {
                chunks.extend(self.content_part(output, part, &event["part"])?)
            }
            Some(
                "response.function_call_arguments.delta"
                | "response.function_call_arguments.done"
                | "response.custom_tool_call_input.delta"
                | "response.custom_tool_call_input.done",
            ) => {
                let kind = event["type"].as_str().expect("matched type");
                let tool = self
                    .tools
                    .get_mut(&output)
                    .ok_or_else(ApiError::upstream_incomplete)?;
                let field = if tool.custom { "input" } else { "arguments" };
                let key = if kind.ends_with(".done") {
                    field
                } else {
                    "delta"
                };
                let full = event[key]
                    .as_str()
                    .ok_or_else(ApiError::upstream_incomplete)?;
                let suffix = if kind.ends_with(".done") {
                    full.strip_prefix(&tool.text)
                        .ok_or_else(ApiError::upstream_incomplete)?
                } else {
                    full
                }
                .to_owned();
                tool.text.push_str(&suffix);
                let index = tool.index;
                let kind = if tool.custom { "custom" } else { "function" };
                if !suffix.is_empty() {
                    chunks.push(self.chunk(
                        json!({"tool_calls":[{"index":index,kind:{field:suffix}}]}),
                        Value::Null,
                    ));
                }
            }
            Some("response.audio.delta" | "response.output_audio.delta") => {
                let data = event["delta"]
                    .as_str()
                    .ok_or_else(ApiError::upstream_incomplete)?;
                chunks.extend(self.text(0, 0, "audio_data", data, false)?);
            }
            Some("response.audio_transcript.delta" | "response.output_audio_transcript.delta") => {
                let transcript = event["delta"]
                    .as_str()
                    .ok_or_else(ApiError::upstream_incomplete)?;
                chunks.extend(self.text(0, 0, "audio_transcript", transcript, false)?);
            }
            Some("response.completed" | "response.done" | "response.incomplete") => {
                for (index, item) in response["output"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    chunks.extend(self.item(index as u64, item, true)?);
                }
                let reason = finish_reason(response, !self.tools.is_empty())?;
                chunks.push(self.chunk(json!({}), reason.into()));
                if self.include_usage {
                    let mut usage = self.chunk(json!({}), Value::Null);
                    usage["choices"] = json!([]);
                    usage["usage"] = chat_usage(&response["usage"]);
                    chunks.push(usage);
                }
                self.finished = true;
            }
            Some("response.failed" | "error") => return Err(ApiError::upstream_value(event)),
            _ => {}
        }
        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn api_rewrite_chat_history_preserves_custom_tools_structured_outputs_and_multimodal_content() {
        let body = json!({"model":"custom","messages":[{"role":"system","name":"policy","content":"Be precise"},
            {"role":"user","content":[{"type":"text","text":"Read this"},{"type":"image_url","image_url":{"url":"https://example.test/image.png","detail":"original"}},{"type":"file","file":{"file_id":"file_1"}},{"type":"input_audio","input_audio":{"format":"wav","data":"AAAA"}}]},
            {"role":"assistant","content":null,"reasoning":[{"encrypted_content":"encrypted","summary":[]}],"tool_calls":[{"id":"call_1","type":"custom","custom":{"name":"code","input":"1+1","namespace":"tools"}}]},
            {"role":"tool","tool_call_id":"call_1","content":[{"type":"text","text":"2"}]}],
            "tools":[{"type":"custom","custom":{"name":"code"}}],"response_format":{"type":"json_schema","json_schema":{"name":"result","strict":true,"schema":{"type":"object"}}},"reasoning_effort":"high","verbosity":"low","web_search_options":{"search_context_size":"high"}});
        let value = chat_to_responses(&body).unwrap();
        assert_eq!(value["input"][0]["name"], "policy");
        assert_eq!(value["input"][1]["content"][1]["detail"], "original");
        assert_eq!(value["input"][1]["content"][2]["type"], "input_file");
        assert_eq!(
            value["input"][1]["content"][3]["audio_url"],
            "data:audio/wav;base64,AAAA"
        );
        assert_eq!(value["input"][2]["encrypted_content"], "encrypted");
        assert_eq!(value["input"][3]["namespace"], "tools");
        assert_eq!(value["input"][4]["type"], "custom_tool_call_output");
        assert_eq!(value["text"]["format"]["strict"], true);
        assert_eq!(value["text"]["verbosity"], "low");
        assert_eq!(value["tools"][1]["type"], "web_search");
    }
    #[test]
    fn api_rewrite_stream_and_sync_keep_reasoning_custom_calls_refusal_citations_and_usage() {
        let response = json!({"id":"resp_1","model":"custom","created_at":123,"status":"completed","output":[
            {"type":"reasoning","summary":[{"type":"summary_text","text":"Thought"}],"encrypted_content":"encrypted"},
            {"type":"custom_tool_call","call_id":"call_1","name":"code","namespace":"tools","input":"print(1)"},
            {"type":"message","content":[{"type":"output_text","text":"Hello","annotations":[{"type":"url_citation","url":"https://example.test","title":"example","start_index":0,"end_index":5}]},{"type":"refusal","refusal":"No"}]}],
            "usage":{"input_tokens":10,"output_tokens":4,"output_tokens_details":{"reasoning_tokens":2}}});
        let sync = chat_response(&response).unwrap();
        assert_eq!(
            sync["choices"][0]["message"]["reasoning_content"],
            "Thought"
        );
        assert_eq!(
            sync["choices"][0]["message"]["reasoning"][0]["encrypted_content"],
            "encrypted"
        );
        assert_eq!(
            sync["choices"][0]["message"]["tool_calls"][0]["custom"]["namespace"],
            "tools"
        );
        assert_eq!(
            sync["choices"][0]["message"]["annotations"][0]["url_citation"]["url"],
            "https://example.test"
        );
        let mut stream = ChatStream::new(true);
        stream.event(&json!({"type":"response.created","response":{"id":"resp_1","model":"custom","created_at":123}})).unwrap();
        stream.event(&json!({"type":"response.output_item.added","output_index":1,"item":{"type":"custom_tool_call","call_id":"call_1","name":"code","input":"","namespace":"tools"}})).unwrap();
        let tool_delta = stream.event(&json!({"type":"response.custom_tool_call_input.delta","output_index":1,"delta":"print("})).unwrap();
        assert_eq!(
            tool_delta[0]["choices"][0]["delta"]["tool_calls"][0]["custom"]["input"],
            "print("
        );
        let chunks = stream
            .event(&json!({"type":"response.completed","response":response}))
            .unwrap();
        assert!(
            chunks
                .iter()
                .any(|v| v["choices"][0]["delta"]["tool_calls"][0]["custom"]["input"] == "1)")
        );
        assert!(
            chunks
                .iter()
                .any(|v| v["choices"][0]["delta"]["content"] == "Hello")
        );
        assert!(
            chunks
                .iter()
                .any(|v| v["choices"][0]["delta"]["refusal"] == "No")
        );
        assert_eq!(chunks.last().unwrap()["usage"]["total_tokens"], 14);
        assert!(
            stream
                .event(&json!({"type":"response.completed","response":response}))
                .unwrap()
                .is_empty()
        );
    }
}
