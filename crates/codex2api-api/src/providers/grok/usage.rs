//! Grok's native Responses, Chat Completions and Messages usage readers.
//! Retains only model/usage/termination metadata; content stays in the forwarded stream.
use serde_json::{Value, json};
#[derive(Default)]
pub struct Observer {
    model: Option<String>,
    usage: Value,
    finished: bool,
    chat_finished: bool,
}
impl Observer {
    fn complete(&mut self) -> Value {
        self.finished = true;
        json!({"type":"response.completed","response":{"model":self.model,"status":"completed","usage":self.usage}})
    }
    fn message_usage(&mut self, value: &Value) {
        if !value.is_object() {
            return;
        }
        if let Some(input) = value["input_tokens"].as_i64() {
            let read = value["cache_read_input_tokens"].as_i64();
            let write = value["cache_creation_input_tokens"].as_i64();
            self.usage["input_tokens"] = json!(
                input
                    .saturating_add(read.unwrap_or(0))
                    .saturating_add(write.unwrap_or(0))
            );
            self.usage["input_tokens_details"] =
                json!({"cached_tokens":read,"cache_write_tokens":write});
        }
        if let Some(output) = value["output_tokens"].as_i64() {
            self.usage["output_tokens"] = output.into();
        }
    }
}
impl crate::usage::ProtocolObserver for Observer {
    fn observe(&mut self, bytes: &[u8]) -> Vec<Value> {
        if self.finished {
            return vec![];
        }
        if bytes.trim_ascii() == b"[DONE]" {
            return if self.chat_finished {
                vec![self.complete()]
            } else {
                vec![]
            };
        }
        let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
            return vec![];
        };
        if value["type"]
            .as_str()
            .is_some_and(|t| t.starts_with("response."))
        {
            return vec![value];
        }
        if value["object"] == "response"
            || (value.get("output").is_some() && value["status"].is_string())
        {
            let kind = match value["status"].as_str() {
                Some("completed") => "response.completed",
                Some("failed") => "response.failed",
                Some("incomplete") => "response.incomplete",
                _ => "response.in_progress",
            };
            return vec![json!({"type":kind,"response":value})];
        }
        if value.get("error").is_some_and(|v| !v.is_null()) || value["type"] == "error" {
            self.finished = true;
            let error = codex2api_upstream::grok::errors::normalize(&value);
            return vec![
                json!({"type":"response.failed","response":{"model":self.model,"usage":self.usage,"error":error}}),
            ];
        }
        let model = value["model"]
            .as_str()
            .or_else(|| value["message"]["model"].as_str());
        if let Some(model) = model {
            self.model = Some(model.to_owned());
        }
        if value.get("choices").is_some() {
            let usage = &value["usage"];
            if usage.is_object() {
                self.usage = json!({"input_tokens":usage["prompt_tokens"],"output_tokens":usage["completion_tokens"],"input_tokens_details":usage["prompt_tokens_details"],"output_tokens_details":usage["completion_tokens_details"]});
            }
            self.chat_finished |= value["choices"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|c| !c["finish_reason"].is_null()));
            if value["object"] == "chat.completion" {
                return vec![self.complete()];
            }
            return vec![
                json!({"type":"response.output_text.delta","response":{"model":self.model,"usage":self.usage}}),
            ];
        }
        match value["type"].as_str() {
            Some("message_start") => {
                self.usage = json!({});
                self.message_usage(&value["message"]["usage"]);
            }
            Some("message_delta") => {
                self.message_usage(&value["usage"]);
            }
            Some("message_stop") => return vec![self.complete()],
            Some("message") => {
                self.usage = json!({});
                self.message_usage(&value["usage"]);
                return vec![self.complete()];
            }
            _ => {}
        }
        vec![
            json!({"type":"response.in_progress","response":{"model":self.model,"usage":self.usage}}),
        ]
    }
}
pub fn prelude(value: &Value) -> bool {
    if matches!(value["type"].as_str(), Some("message_start" | "ping")) {
        return true;
    }
    if value["object"] == "chat.completion.chunk" {
        return value["choices"].as_array().is_some_and(|rows| {
            rows.iter().all(|c| {
                c["delta"]["content"].as_str().is_none_or(str::is_empty)
                    && c["delta"]["tool_calls"].is_null()
                    && c["delta"]["reasoning_content"]
                        .as_str()
                        .is_none_or(str::is_empty)
                    && c["finish_reason"].is_null()
            })
        });
    }
    crate::pool_execution::prelude_event(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::ProtocolObserver;
    #[test]
    fn chat_completion_waits_for_usage_trailer_before_completion() {
        let mut observer = Observer::default();
        observer.observe(br#"{"object":"chat.completion.chunk","model":"actual","choices":[{"finish_reason":"stop"}]}"#);
        observer.observe(br#"{"object":"chat.completion.chunk","choices":[],"usage":{"prompt_tokens":100,"completion_tokens":20,"prompt_tokens_details":{"cached_tokens":60},"completion_tokens_details":{"reasoning_tokens":5}}}"#);
        let end = observer.observe(b"[DONE]").remove(0);
        assert_eq!(end["response"]["usage"]["input_tokens"], 100);
        assert_eq!(end["response"]["usage"]["output_tokens"], 20);
        assert_eq!(
            end["response"]["usage"]["input_tokens_details"]["cached_tokens"],
            60
        );
        assert!(observer.observe(b"[DONE]").is_empty());
    }
    #[test]
    fn messages_adds_separate_cache_categories_once_and_merges_output_delta() {
        let mut observer = Observer::default();
        observer.observe(br#"{"type":"message_start","message":{"model":"actual","usage":{"input_tokens":10,"cache_read_input_tokens":40,"cache_creation_input_tokens":20,"output_tokens":1}}}"#);
        observer.observe(br#"{"type":"message_delta","usage":{"output_tokens":30}}"#);
        let end = observer.observe(br#"{"type":"message_stop"}"#).remove(0);
        assert_eq!(end["response"]["usage"]["input_tokens"], 70);
        assert_eq!(end["response"]["usage"]["output_tokens"], 30);
        assert_eq!(
            end["response"]["usage"]["input_tokens_details"]["cache_write_tokens"],
            20
        );
    }
}
