//! Chat Completions output: chunk framing and the synchronous collector.
pub mod responses;

use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// Chunk envelope shared by converted streams.
#[derive(Default)]
pub(crate) struct Chunks {
    pub id: Value,
    pub model: Value,
    pub created: Value,
    pub service_tier: Option<Value>,
    pub include_usage: bool,
}
impl Chunks {
    pub fn chunk(&self, delta: Value, finish: Value) -> Value {
        let mut value = json!({"id":self.id,"object":"chat.completion.chunk","created":self.created,"model":self.model,"choices":[{"index":0,"delta":delta,"finish_reason":finish,"logprobs":null}]});
        if self.include_usage {
            value["usage"] = Value::Null;
        }
        if let Some(tier) = &self.service_tier {
            value["service_tier"] = tier.clone();
        }
        value
    }
    pub fn usage(&self, usage: Value) -> Option<Value> {
        self.include_usage.then(|| {
            let mut chunk = self.chunk(json!({}), Value::Null);
            chunk["choices"] = json!([]);
            chunk["usage"] = usage;
            chunk
        })
    }
}

#[derive(Default)]
struct ToolCall {
    fields: Map<String, Value>,
    kind: Option<String>,
    payload: Map<String, Value>,
    text: String,
}

/// Assembles `chat.completion` from Chat chunks, including converted extensions.
/// Text grows in place; values are built once when the reply completes.
#[derive(Default)]
pub struct Output {
    envelope: Map<String, Value>,
    message: Map<String, Value>,
    text: BTreeMap<&'static str, String>,
    audio: Map<String, Value>,
    audio_text: BTreeMap<&'static str, String>,
    tools: BTreeMap<u64, ToolCall>,
    logprobs: Vec<Value>,
    finish: Value,
    usage: Value,
}
impl Output {
    pub(crate) fn push(&mut self, mut chunk: Value) {
        let Some(object) = chunk.as_object_mut() else {
            return;
        };
        for key in [
            "id",
            "model",
            "created",
            "service_tier",
            "system_fingerprint",
        ] {
            if let Some(value) = object.remove(key).filter(|v| !v.is_null()) {
                self.envelope.insert(key.into(), value);
            }
        }
        if let Some(usage) = object.remove("usage").filter(Value::is_object) {
            self.usage = usage;
        }
        let Some(Value::Array(choices)) = object.remove("choices") else {
            return;
        };
        for mut choice in choices {
            if !choice["finish_reason"].is_null() {
                self.finish = choice["finish_reason"].take();
            }
            if let Some(Value::Array(content)) =
                choice.pointer_mut("/logprobs/content").map(Value::take)
            {
                self.logprobs.extend(content);
            }
            let Some(Value::Object(delta)) = choice.get_mut("delta").map(Value::take) else {
                continue;
            };
            for (key, value) in delta {
                match (key.as_str(), value) {
                    ("content", Value::String(text)) => self.append("content", &text),
                    ("reasoning_content", Value::String(text)) => {
                        self.append("reasoning_content", &text)
                    }
                    ("refusal", Value::String(text)) => self.append("refusal", &text),
                    ("reasoning" | "annotations", Value::Array(items)) => self
                        .message
                        .entry(key)
                        .or_insert_with(|| json!([]))
                        .as_array_mut()
                        .expect("array")
                        .extend(items),
                    ("audio", Value::Object(audio)) => {
                        for (field, value) in audio {
                            match (field.as_str(), value) {
                                ("data", Value::String(text)) => {
                                    self.audio_text.entry("data").or_default().push_str(&text)
                                }
                                ("transcript", Value::String(text)) => self
                                    .audio_text
                                    .entry("transcript")
                                    .or_default()
                                    .push_str(&text),
                                (_, value) => {
                                    self.audio.insert(field, value);
                                }
                            }
                        }
                    }
                    ("tool_calls", Value::Array(calls)) => {
                        for call in calls {
                            self.tool_call(call);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    fn append(&mut self, key: &'static str, text: &str) {
        self.text.entry(key).or_default().push_str(text);
    }
    fn tool_call(&mut self, call: Value) {
        let Value::Object(call) = call else { return };
        let index = call.get("index").and_then(Value::as_u64).unwrap_or(0);
        let entry = self.tools.entry(index).or_default();
        for (key, value) in call {
            match (key.as_str(), value) {
                ("index", _) => {}
                ("function" | "custom", Value::Object(payload)) => {
                    entry.kind = Some(key);
                    for (field, value) in payload {
                        match (field.as_str(), value) {
                            ("arguments" | "input", Value::String(text)) => {
                                entry.text.push_str(&text)
                            }
                            (_, Value::String(text)) if text.is_empty() => {}
                            (_, value) => {
                                entry.payload.insert(field, value);
                            }
                        }
                    }
                }
                (_, Value::Null) => {}
                (_, Value::String(text)) if text.is_empty() => {}
                (_, value) => {
                    entry.fields.insert(key, value);
                }
            }
        }
    }
    pub(crate) fn finish(mut self) -> super::Result<Value> {
        if self.finish.is_null() {
            return Err(super::Error::upstream_incomplete());
        }
        self.message.insert("role".into(), "assistant".into());
        for (key, text) in std::mem::take(&mut self.text) {
            self.message.insert(key.into(), text.into());
        }
        for key in ["content", "refusal"] {
            self.message.entry(key).or_insert(Value::Null);
        }
        for (key, text) in self.audio_text {
            self.audio.insert(key.into(), text.into());
        }
        if !self.audio.is_empty() {
            self.message.insert("audio".into(), self.audio.into());
        }
        if !self.tools.is_empty() {
            let calls = self
                .tools
                .into_values()
                .map(|mut call| {
                    let kind = call.kind.unwrap_or_else(|| "function".into());
                    let field = if kind == "custom" {
                        "input"
                    } else {
                        "arguments"
                    };
                    call.payload.insert(field.into(), call.text.into());
                    call.fields.insert(kind, call.payload.into());
                    Value::Object(call.fields)
                })
                .collect::<Vec<_>>();
            self.message.insert("tool_calls".into(), calls.into());
        }
        let logprobs = if self.logprobs.is_empty() {
            Value::Null
        } else {
            json!({"content":self.logprobs,"refusal":null})
        };
        let mut result = Value::Object(self.envelope);
        result["object"] = "chat.completion".into();
        result["choices"] = json!([{"index":0,"message":self.message,"finish_reason":self.finish,"logprobs":logprobs}]);
        result["usage"] = self.usage;
        Ok(result)
    }
}
