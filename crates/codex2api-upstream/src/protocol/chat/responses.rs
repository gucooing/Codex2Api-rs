//! Chat Completions callers served by a Responses supplier.
use super::Chunks;
use crate::protocol::{Error as ApiError, Result, request::content_parts};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request(format!("{key} must be a nonempty string")))
}

/// Build the Responses request from the Chat fields it can represent.
pub fn request(body: &Value) -> Result<Value> {
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
    crate::protocol::request::normalize_tools(&mut out)?;
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

pub(crate) fn usage(usage: &Value) -> Value {
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
    if !result.contains_key("total_tokens")
        && let (Some(input), Some(output)) = (
            result.get("prompt_tokens").and_then(Value::as_u64),
            result.get("completion_tokens").and_then(Value::as_u64),
        )
    {
        result.insert("total_tokens".into(), input.saturating_add(output).into());
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

struct StreamTool {
    index: usize,
    custom: bool,
    text: String,
}

/// Responses events to Chat chunks. Terminal snapshots only add what deltas omitted.
pub struct Stream {
    chunks: Chunks,
    role_sent: bool,
    tools: BTreeMap<u64, StreamTool>,
    parts: BTreeMap<(u64, u64, &'static str), String>,
    logprob_counts: BTreeMap<(u64, u64), usize>,
    annotations: BTreeSet<(u64, u64, u64)>,
    reasoning_done: BTreeSet<u64>,
    audio_metadata_sent: bool,
    finished: bool,
}

impl Stream {
    pub(crate) fn new(include_usage: bool) -> Self {
        Self {
            chunks: Chunks {
                include_usage,
                ..Default::default()
            },
            role_sent: false,
            tools: BTreeMap::new(),
            parts: BTreeMap::new(),
            logprob_counts: BTreeMap::new(),
            annotations: BTreeSet::new(),
            reasoning_done: BTreeSet::new(),
            audio_metadata_sent: false,
            finished: false,
        }
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
            match text.strip_prefix(emitted.as_str()) {
                Some(suffix) => suffix,
                // A divergent snapshot cannot be expressed as an append.
                None => return Ok(None),
            }
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
        Ok(Some(self.chunks.chunk(delta, Value::Null)))
    }

    fn item(&mut self, output: u64, item: &Value, complete: bool) -> Result<Vec<Value>> {
        let mut chunks = Vec::new();
        if let Some(mut call) = tool_call(item) {
            let custom = item["type"] == "custom_tool_call";
            let field = if custom { "input" } else { "arguments" };
            let kind = if custom { "custom" } else { "function" };
            let full = item[field].as_str().unwrap_or("");
            if let Some(tool) = self.tools.get_mut(&output) {
                if complete && let Some(suffix) = full.strip_prefix(tool.text.as_str()) {
                    let suffix = suffix.to_owned();
                    tool.text.push_str(&suffix);
                    let index = tool.index;
                    if !suffix.is_empty() {
                        chunks.push(self.chunks.chunk(
                            json!({"tool_calls":[{"index":index,kind:{field:suffix}}]}),
                            Value::Null,
                        ));
                    }
                }
            } else {
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
                chunks.push(self.chunks.chunk(json!({"tool_calls":[call]}), Value::Null));
            }
        }
        if complete && item["type"] == "reasoning" {
            for (key, offset) in [("summary", 0), ("content", 1u64 << 32)] {
                for (index, part) in item[key].as_array().into_iter().flatten().enumerate() {
                    chunks.extend(self.text(
                        output,
                        index as u64 + offset,
                        "reasoning_content",
                        part["text"].as_str().unwrap_or(""),
                        true,
                    )?);
                }
            }
            if self.reasoning_done.insert(output) {
                chunks.push(self.chunks.chunk(json!({"reasoning":[item]}), Value::Null));
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
                    chunks.push(self.chunks.chunk(json!({"audio":audio}), Value::Null));
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
                let chunk = chunk.get_or_insert_with(|| self.chunks.chunk(json!({}), Value::Null));
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
                chunks.push(self.chunks.chunk(
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
            (&mut self.chunks.id, "id"),
            (&mut self.chunks.model, "model"),
            (&mut self.chunks.created, "created_at"),
        ] {
            if let Some(value) = response.get(key) {
                *target = value.clone();
            }
        }
        if let Some(tier) = response.get("service_tier") {
            self.chunks.service_tier = Some(tier.clone());
        }
        let mut chunks = Vec::new();
        if !self.role_sent && !self.chunks.id.is_null() {
            self.role_sent = true;
            chunks.push(
                self.chunks
                    .chunk(json!({"role":"assistant","content":""}), Value::Null),
            );
        }
        let output = event["output_index"].as_u64().unwrap_or(0);
        let part = event["content_index"].as_u64().unwrap_or(0);
        let kind = event["type"].as_str().unwrap_or("");
        match kind {
            "response.output_text.delta"
            | "response.output_text.done"
            | "response.refusal.delta"
            | "response.refusal.done"
            | "response.reasoning_summary_text.delta"
            | "response.reasoning_summary_text.done"
            | "response.reasoning_text.delta"
            | "response.reasoning_text.done" => {
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
                let key = match (done, field) {
                    (false, _) => "delta",
                    (true, "refusal") => "refusal",
                    (true, _) => "text",
                };
                let text = event[key]
                    .as_str()
                    .ok_or_else(ApiError::upstream_incomplete)?;
                let mut chunk = self.text(output, part, field, text, done)?;
                if let Some(logprobs) = self.logprobs(output, part, &event["logprobs"], done) {
                    let chunk =
                        chunk.get_or_insert_with(|| self.chunks.chunk(json!({}), Value::Null));
                    chunk["choices"][0]["logprobs"] = logprobs;
                }
                chunks.extend(chunk);
            }
            "response.output_text.annotation.added" => {
                let index = event["annotation_index"]
                    .as_u64()
                    .unwrap_or(self.annotations.len() as u64);
                if self.annotations.insert((output, part, index)) {
                    chunks.push(self.chunks.chunk(
                        json!({"annotations":[annotation(&event["annotation"], self.text_offset(output, part))]}),
                        Value::Null,
                    ));
                }
            }
            "response.output_item.added" | "response.output_item.done" => {
                chunks.extend(self.item(
                    output,
                    &event["item"],
                    kind == "response.output_item.done",
                )?);
            }
            "response.content_part.done" => {
                chunks.extend(self.content_part(output, part, &event["part"])?)
            }
            "response.function_call_arguments.delta"
            | "response.function_call_arguments.done"
            | "response.custom_tool_call_input.delta"
            | "response.custom_tool_call_input.done" => {
                let Some(tool) = self.tools.get_mut(&output) else {
                    return Err(ApiError::upstream_incomplete());
                };
                let field = if tool.custom { "input" } else { "arguments" };
                let done = kind.ends_with(".done");
                let full = event[if done { field } else { "delta" }]
                    .as_str()
                    .ok_or_else(ApiError::upstream_incomplete)?;
                let suffix = if done {
                    full.strip_prefix(tool.text.as_str()).unwrap_or("")
                } else {
                    full
                }
                .to_owned();
                tool.text.push_str(&suffix);
                let index = tool.index;
                let kind = if tool.custom { "custom" } else { "function" };
                if !suffix.is_empty() {
                    chunks.push(self.chunks.chunk(
                        json!({"tool_calls":[{"index":index,kind:{field:suffix}}]}),
                        Value::Null,
                    ));
                }
            }
            "response.audio.delta" | "response.output_audio.delta" => {
                let data = event["delta"]
                    .as_str()
                    .ok_or_else(ApiError::upstream_incomplete)?;
                chunks.extend(self.text(0, 0, "audio_data", data, false)?);
            }
            "response.audio_transcript.delta" | "response.output_audio_transcript.delta" => {
                let transcript = event["delta"]
                    .as_str()
                    .ok_or_else(ApiError::upstream_incomplete)?;
                chunks.extend(self.text(0, 0, "audio_transcript", transcript, false)?);
            }
            "response.completed" | "response.done" | "response.incomplete" => {
                for (index, item) in response["output"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    chunks.extend(self.item(index as u64, item, true)?);
                }
                let reason = finish_reason(response, !self.tools.is_empty())?;
                chunks.push(self.chunks.chunk(json!({}), reason.into()));
                chunks.extend(self.chunks.usage(usage(&response["usage"])));
                self.finished = true;
            }
            _ => {}
        }
        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chat_history_keeps_custom_tools_structured_outputs_and_multimodal_content() {
        let body = json!({"model":"custom","messages":[{"role":"system","name":"policy","content":"Be precise"},
            {"role":"user","content":[{"type":"text","text":"Read this"},{"type":"image_url","image_url":{"url":"https://example.test/image.png","detail":"original"}},{"type":"file","file":{"file_id":"file_1"}},{"type":"input_audio","input_audio":{"format":"wav","data":"AAAA"}}]},
            {"role":"assistant","content":null,"reasoning":[{"encrypted_content":"encrypted","summary":[]}],"tool_calls":[{"id":"call_1","type":"custom","custom":{"name":"code","input":"1+1","namespace":"tools"}}]},
            {"role":"tool","tool_call_id":"call_1","content":[{"type":"text","text":"2"}]}],
            "tools":[{"type":"custom","custom":{"name":"code"}}],"response_format":{"type":"json_schema","json_schema":{"name":"result","strict":true,"schema":{"type":"object"}}},"reasoning_effort":"high","verbosity":"low","web_search_options":{"search_context_size":"high"},"stop":["x"],"seed":1});
        let value = request(&body).unwrap();
        assert!(value["input"][0].get("name").is_none());
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
        assert_eq!(value["reasoning"]["effort"], "high");
        assert_eq!(value["tools"][1]["type"], "web_search");
        assert!(value.get("stop").is_none() && value.get("seed").is_none());
    }

    #[test]
    fn stream_and_sync_keep_reasoning_custom_calls_refusal_citations_and_usage() {
        let response = json!({"id":"resp_1","model":"custom","created_at":123,"status":"completed","output":[
            {"type":"reasoning","summary":[{"type":"summary_text","text":"Thought"}],"encrypted_content":"encrypted"},
            {"type":"custom_tool_call","call_id":"call_1","name":"code","namespace":"tools","input":"print(1)"},
            {"type":"message","content":[{"type":"output_text","text":"Hello","annotations":[{"type":"url_citation","url":"https://example.test","title":"example","start_index":0,"end_index":5}]},{"type":"refusal","refusal":"No"}]}],
            "usage":{"input_tokens":10,"output_tokens":4,"output_tokens_details":{"reasoning_tokens":2}}});
        let mut stream = Stream::new(true);
        let mut output = super::super::Output::default();
        let mut chunks = Vec::new();
        for event in [
            json!({"type":"response.created","response":{"id":"resp_1","model":"custom","created_at":123}}),
            json!({"type":"response.output_item.added","output_index":1,"item":{"type":"custom_tool_call","call_id":"call_1","name":"code","input":"","namespace":"tools"}}),
            json!({"type":"response.custom_tool_call_input.delta","output_index":1,"delta":"print("}),
            json!({"type":"response.completed","response":response}),
        ] {
            chunks.extend(stream.event(&event).unwrap());
        }
        assert!(
            chunks
                .iter()
                .any(|v| v["choices"][0]["delta"]["tool_calls"][0]["custom"]["input"] == "1)")
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
        for chunk in chunks {
            output.push(chunk);
        }
        let sync = output.finish().unwrap();
        let message = &sync["choices"][0]["message"];
        assert_eq!(message["content"], "Hello");
        assert_eq!(message["reasoning_content"], "Thought");
        assert_eq!(message["reasoning"][0]["encrypted_content"], "encrypted");
        assert_eq!(message["tool_calls"][0]["custom"]["input"], "print(1)");
        assert_eq!(message["tool_calls"][0]["custom"]["namespace"], "tools");
        assert_eq!(
            message["annotations"][0]["url_citation"]["url"],
            "https://example.test"
        );
        assert_eq!(sync["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(sync["usage"]["completion_tokens"], 4);
    }
}
