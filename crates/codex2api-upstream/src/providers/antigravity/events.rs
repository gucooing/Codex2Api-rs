//! Gemini streamGenerateContent chunks become Responses events, so the shared ledger,
//! outcome, timing and caller conversions never see the Gemini wire format.
use super::errors;
use bytes::Bytes;
use eventsource_stream::Eventsource;
use futures::StreamExt;
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
    let count = |key: &str| meta[key].as_u64().unwrap_or(0);
    let input = count("promptTokenCount");
    let thoughts = count("thoughtsTokenCount");
    let output = count("candidatesTokenCount") + thoughts;
    json!({"input_tokens":input,"input_tokens_details":{"cached_tokens":count("cachedContentTokenCount")},
        "output_tokens":output,"output_tokens_details":{"reasoning_tokens":thoughts},
        "total_tokens":meta["totalTokenCount"].as_u64().unwrap_or(input+output)})
}

pub(crate) struct Converter {
    id: String,
    model: String,
    created_at: i64,
    started: bool,
    pub(crate) finished: bool,
    items: Vec<Item>,
    open: bool,
    usage: Option<Value>,
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
            usage: None,
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
                json!({"type":"response.content_part.added","output_index":index,"content_index":0,
                "part":{"type":"output_text","text":"","annotations":[]}}),
            );
            for annotation in std::mem::take(&mut self.pending_annotations) {
                self.annotate(annotation, out);
            }
        } else {
            if added["type"] == "function_call" {
                added["status"] = "in_progress".into();
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
            Item::Reasoning { text, .. } => {
                if !text.is_empty() {
                    out.push(json!({"type":"response.reasoning_summary_text.done","output_index":index,"summary_index":0,"text":text}));
                }
            }
            Item::Message { text, .. } => {
                out.push(json!({"type":"response.output_text.done","output_index":index,"content_index":0,"text":text}));
                out.push(json!({"type":"response.content_part.done","output_index":index,"content_index":0,
                    "part":item.value()["content"][0]}));
            }
            Item::FunctionCall { arguments, .. } => {
                out.push(json!({"type":"response.function_call_arguments.done","output_index":index,"arguments":arguments}));
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

    /// A signature on a non-thought part belongs to the reasoning that produced it;
    /// without one it travels as an empty reasoning item so callers can replay it.
    fn carry_signature(&mut self, signature: Option<String>, out: &mut Vec<Value>) {
        let Some(signature) = signature else {
            return;
        };
        if self.open_reasoning()
            && let Some(Item::Reasoning {
                signature: slot, ..
            }) = self.items.last_mut()
            && slot.is_none()
        {
            *slot = Some(signature);
            return;
        }
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
        out.push(json!({"type":"response.output_text.delta","output_index":index,"content_index":0,"delta":text}));
    }

    fn annotate(&mut self, mut annotation: Value, out: &mut Vec<Value>) {
        let index = self.index();
        let Some(Item::Message {
            text, annotations, ..
        }) = self.items.last_mut()
        else {
            return;
        };
        let offset = text.chars().count();
        annotation["start_index"] = offset.into();
        annotation["end_index"] = offset.into();
        let position = annotations.len();
        annotations.push(annotation.clone());
        out.push(json!({"type":"response.output_text.annotation.added","output_index":index,"content_index":0,
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
            out.push(json!({"type":"response.function_call_arguments.delta","output_index":index,"delta":arguments}));
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
                text: buffer,
                signature: slot,
                ..
            }) = self.items.last_mut()
            {
                if let Some(text) = part["text"].as_str().filter(|t| !t.is_empty()) {
                    buffer.push_str(text);
                    out.push(json!({"type":"response.reasoning_summary_text.delta","output_index":index,"summary_index":0,"delta":text}));
                }
                if signature.is_some() {
                    *slot = signature;
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
        if let Some(usage) = &self.usage {
            response["usage"] = usage.clone();
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
            let normalized =
                errors::normalize(status, json!({"error":error}).to_string().as_bytes());
            return self.fail(normalized["error"].clone(), out);
        }
        if let Some(meta) = body.get("usageMetadata").filter(|u| u.is_object()) {
            self.usage = Some(usage(meta));
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

    /// The terminal event once the upstream stream ends. A stream that never
    /// reported a finish reason yields none, which the caller treats as incomplete.
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
            return out;
        };
        let (kind, status, incomplete) = match reason.as_str() {
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
            _ => ("response.completed", "completed", None),
        };
        let mut response = self.response(status);
        response["output"] = self.items.iter().map(Item::value).collect();
        if let Some(reason) = incomplete {
            response["incomplete_details"] = json!({"reason":reason});
        }
        if let Some(usage) = &self.usage {
            response["usage"] = usage.clone();
        }
        out.push(json!({"type":kind,"response":response}));
        self.finished = true;
        out
    }
}

fn frame(value: &Value) -> Bytes {
    let mut buffer = Vec::with_capacity(128);
    buffer.extend_from_slice(b"data: ");
    serde_json::to_writer(&mut buffer, value).expect("JSON value");
    buffer.extend_from_slice(b"\n\n");
    Bytes::from(buffer)
}

/// The upstream SSE body re-emitted as a Responses event stream.
pub(crate) fn responses_stream(upstream: reqwest::Response, model: String) -> reqwest::Response {
    let stream = async_stream::stream! {
        let mut events = Box::pin(crate::buffered_sse_lines(upstream.bytes_stream()).eventsource());
        let mut converter = Converter::new(model);
        while let Some(event) = events.next().await {
            let event = match event {
                Ok(event) => event,
                Err(error) => {
                    yield Err(std::io::Error::other(error.to_string()));
                    return;
                }
            };
            if event.data.trim().is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(&event.data) else {
                yield Err(std::io::Error::other("Antigravity stream chunk was not JSON"));
                return;
            };
            for event in converter.push(value) {
                yield Ok::<_, std::io::Error>(frame(&event));
            }
            if converter.finished {
                break;
            }
        }
        for event in converter.finish() {
            yield Ok(frame(&event));
        }
    };
    let mut response = http::Response::new(reqwest::Body::wrap_stream(stream));
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("text/event-stream"),
    );
    response.into()
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
