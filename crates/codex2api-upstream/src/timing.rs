//! Request clocks and upstream token observation shared by every execution surface.
//! Transport arrival is measured before quota writes, pool prelude buffering and output conversion.
use std::{
    future::Future,
    sync::{Arc, OnceLock},
    time::Instant,
};

use futures::StreamExt;
use serde_json::Value;

tokio::task_local! { static REQUEST_CLOCK: RequestClock; }

#[derive(Clone, Copy, Debug)]
pub struct RequestClock {
    pub started: Instant,
    pub requested_at_ms: i64,
}
impl Default for RequestClock {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            requested_at_ms: chrono::Utc::now().timestamp_millis(),
        }
    }
}
impl RequestClock {
    pub fn current() -> Option<Self> {
        REQUEST_CLOCK.try_with(|clock| *clock).ok()
    }
    pub async fn scope<F: Future>(self, work: F) -> F::Output {
        REQUEST_CLOCK.scope(self, work).await
    }
    pub fn inherit<F: Future>(work: F) -> impl Future<Output = F::Output> {
        let clock = Self::current();
        async move {
            match clock {
                Some(clock) => clock.scope(work).await,
                None => work.await,
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TokenProtocol {
    Responses,
    ChatCompletions,
    Messages,
}

#[derive(Clone, Debug)]
pub struct ResponseTiming {
    sent_at: Instant,
    first_token_at: Arc<OnceLock<Instant>>,
}
impl Default for ResponseTiming {
    fn default() -> Self {
        Self {
            sent_at: Instant::now(),
            first_token_at: Arc::new(OnceLock::new()),
        }
    }
}
impl ResponseTiming {
    pub fn first_token_ms(&self, started: Instant) -> Option<i64> {
        self.first_token_at.get().map(|at| {
            at.saturating_duration_since(started)
                .as_millis()
                .min(i64::MAX as u128) as i64
        })
    }
    pub fn supplier_ttft_ms(&self) -> Option<f64> {
        self.first_token_at
            .get()
            .map(|at| at.saturating_duration_since(self.sent_at).as_secs_f64() * 1000.0)
    }
    pub(crate) fn observe(&self, protocol: TokenProtocol, value: &Value, received: Instant) {
        if self.first_token_at.get().is_none() && has_tokens(protocol, value) {
            let _ = self.first_token_at.set(received);
        }
    }

    /// Preserve exact response bytes and errors; JSON-only responses have no observable TTFT.
    pub fn wrap(
        self,
        mut response: reqwest::Response,
        protocol: TokenProtocol,
    ) -> reqwest::Response {
        let status = response.status();
        let version = response.version();
        let headers = response.headers().clone();
        let mut extensions = std::mem::take(response.extensions_mut());
        extensions.insert(self.clone());
        let stream = async_stream::stream! {
            let mut source = response.bytes_stream();
            let mut parser = TokenStream::default();
            while let Some(chunk) = source.next().await {
                if status.is_success() && self.first_token_at.get().is_none() && let Ok(bytes) = &chunk {
                    parser.push(bytes, &self, protocol, Instant::now());
                }
                yield chunk;
            }
        };
        let mut response = http::Response::new(reqwest::Body::wrap_stream(stream));
        *response.status_mut() = status;
        *response.version_mut() = version;
        *response.headers_mut() = headers;
        *response.extensions_mut() = extensions;
        response.into()
    }
}

fn nonempty(value: &Value) -> bool {
    value.as_str().is_some_and(|text| !text.is_empty())
}
fn content(value: &Value) -> bool {
    nonempty(value)
        || value.as_array().is_some_and(|parts| {
            parts.iter().any(|part| {
                matches!(
                    part["type"].as_str(),
                    Some(
                        "text"
                            | "output_text"
                            | "reasoning_text"
                            | "summary_text"
                            | "thinking"
                            | "refusal"
                    )
                ) && ["text", "thinking", "refusal"]
                    .iter()
                    .any(|key| nonempty(&part[*key]))
            })
        })
}
fn response_item(item: &Value) -> bool {
    match item["type"].as_str() {
        Some("message") => content(&item["content"]),
        Some("reasoning") => content(&item["summary"]) || content(&item["content"]),
        Some("function_call") => nonempty(&item["arguments"]),
        Some("custom_tool_call") => nonempty(&item["input"]),
        _ => false,
    }
}
fn has_tokens(protocol: TokenProtocol, value: &Value) -> bool {
    match protocol {
        TokenProtocol::Responses => match value["type"].as_str() {
            Some(
                "response.output_text.delta"
                | "response.reasoning_text.delta"
                | "response.reasoning_summary_text.delta"
                | "response.refusal.delta"
                | "response.function_call_arguments.delta"
                | "response.custom_tool_call_input.delta",
            ) => nonempty(&value["delta"]),
            Some(
                "response.output_text.done"
                | "response.reasoning_text.done"
                | "response.reasoning_summary_text.done",
            ) => nonempty(&value["text"]),
            Some("response.output_item.added" | "response.output_item.done") => {
                response_item(&value["item"])
            }
            Some("response.completed" | "response.incomplete" | "response.failed") => {
                value["response"]["output"]
                    .as_array()
                    .is_some_and(|items| items.iter().any(response_item))
            }
            _ => false,
        },
        TokenProtocol::ChatCompletions => value["choices"].as_array().is_some_and(|choices| {
            choices.iter().any(|choice| {
                let delta = &choice["delta"];
                ["content", "reasoning", "reasoning_content", "refusal"]
                    .iter()
                    .any(|key| content(&delta[*key]))
                    || nonempty(&delta["function_call"]["arguments"])
                    || delta["tool_calls"].as_array().is_some_and(|calls| {
                        calls
                            .iter()
                            .any(|call| nonempty(&call["function"]["arguments"]))
                    })
            })
        }),
        TokenProtocol::Messages => match value["type"].as_str() {
            Some("content_block_delta") => ["text", "thinking", "partial_json"]
                .iter()
                .any(|key| nonempty(&value["delta"][*key])),
            Some("content_block_start") => ["text", "thinking"]
                .iter()
                .any(|key| nonempty(&value["content_block"][*key])),
            _ => false,
        },
    }
}

#[derive(Default)]
struct TokenStream {
    prefix: Vec<u8>,
    format: Option<crate::ResponseFormat>,
    line: Vec<u8>,
    data: Vec<u8>,
    event: Option<String>,
    after_cr: bool,
    skipped: bool,
}
impl TokenStream {
    fn push(
        &mut self,
        bytes: &[u8],
        timing: &ResponseTiming,
        protocol: TokenProtocol,
        received: Instant,
    ) {
        if self.format.is_none() {
            self.prefix.extend_from_slice(bytes);
            self.format = crate::ResponseFormat::from_prefix(&self.prefix);
            if self.format.is_none() && self.prefix.len() > 1024 {
                self.format = Some(crate::ResponseFormat::Other);
            }
            if self.format.is_none() {
                return;
            }
            let prefix = std::mem::take(&mut self.prefix);
            self.push(
                prefix.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&prefix),
                timing,
                protocol,
                received,
            );
            return;
        }
        if self.format != Some(crate::ResponseFormat::EventStream) {
            return;
        }
        for &byte in bytes {
            if timing.first_token_at.get().is_some() {
                self.line.clear();
                self.data.clear();
                return;
            }
            if byte == b'\n' && self.after_cr {
                self.after_cr = false;
                continue;
            }
            self.after_cr = byte == b'\r';
            if byte == b'\r' || byte == b'\n' {
                if self.line.is_empty() {
                    if !self.skipped
                        && let Ok(mut value) = serde_json::from_slice::<Value>(&self.data)
                    {
                        if value.get("type").is_none()
                            && let Some(event) = &self.event
                            && value.is_object()
                        {
                            value["type"] = event.clone().into();
                        }
                        timing.observe(protocol, &value, received);
                    }
                    self.data.clear();
                    self.event = None;
                    self.skipped = false;
                } else if !self.skipped {
                    if let Some(data) = self.line.strip_prefix(b"data:") {
                        if !self.data.is_empty() {
                            self.data.push(b'\n');
                        }
                        self.data
                            .extend_from_slice(data.strip_prefix(b" ").unwrap_or(data));
                    } else if let Some(event) = self.line.strip_prefix(b"event:") {
                        self.event = std::str::from_utf8(event).ok().map(|s| s.trim().to_owned());
                    }
                }
                self.line.clear();
            } else if self.line.len() + self.data.len() < crate::MAX_REQUEST_BYTES {
                self.line.push(byte);
            } else {
                self.skipped = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn ttft_counts_only_nonempty_generated_content_for_all_wire_protocols() {
        let cases = [
            (
                TokenProtocol::Responses,
                vec![
                    json!({"type":"response.created"}),
                    json!({"type":"response.in_progress"}),
                    json!({"type":"codex.rate_limits"}),
                    json!({"type":"response.output_item.added","item":{"type":"function_call","name":"tool","arguments":""}}),
                    json!({"type":"response.output_text.delta","delta":""}),
                    json!({"type":"response.completed","response":{"usage":{"output_tokens":100}}}),
                ],
                vec![
                    json!({"type":"response.output_text.delta","delta":" "}),
                    json!({"type":"response.reasoning_summary_text.delta","delta":"think"}),
                    json!({"type":"response.function_call_arguments.delta","delta":"{"}),
                    json!({"type":"response.refusal.delta","delta":"No"}),
                    json!({"type":"response.completed","response":{"output":[{"type":"message","content":[{"type":"output_text","text":"Hello"}]}]}}),
                ],
            ),
            (
                TokenProtocol::ChatCompletions,
                vec![
                    json!({"choices":[{"delta":{"role":"assistant","content":""}}]}),
                    json!({"choices":[],"usage":{"completion_tokens":20}}),
                ],
                vec![
                    json!({"choices":[{"delta":{"content":"Hello"}}]}),
                    json!({"choices":[{"delta":{"reasoning_content":"think"}}]}),
                    json!({"choices":[{"delta":{"tool_calls":[{"function":{"arguments":"{"}}]}}]}),
                ],
            ),
            (
                TokenProtocol::Messages,
                vec![
                    json!({"type":"message_start","message":{"role":"assistant"}}),
                    json!({"type":"content_block_delta","delta":{"text":""}}),
                    json!({"type":"message_stop"}),
                ],
                vec![
                    json!({"type":"content_block_delta","delta":{"text":"Hello"}}),
                    json!({"type":"content_block_delta","delta":{"thinking":"think"}}),
                    json!({"type":"content_block_delta","delta":{"partial_json":"{"}}),
                ],
            ),
        ];
        for (protocol, empty, outputs) in cases {
            for output in outputs {
                let timing = ResponseTiming::default();
                let request_started = timing.sent_at - Duration::from_millis(80);
                for event in &empty {
                    timing.observe(protocol, event, timing.sent_at + Duration::from_millis(10));
                }
                assert_eq!(timing.first_token_ms(request_started), None, "{protocol:?}");
                timing.observe(
                    protocol,
                    &output,
                    timing.sent_at + Duration::from_millis(120),
                );
                timing.observe(
                    protocol,
                    &output,
                    timing.sent_at + Duration::from_millis(600),
                );
                assert_eq!(
                    timing.first_token_ms(request_started),
                    Some(200),
                    "{protocol:?} {output}"
                );
                assert_eq!(timing.supplier_ttft_ms(), Some(120.0));
            }
        }
    }

    #[test]
    fn sse_fragmentation_and_named_events_keep_the_first_token_timestamp() {
        let prelude = "\u{feff}: heartbeat\r\nevent: response.created\r\ndata: {}\r\n\r\n";
        let token =
            "event: response.output_text.delta\r\ndata: {\"delta\":\r\ndata: \"中文🙂\"}\r\n\r\n";
        for size in [1, 2, 7, 1000] {
            let timing = ResponseTiming::default();
            let mut parser = TokenStream::default();
            for part in prelude.as_bytes().chunks(size) {
                parser.push(
                    part,
                    &timing,
                    TokenProtocol::Responses,
                    timing.sent_at + Duration::from_millis(10),
                );
            }
            assert_eq!(timing.supplier_ttft_ms(), None);
            for part in token.as_bytes().chunks(size) {
                parser.push(
                    part,
                    &timing,
                    TokenProtocol::Responses,
                    timing.sent_at + Duration::from_millis(75),
                );
            }
            assert_eq!(timing.supplier_ttft_ms(), Some(75.0), "chunk size {size}");
        }
        let timing = ResponseTiming::default();
        let mut parser = TokenStream::default();
        parser.push(br#"{"output":[{"type":"message","content":[{"type":"output_text","text":"complete JSON"}]}]}"#, &timing, TokenProtocol::Responses, timing.sent_at);
        assert_eq!(timing.supplier_ttft_ms(), None);
    }

    #[tokio::test]
    async fn observation_does_not_buffer_the_stream_or_treat_created_as_a_token() {
        let (sender, receiver) = tokio::sync::mpsc::channel::<std::io::Result<bytes::Bytes>>(4);
        let response: reqwest::Response = http::Response::builder()
            .header("content-type", "text/event-stream")
            .body(reqwest::Body::wrap_stream(
                tokio_stream::wrappers::ReceiverStream::new(receiver),
            ))
            .unwrap()
            .into();
        let timing = ResponseTiming::default();
        let mut response = timing.clone().wrap(response, TokenProtocol::Responses);
        let created = bytes::Bytes::from_static(b"data: {\"type\":\"response.created\"}\n\n");
        sender.send(Ok(created.clone())).await.unwrap();
        assert_eq!(response.chunk().await.unwrap().unwrap(), created);
        assert_eq!(timing.supplier_ttft_ms(), None);
        let delta = bytes::Bytes::from_static(
            b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"x\"}\n\n",
        );
        sender.send(Ok(delta.clone())).await.unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), response.chunk())
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
            delta
        );
        assert!(timing.supplier_ttft_ms().is_some());
        sender
            .send(Err(std::io::Error::other("fixture stream error")))
            .await
            .unwrap();
        assert!(response.chunk().await.is_err());
    }

    #[tokio::test]
    async fn request_clocks_survive_workers_without_leaking_between_requests() {
        let first = RequestClock::default();
        let second = RequestClock {
            started: first.started + Duration::from_secs(1),
            requested_at_ms: first.requested_at_ms + 1000,
        };
        let one = first.scope(async {
            tokio::spawn(RequestClock::inherit(async {
                RequestClock::current().unwrap()
            }))
            .await
            .unwrap()
        });
        let two = second.scope(async {
            tokio::spawn(RequestClock::inherit(async {
                RequestClock::current().unwrap()
            }))
            .await
            .unwrap()
        });
        let (one, two) = tokio::join!(one, two);
        assert_eq!(one.started, first.started);
        assert_eq!(two.started, second.started);
        assert!(RequestClock::current().is_none());
    }
}
