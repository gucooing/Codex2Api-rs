//! Protocol completion and conversion share a stream lifetime. Finish reasons can
//! precede usage and closing events; only terminal events wait for the actual end.
use super::{EventConversion, Observation, Outcome};
use crate::{
    inbound::{Error, Inbound, Result},
    supply::Wire,
};
use serde_json::Value;

pub struct Frame {
    pub observation: Observation,
    pub events: Vec<Value>,
}

pub struct Stream {
    wire: Wire,
    converter: Box<dyn EventConversion>,
    pending: Option<Outcome>,
    tail: Vec<Value>,
    ended: bool,
}

impl Stream {
    pub fn new(inbound: Inbound, wire: Wire, include_usage: bool, model: &str) -> Self {
        Self {
            wire,
            converter: super::events(inbound, wire, include_usage, model),
            pending: None,
            tail: Vec::new(),
            ended: false,
        }
    }

    pub fn event(&mut self, value: Value) -> Result<Frame> {
        let mut observation = super::observe(self.wire, &value);
        let deferred_gemini = self.wire == Wire::Gemini
            && !value.get("error").is_some_and(|v| !v.is_null())
            && !value
                .pointer("/response/error")
                .is_some_and(|v| !v.is_null());
        if !deferred_gemini
            && matches!(
                observation.outcome,
                Some(Outcome::Failed { .. } | Outcome::Cancelled)
            )
        {
            self.ended = true;
            self.tail.clear();
            return Ok(Frame {
                observation,
                events: Vec::new(),
            });
        }
        let deferred = !self.wire.responses_family();
        if deferred && observation.outcome.is_some() {
            self.pending = observation.outcome.take();
        }
        let closing = self.wire == Wire::Messages && value["type"] == "message_stop";
        let events = self.converter.event(value)?;
        if closing {
            observation.outcome = Some(self.pending.take().ok_or_else(Error::upstream_incomplete)?);
        }
        if observation.outcome.is_some() {
            self.ended = true;
            self.tail.extend(events);
            self.tail.extend(self.converter.finish()?);
            return Ok(Frame {
                observation,
                events: std::mem::take(&mut self.tail),
            });
        }
        if self.pending.is_some() {
            Ok(Frame {
                observation,
                events: self.defer_completion(events),
            })
        } else {
            Ok(Frame {
                observation,
                events,
            })
        }
    }

    /// A finish reason can share a chunk with the last text or tool arguments.
    /// Deliver that content now; hold only completion and usage until settlement.
    fn defer_completion(&mut self, events: Vec<Value>) -> Vec<Value> {
        let mut ready = Vec::new();
        for mut event in events {
            if matches!(
                event["type"].as_str(),
                Some(
                    "message_delta"
                        | "message_stop"
                        | "response.completed"
                        | "response.incomplete"
                        | "response.failed"
                        | "error"
                )
            ) {
                self.tail.push(event);
            } else if let Some(choices) = event["choices"].as_array() {
                if choices.is_empty() {
                    self.tail.push(event);
                } else if choices
                    .iter()
                    .any(|choice| choice["finish_reason"].is_string())
                {
                    let mut terminal = event.clone();
                    let mut has_delta = false;
                    for choice in event["choices"].as_array_mut().expect("choices") {
                        has_delta |= choice["delta"]
                            .as_object()
                            .is_some_and(|delta| !delta.is_empty());
                        choice["finish_reason"] = Value::Null;
                    }
                    if has_delta {
                        event.as_object_mut().expect("event").remove("usage");
                        ready.push(event);
                        for choice in terminal["choices"].as_array_mut().expect("choices") {
                            choice["delta"] = serde_json::json!({});
                            choice["logprobs"] = Value::Null;
                        }
                    }
                    self.tail.push(terminal);
                } else {
                    ready.push(event);
                }
            } else {
                ready.push(event);
            }
        }
        ready
    }

    pub fn finish(&mut self) -> Result<Frame> {
        if self.ended || !matches!(self.wire, Wire::ChatCompletions | Wire::Gemini) {
            return Err(Error::stream_incomplete());
        }
        let outcome = self.pending.take().ok_or_else(Error::stream_incomplete)?;
        self.ended = true;
        self.tail.extend(self.converter.finish()?);
        Ok(Frame {
            observation: Observation {
                outcome: Some(outcome),
                ..Default::default()
            },
            events: std::mem::take(&mut self.tail),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chat_tail_keeps_usage_and_finishes_every_downstream_protocol() {
        for inbound in [Inbound::Responses, Inbound::Chat, Inbound::Messages] {
            let mut stream = Stream::new(inbound, Wire::ChatCompletions, true, "model");
            let mut output = super::super::Collector::new(inbound);
            let chunks = [
                json!({"id":"r","model":"model","choices":[{"index":0,"delta":{"content":"hi"},"finish_reason":null}]}),
                json!({"id":"r","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}),
                json!({"id":"r","choices":[],"usage":{"prompt_tokens":100,"completion_tokens":5}}),
            ];
            let mut input = None;
            for chunk in chunks {
                let frame = stream.event(chunk).unwrap();
                assert!(frame.observation.outcome.is_none());
                if let Some(usage) = frame.observation.usage {
                    input = usage.input;
                }
                for event in frame.events {
                    output.push(event).unwrap();
                }
            }
            assert_eq!(input, Some(100));
            let frame = stream.finish().unwrap();
            assert_eq!(frame.observation.outcome, Some(Outcome::Completed));
            for event in frame.events {
                output.push(event).unwrap();
            }
            assert!(output.finish().is_ok(), "{inbound:?}");
        }
    }

    #[test]
    fn messages_require_the_stop_event_before_releasing_completion() {
        for inbound in [Inbound::Responses, Inbound::Chat, Inbound::Messages] {
            let mut stream = Stream::new(inbound, Wire::Messages, true, "model");
            let mut output = super::super::Collector::new(inbound);
            for event in [
                json!({"type":"message_start","message":{"id":"r","model":"model","role":"assistant","usage":{"input_tokens":10,"output_tokens":0}}}),
                json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"hi"}}),
                json!({"type":"content_block_stop","index":0}),
                json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2}}),
            ] {
                let frame = stream.event(event).unwrap();
                assert!(frame.observation.outcome.is_none());
                for event in frame.events {
                    output.push(event).unwrap();
                }
            }
            let frame = stream.event(json!({"type":"message_stop"})).unwrap();
            assert_eq!(frame.observation.outcome, Some(Outcome::Completed));
            for event in frame.events {
                output.push(event).unwrap();
            }
            assert!(output.finish().is_ok(), "{inbound:?}");
        }
        let mut stream = Stream::new(Inbound::Messages, Wire::Messages, true, "m");
        stream
            .event(json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}}))
            .unwrap();
        assert!(stream.finish().is_err());
    }
}
