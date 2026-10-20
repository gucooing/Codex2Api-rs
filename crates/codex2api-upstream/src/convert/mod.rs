//! The conversion matrix: one direct conversion per (downstream format, wire) pair,
//! from the request as sent to the supplier's official wire request, and from the
//! wire's events back to the caller's protocol. No pair goes through another.
pub mod chat_completions;
pub mod gemini;
pub mod messages;
pub mod messages_output;
pub mod observe;
pub mod responses;
pub mod sanitize;
pub mod stream;
pub(crate) mod usage;

pub use crate::inbound::{Error, Result};
pub use observe::{Observation, Outcome, Usage, observe};

/// Caller-scoped Codex context headers and `client_metadata` for a Codex request.
pub fn codex_context(body: &mut Value, inbound: &http::HeaderMap) -> Result<http::HeaderMap> {
    crate::providers::chatgpt::api::request_context(body, inbound)
}

use crate::inbound::{Inbound, InboundRequest};
use crate::supply::{Operation, Wire, WireRequest};
use serde_json::{Value, json};

/// Whether a conversion exists from `inbound` to `wire` for generations. A native
/// client's request only goes to the wire it was written for.
pub fn supports(inbound: Inbound, wire: Wire) -> bool {
    match inbound {
        Inbound::CodexClient => wire == Wire::Codex,
        Inbound::GrokClient => wire == Wire::Xai,
        Inbound::Responses | Inbound::Chat | Inbound::Messages => true,
    }
}

/// The supplier's wire request for one attempt, built from the downstream request.
pub fn request(input: &InboundRequest, wire: Wire) -> Result<WireRequest> {
    if !input.operation.generation() {
        return operation(input, wire);
    }
    let (body, headers) = match wire {
        Wire::Codex => responses::build(input, responses::Dialect::Codex)?,
        Wire::Xai => responses::build(input, responses::Dialect::Xai)?,
        Wire::OpenAiResponses => responses::build(input, responses::Dialect::OpenAi)?,
        Wire::Gemini => (gemini::build(input)?, http::HeaderMap::new()),
        Wire::ChatCompletions => (chat_completions::build(input)?, http::HeaderMap::new()),
        Wire::Messages => (messages::build(input)?, http::HeaderMap::new()),
    };
    Ok(WireRequest {
        wire,
        operation: Operation::Generate,
        model: body["model"].as_str().map(str::to_owned),
        body,
        headers,
        stream: true,
    })
}

/// Channel operations with their own official bodies: compaction, counting, search
/// and images. They exist on the ChatGPT and Grok wires only.
fn operation(input: &InboundRequest, wire: Wire) -> Result<WireRequest> {
    match (wire, input.operation) {
        (Wire::Codex, Operation::Compact | Operation::InputTokens | Operation::Search) => {
            crate::providers::chatgpt::api::operation(input)
        }
        (Wire::Codex, Operation::ImageGeneration | Operation::ImageEdit) => {
            crate::providers::chatgpt::api_images::prepare(
                input.body.clone(),
                input.operation == Operation::ImageEdit,
                &input.headers,
            )
        }
        (Wire::Xai, Operation::ImageGeneration | Operation::ImageEdit) => {
            crate::providers::grok::api_images::prepare(
                input.body.clone(),
                input.operation == Operation::ImageEdit,
                &input.headers,
            )
        }
        _ => Err(Error::bad_request(
            "This endpoint is not offered by the selected supplier's platform",
        )),
    }
}

/// Bind the already-converted Codex metadata to the selected supplier's headers.
/// Header filtering never clones the caller's request body.
pub fn bind_turn_state(request: &mut WireRequest) {
    if request.wire == Wire::Codex && !request.headers.contains_key("x-codex-turn-state") {
        if let Some(metadata) = request
            .body
            .get_mut("client_metadata")
            .and_then(Value::as_object_mut)
        {
            metadata.remove("x-codex-turn-state");
        }
    }
}

/// Converts one wire's events into the caller's protocol events, one event at a time.
pub trait EventConversion: Send {
    fn event(&mut self, value: Value) -> Result<Vec<Value>>;
    /// The stream ended; a wire whose terminal state is implicit emits it now.
    fn finish(&mut self) -> Result<Vec<Value>> {
        Ok(Vec::new())
    }
}

/// The event conversion for a generation on `wire` read by `inbound`.
pub fn events(
    inbound: Inbound,
    wire: Wire,
    include_usage: bool,
    model: &str,
) -> Box<dyn EventConversion> {
    match wire {
        Wire::Codex | Wire::Xai | Wire::OpenAiResponses => {
            Box::new(responses::Events::new(inbound, include_usage))
        }
        Wire::Gemini => gemini::events::conversion(inbound, include_usage, model),
        Wire::ChatCompletions => {
            chat_completions::events::conversion(inbound, include_usage, model)
        }
        Wire::Messages => messages::events::conversion(inbound, include_usage, model),
    }
}

