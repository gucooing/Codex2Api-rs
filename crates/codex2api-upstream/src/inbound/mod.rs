//! Downstream request formats. A request is validated once in its own format and
//! kept as sent; the selected supplier's wire decides the conversion afterwards.
pub mod images;
pub mod messages;
pub mod request;

use crate::supply::Operation;
use http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

pub type Result<T> = std::result::Result<T, Error>;

/// A caller-facing rejection in the OpenAI error shape; Messages callers receive it
/// rewritten into theirs at the listener.
#[derive(Debug)]
pub struct Error {
    pub status: StatusCode,
    pub body: Value,
}
impl Error {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            body: json!({"error":{"type":"invalid_request_error","code":"invalid_request_error","message":message.into()}}),
        }
    }
    pub fn upstream_incomplete() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            body: json!({"error":{"type":"server_error","code":"upstream_protocol_error","message":"The supplier returned an incomplete or unsupported protocol payload."}}),
        }
    }
    /// A supplier failure value delivered as the caller's error.
    pub fn upstream_value(value: &Value) -> Self {
        let status = crate::ResponseFailure::from_error(None, value)
            .status
            .and_then(|status| StatusCode::from_u16(status).ok())
            .unwrap_or(StatusCode::BAD_GATEWAY);
        let error = crate::supply::error_object(value).clone();
        Self {
            status,
            body: json!({"error":error}),
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Request conversion failed: {}", self.status)
    }
}
impl std::error::Error for Error {}

/// The public API gateway's endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Responses,
    ChatCompletions,
    Messages,
    Compact,
    InputTokens,
    Search,
    ImageGeneration,
    ImageEdit,
}
impl Protocol {
    pub fn endpoint(self) -> &'static str {
        match self {
            Self::Responses => "/v1/responses",
            Self::ChatCompletions => "/v1/chat/completions",
            Self::Messages => "/v1/messages",
            Self::Compact => "/v1/responses/compact",
            Self::InputTokens => "/v1/responses/input_tokens",
            Self::Search => "/v1/alpha/search",
            Self::ImageGeneration => "/v1/images/generations",
            Self::ImageEdit => "/v1/images/edits",
        }
    }
    /// Supplier operation path relative to the provider's `/v1` root.
    pub fn upstream_path(self) -> &'static str {
        &self.endpoint()["/v1/".len()..]
    }
    pub fn generation(self) -> bool {
        matches!(
            self,
            Self::Responses | Self::ChatCompletions | Self::Messages
        )
    }
    pub fn image(self) -> bool {
        matches!(self, Self::ImageGeneration | Self::ImageEdit)
    }
    pub fn operation(self) -> Operation {
        match self {
            Self::Responses | Self::ChatCompletions | Self::Messages => Operation::Generate,
            Self::Compact => Operation::Compact,
            Self::InputTokens => Operation::InputTokens,
            Self::Search => Operation::Search,
            Self::ImageGeneration => Operation::ImageGeneration,
            Self::ImageEdit => Operation::ImageEdit,
        }
    }
    pub fn format(self) -> Inbound {
        match self {
            Self::ChatCompletions => Inbound::Chat,
            Self::Messages => Inbound::Messages,
            _ => Inbound::Responses,
        }
    }
}

/// The format a downstream request arrives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Inbound {
    /// The official Codex client's Responses request (OAuth listener).
    CodexClient,
    /// Grok Build's native request (OAuth listener), on its native endpoint.
    GrokClient,
    /// Public Responses.
    Responses,
    /// Public Chat Completions.
    Chat,
    /// Public Messages.
    Messages,
}
impl Inbound {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CodexClient => "codex_client",
            Self::GrokClient => "grok_client",
            Self::Responses => "responses",
            Self::Chat => "chat",
            Self::Messages => "messages",
        }
    }
    /// The event protocol this caller reads.
    pub fn responses_family(self) -> bool {
        matches!(self, Self::CodexClient | Self::GrokClient | Self::Responses)
    }
}

/// A validated downstream request, kept in its own format.
#[derive(Clone, Debug)]
pub struct InboundRequest {
    pub format: Inbound,
    pub operation: Operation,
    pub body: Value,
    pub headers: HeaderMap,
    pub stream: bool,
    pub include_usage: bool,
    /// Caller metadata stays local and is echoed by Responses and Chat.
    pub metadata: Option<Value>,
    /// A client's native endpoint path under the supplier's `/v1` root, when the
    /// downstream format is a native client protocol.
    pub path: Option<String>,
}
impl InboundRequest {
    /// A public API request, validated once in its own protocol.
    pub fn new(protocol: Protocol, body: Value, mut headers: HeaderMap) -> Result<Self> {
        let parsed = request::parse(protocol, body)?;
        // Request identity is stable across supplier failover, never account-derived.
        headers
            .entry("x-grok-req-id")
            .or_insert_with(|| uuid::Uuid::new_v4().to_string().parse().expect("UUID"));
        Ok(Self {
            format: protocol.format(),
            operation: protocol.operation(),
            body: parsed.body,
            headers,
            stream: parsed.stream,
            include_usage: parsed.include_usage,
            metadata: parsed.metadata,
            path: None,
        })
    }
    /// An official client's request on its own protocol; the listener validated it.
    pub fn client(
        format: Inbound,
        operation: Operation,
        body: Value,
        headers: HeaderMap,
        path: Option<String>,
    ) -> Self {
        let stream = body["stream"] == true || operation.generation() && body["stream"] != false;
        Self {
            format,
            operation,
            stream,
            include_usage: false,
            metadata: None,
            body,
            headers,
            path,
        }
    }
    pub fn model(&self) -> Option<&str> {
        self.body["model"].as_str()
    }
}

pub fn boolean(value: &Value, key: &str, default: bool) -> Result<bool> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(v)) => Ok(*v),
        _ => Err(Error::bad_request(format!("{key} must be a boolean"))),
    }
}
