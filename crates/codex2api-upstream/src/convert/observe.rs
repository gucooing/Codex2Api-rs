//! Reading a wire's events for the ledger: usage, the reported model and the
//! terminal outcome, in the wire's own vocabulary. Downstream ledgers never parse a
//! wire themselves.
use crate::supply::Wire;
use serde_json::Value;

/// A generation's terminal state as one wire reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    /// The generation stopped early; `reason` is the Responses incomplete reason.
    Incomplete {
        reason: Option<String>,
    },
    /// The wire reported a failure; `body` is its error in the wire's shape.
    Failed {
        status: Option<u16>,
        body: Value,
    },
    Cancelled,
}

/// Token counts in the Responses vocabulary, as far as the wire reported them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub cached: Option<i64>,
    pub cache_write: Option<i64>,
    pub reasoning: Option<i64>,
}
impl Usage {
    pub fn any(&self) -> bool {
        self.input.is_some() || self.output.is_some()
    }
    fn count(value: &Value) -> Option<i64> {
        value.as_i64().filter(|n| *n >= 0)
    }
    fn responses(usage: &Value) -> Self {
        Self {
            input: Self::count(&usage["input_tokens"]),
            output: Self::count(&usage["output_tokens"]),
            cached: Self::count(&usage["input_tokens_details"]["cached_tokens"]),
            cache_write: Self::count(&usage["input_tokens_details"]["cache_write_tokens"]),
            reasoning: Self::count(&usage["output_tokens_details"]["reasoning_tokens"]),
        }
    }
    fn chat(usage: &Value) -> Self {
        Self {
            input: Self::count(&usage["prompt_tokens"]),
            output: Self::count(&usage["completion_tokens"]),
            cached: Self::count(&usage["prompt_tokens_details"]["cached_tokens"]),
            cache_write: Self::count(&usage["prompt_tokens_details"]["cache_write_tokens"]),
            reasoning: Self::count(&usage["completion_tokens_details"]["reasoning_tokens"]),
        }
    }
    /// Messages counts uncached input separately from cache reads and writes.
    fn messages(usage: &Value) -> Self {
        let cached = Self::count(&usage["cache_read_input_tokens"]);
        let written = Self::count(&usage["cache_creation_input_tokens"]);
        Self {
            input: Self::count(&usage["input_tokens"])
                .map(|n| n + cached.unwrap_or(0) + written.unwrap_or(0)),
            output: Self::count(&usage["output_tokens"]),
            cached,
            cache_write: written,
            reasoning: None,
        }
    }
    /// Gemini counts cached prompt tokens inside the prompt total and thoughts
    /// outside the candidate total.
    fn gemini(meta: &Value) -> Self {
        let thoughts = Self::count(&meta["thoughtsTokenCount"]);
        Self {
            input: Self::count(&meta["promptTokenCount"]),
            output: Self::count(&meta["candidatesTokenCount"])
                .map(|n| n.saturating_add(thoughts.unwrap_or(0))),
            cached: Self::count(&meta["cachedContentTokenCount"]),
            cache_write: None,
            reasoning: thoughts,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Observation {
    pub usage: Option<Usage>,
    pub model: Option<String>,
    pub outcome: Option<Outcome>,
    /// Output or usage was visible: a replay would duplicate it.
    pub observed: bool,
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 256)
        .map(str::to_owned)
}

/// Read one wire event (or a JSON reply) for the ledger.
pub fn observe(wire: Wire, value: &Value) -> Observation {
    let mut observation = Observation {
        observed: crate::executor::generation_observed(value),
        ..Default::default()
    };
    match wire {
        Wire::Codex | Wire::Xai | Wire::OpenAiResponses => {
            let response = value.get("response").unwrap_or(value);
            observation.model = text(&response["model"]);
            if response["usage"].is_object() {
                observation.usage = Some(Usage::responses(&response["usage"]));
            }
            observation.outcome = match crate::ResponseOutcome::from_value(value, None) {
                Some(crate::ResponseOutcome::Completed) => Some(Outcome::Completed),
                Some(crate::ResponseOutcome::Incomplete(failure)) => Some(Outcome::Incomplete {
                    reason: failure.code,
                }),
                Some(crate::ResponseOutcome::Failed(failure)) => Some(Outcome::Failed {
                    status: failure.status,
                    body: value.clone(),
                }),
                Some(crate::ResponseOutcome::Cancelled) => Some(Outcome::Cancelled),
                None => None,
            };
        }
        Wire::ChatCompletions => {
            observation.model = text(&value["model"]);
            if value["usage"].is_object() {
                observation.usage = Some(Usage::chat(&value["usage"]));
            }
            if value.get("error").is_some_and(|e| !e.is_null()) {
                observation.outcome = Some(Outcome::Failed {
                    status: crate::ResponseFailure::from_error(None, value).status,
                    body: value.clone(),
                });
            } else if let Some(reason) = value["choices"]
                .as_array()
                .into_iter()
                .flatten()
                .find_map(|choice| choice["finish_reason"].as_str())
            {
                observation.outcome = Some(match reason {
                    "stop" | "tool_calls" | "function_call" => Outcome::Completed,
                    "length" => Outcome::Incomplete {
                        reason: Some("max_output_tokens".into()),
                    },
                    "content_filter" => Outcome::Incomplete {
                        reason: Some("content_filter".into()),
                    },
                    _ => Outcome::Completed,
                });
            }
        }
        Wire::Messages => match value["type"].as_str() {
            Some("message_start") => {
                observation.model = text(&value["message"]["model"]);
                if value["message"]["usage"].is_object() {
                    observation.usage = Some(Usage::messages(&value["message"]["usage"]));
                }
            }
            Some("message_delta") => {
                if value["usage"].is_object() {
                    let mut usage = Usage::messages(&value["usage"]);
                    // message_delta repeats input counts only when it has them.
                    if usage.input.is_none() {
                        usage.input = None;
                    }
                    observation.usage = Some(usage);
                }
                observation.outcome =
                    value["delta"]["stop_reason"]
                        .as_str()
                        .map(|reason| match reason {
                            "end_turn" | "tool_use" | "stop_sequence" | "pause_turn" => {
                                Outcome::Completed
                            }
                            "max_tokens" => Outcome::Incomplete {
                                reason: Some("max_output_tokens".into()),
                            },
                            "refusal" => Outcome::Incomplete {
                                reason: Some("content_filter".into()),
                            },
                            _ => Outcome::Completed,
                        });
            }
            Some("error") => {
                observation.outcome = Some(Outcome::Failed {
                    status: crate::ResponseFailure::from_error(None, value).status,
                    body: value.clone(),
                });
            }
            _ => {
                // A synchronous Messages reply.
                if value["type"] == "message" {
                    observation.model = text(&value["model"]);
                    if value["usage"].is_object() {
                        observation.usage = Some(Usage::messages(&value["usage"]));
                    }
                    observation.outcome =
                        value["stop_reason"].as_str().map(|reason| match reason {
                            "max_tokens" => Outcome::Incomplete {
                                reason: Some("max_output_tokens".into()),
                            },
                            "refusal" => Outcome::Incomplete {
                                reason: Some("content_filter".into()),
                            },
                            _ => Outcome::Completed,
                        });
                }
            }
        },
        Wire::Gemini => {
            let body = value
                .get("response")
                .filter(|v| v.is_object())
                .unwrap_or(value);
            observation.model = text(&body["modelVersion"]);
            if body["usageMetadata"].is_object() {
                observation.usage = Some(Usage::gemini(&body["usageMetadata"]));
            }
            if let Some(error) = value
                .get("error")
                .or_else(|| body.get("error"))
                .filter(|e| !e.is_null())
            {
                observation.outcome = Some(Outcome::Failed {
                    status: error["code"]
                        .as_u64()
                        .and_then(|code| u16::try_from(code).ok())
                        .filter(|code| (400..=599).contains(code)),
                    body: value.clone(),
                });
            } else if let Some(reason) = body["promptFeedback"]["blockReason"].as_str() {
                observation.outcome = Some(Outcome::Failed {
                    status: Some(400),
                    body: serde_json::json!({"error":{"code":"prompt_blocked","type":"invalid_request_error","message":format!("The prompt was blocked: {reason}")}}),
                });
            } else if let Some(reason) = body["candidates"]
                .as_array()
                .and_then(|c| c.first())
                .and_then(|c| c["finishReason"].as_str())
                .filter(|r| !r.is_empty())
            {
                observation.outcome = Some(match reason {
                    "STOP" => Outcome::Completed,
                    "MAX_TOKENS" => Outcome::Incomplete {
                        reason: Some("max_output_tokens".into()),
                    },
                    "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII"
                    | "IMAGE_SAFETY" | "LANGUAGE" => Outcome::Incomplete {
                        reason: Some("content_filter".into()),
                    },
                    other => Outcome::Failed {
                        status: None,
                        body: serde_json::json!({"error":{"code":other.to_ascii_lowercase(),"type":"server_error","message":format!("The model stopped with finish reason {other}")}}),
                    },
                });
            }
        }
    }
    observation
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_wire_reports_usage_model_and_outcome_in_its_own_vocabulary() {
        let responses = observe(
            Wire::Codex,
            &json!({"type":"response.completed","response":{"model":"gpt-5","status":"completed","usage":{"input_tokens":10,"output_tokens":2,"input_tokens_details":{"cached_tokens":4}}}}),
        );
        assert_eq!(responses.outcome, Some(Outcome::Completed));
        assert_eq!(responses.usage.unwrap().cached, Some(4));
        let chat = observe(
            Wire::ChatCompletions,
            &json!({"model":"deepseek","choices":[{"delta":{},"finish_reason":"length"}],"usage":{"prompt_tokens":7,"completion_tokens":3}}),
        );
        assert_eq!(
            chat.outcome,
            Some(Outcome::Incomplete {
                reason: Some("max_output_tokens".into())
            })
        );
        let messages = observe(
            Wire::Messages,
            &json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":9,"input_tokens":3,"cache_read_input_tokens":5}}),
        );
        assert_eq!(messages.outcome, Some(Outcome::Completed));
        assert_eq!(messages.usage.as_ref().unwrap().input, Some(8));
        let gemini = observe(
            Wire::Gemini,
            &json!({"response":{"candidates":[{"content":{"parts":[{"text":"x"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5,"thoughtsTokenCount":2}}}),
        );
        assert_eq!(gemini.outcome, Some(Outcome::Completed));
        assert_eq!(gemini.usage.unwrap().output, Some(7));
        assert!(gemini.observed);
    }
}