/// Assembles a synchronous reply from the caller-protocol events.
pub enum Collector {
    Responses(responses::output::Output),
    Chat(Box<responses::chat_output::Output>),
    Messages(messages_output::Output),
}
impl Collector {
    pub fn new(inbound: Inbound) -> Self {
        match inbound {
            Inbound::Chat => Self::Chat(Default::default()),
            Inbound::Messages => Self::Messages(Default::default()),
            _ => Self::Responses(Default::default()),
        }
    }
    pub fn push(&mut self, event: Value) -> Result<()> {
        match self {
            Self::Responses(output) => output.push(event),
            Self::Chat(output) => {
                output.push(event);
                Ok(())
            }
            Self::Messages(output) => output.push(event),
        }
    }
    pub fn finish(self) -> Result<Value> {
        match self {
            Self::Responses(output) => output.finish(),
            Self::Chat(output) => output.finish(),
            Self::Messages(output) => output.finish(),
        }
    }
}

/// A supplier's error reply or event in the caller's error format, with supplier
/// identities redacted. The type, code, parameter and message are the supplier's.
pub fn client_error(inbound: Inbound, status: u16, body: &Value) -> (http::StatusCode, Value) {
    let status = http::StatusCode::from_u16(status).unwrap_or(http::StatusCode::BAD_GATEWAY);
    let error = crate::supply::error_object(body);
    let mut safe = if let Some(object) = error.as_object() {
        let mut safe = serde_json::Map::new();
        for key in ["type", "code", "message", "param"] {
            if let Some(value) = object.get(key).filter(|v| !v.is_null()) {
                safe.insert(key.into(), value.clone());
            }
        }
        Value::Object(safe)
    } else {
        json!({"message":error.as_str().unwrap_or("The supplier rejected the request.")})
    };
    if let Some(message) = safe["message"].as_str() {
        safe["message"] = sanitize::text(message).into();
    }
    if safe.get("type").is_none() {
        safe["type"] = match status.as_u16() {
            401 => "authentication_error",
            403 => "permission_error",
            429 => "rate_limit_error",
            400..=499 => "invalid_request_error",
            _ => "server_error",
        }
        .into();
    }
    if inbound == Inbound::Messages {
        return (
            status,
            crate::inbound::messages::error(status, &json!({"error":safe})),
        );
    }
    (status, json!({"error":safe}))
}

/// The exhaustion error every listener sends when no supplier can serve a request.
pub fn no_available_supplier() -> Value {
    json!({"error":{
        "type":"server_error",
        "code":"no_available_supplier",
        "message":"No supplier is available for this request. Please retry later.",
    }})
}

/// The status a caller sees for a supplier reply the executor did not judge.
pub fn public_status(status: u16) -> u16 {
    if crate::supply::switch_status(status) {
        503
    } else {
        status
    }
}

/// A supplier reply that escaped the executor's judgement: an auxiliary call's
/// reply, or one accepted before its failure was known. A caller fault keeps its
/// status and reason; a status in the switch set is this service's exhaustion and
/// carries none of the supplier's own authorization, billing or throttling details.
pub fn escaped_reply(inbound: Inbound, status: u16, body: &Value) -> (http::StatusCode, Value) {
    if crate::supply::switch_status(status) {
        return client_error(inbound, 503, &no_available_supplier());
    }
    client_error(inbound, status, body)
}

/// An in-stream failure in the caller's protocol.
pub fn failure_event(inbound: Inbound, status: u16, body: &Value) -> Value {
    let (status, mut value) = client_error(inbound, status, body);
    if inbound != Inbound::Messages {
        value["type"] = "error".into();
        value["status"] = status.as_u16().into();
    }
    value
}

#[cfg(test)]
mod isolation_tests {
    use super::*;

    #[test]
    fn supplier_switch_removes_turn_capability_from_header_and_metadata() {
        let input = InboundRequest::new(crate::inbound::Protocol::Responses,
            json!({"model":"m","input":"hi","client_metadata":{"x-codex-turn-state":"owner-capability","turn_id":"caller-turn"}}),
            http::HeaderMap::new()).unwrap();
        let mut request = request(&input, Wire::Codex).unwrap();
        bind_turn_state(&mut request);
        assert_eq!(
            request.body["client_metadata"]["x-codex-turn-state"],
            "owner-capability"
        );
        request.headers.remove("x-codex-turn-state");
        bind_turn_state(&mut request);
        assert!(
            request.body["client_metadata"]
                .get("x-codex-turn-state")
                .is_none()
        );
        assert_eq!(request.body["client_metadata"]["turn_id"], "caller-turn");
        assert_eq!(
            input.body["client_metadata"]["x-codex-turn-state"],
            "owner-capability"
        );
    }
}
