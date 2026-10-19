//! Public generation protocols. A Responses, Chat or Messages request is converted
//! once into the canonical Responses request; each provider adapter turns that into
//! its wire request and delivers Responses events back, converted once more into the
//! caller's protocol. Consumers retain authentication, admission, scheduling and settlement.
pub mod chat;
pub mod images;
pub mod messages;
pub mod request;
pub mod responses;
mod usage;

use http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

pub type Result<T> = std::result::Result<T, Error>;
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
    pub fn upstream_value(value: &Value) -> Self {
        let failure = crate::ResponseFailure::from_error(None, value);
        Self {
            status: StatusCode::from_u16(failure.status.unwrap_or(502))
                .unwrap_or(StatusCode::BAD_GATEWAY),
            body: json!({"error":crate::public_output::error(value.get("error").unwrap_or(value))}),
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Protocol conversion failed: {}", self.status)
    }
}
impl std::error::Error for Error {}

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
}

/// A validated public request. Supplier selection decides its upstream protocol.
pub struct InboundRequest {
    pub protocol: Protocol,
    pub body: Value,
    pub headers: HeaderMap,
    pub stream: bool,
    pub include_usage: bool,
    /// Caller metadata stays local and is echoed by Responses and Chat.
    pub metadata: Option<Value>,
}
impl InboundRequest {
    pub fn new(protocol: Protocol, body: Value, mut headers: HeaderMap) -> Result<Self> {
        let parsed = request::parse(protocol, body)?;
        // Request identity is stable across supplier failover, never account-derived.
        headers
            .entry("x-grok-req-id")
            .or_insert_with(|| uuid::Uuid::new_v4().to_string().parse().expect("UUID"));
        Ok(Self {
            protocol,
            body: parsed.body,
            headers,
            stream: parsed.stream,
            include_usage: parsed.include_usage,
            metadata: parsed.metadata,
        })
    }
}

/// The supplier request and how its reply is delivered to the caller.
#[derive(Clone)]
pub struct PreparedRequest {
    pub body: Value,
    pub headers: HeaderMap,
    pub inbound: Protocol,
    /// The protocol of the events the adapter delivers back.
    pub upstream: Protocol,
    /// The supplier wire operation recorded in the ledger when it is not the protocol path.
    pub upstream_endpoint: Option<&'static str>,
    pub stream: bool,
    pub include_usage: bool,
    pub metadata: Option<Value>,
}
impl PreparedRequest {
    pub(crate) fn new(input: &InboundRequest, upstream: Protocol, body: Value) -> Self {
        Self {
            body,
            headers: HeaderMap::new(),
            inbound: input.protocol,
            upstream,
            upstream_endpoint: None,
            stream: input.stream,
            include_usage: input.include_usage,
            metadata: (input.protocol != Protocol::Messages)
                .then(|| input.metadata.clone())
                .flatten(),
        }
    }
    pub fn endpoint(&self) -> &'static str {
        self.upstream_endpoint
            .unwrap_or_else(|| self.upstream.endpoint())
    }
}

pub fn boolean(value: &Value, key: &str, default: bool) -> Result<bool> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(v)) => Ok(*v),
        _ => Err(Error::bad_request(format!("{key} must be a boolean"))),
    }
}

/// The single request conversion into the Responses protocol every supplier uses.
pub fn convert(from: Protocol, body: &Value) -> Result<Value> {
    match from {
        Protocol::Responses => responses::request(body),
        Protocol::ChatCompletions => chat::responses::request(body),
        Protocol::Messages => messages::responses::request(body),
        _ => Err(Error::bad_request("Unsupported protocol conversion")),
    }
}

/// Converts supplier Responses events into the caller's protocol events.
pub struct Converter(Stream);
enum Stream {
    Responses,
    Chat(Box<chat::responses::Stream>),
    Messages(messages::responses::Stream),
}
impl Converter {
    /// A synchronous caller always receives usage, so its chunks carry it.
    pub fn new(prepared: &PreparedRequest) -> Self {
        Self(match prepared.inbound {
            Protocol::ChatCompletions => Stream::Chat(Box::new(chat::responses::Stream::new(
                prepared.include_usage || !prepared.stream,
            ))),
            Protocol::Messages => Stream::Messages(Default::default()),
            _ => Stream::Responses,
        })
    }
    /// One supplier event; failures are reported by the caller's error format instead.
    pub fn event(&mut self, value: Value) -> Result<Vec<Value>> {
        match &mut self.0 {
            Stream::Responses => Ok(vec![value]),
            Stream::Chat(s) => s.event(&value),
            Stream::Messages(s) => s.event(&value),
        }
    }
}

/// Assembles a synchronous reply from the caller-protocol events.
pub enum Collector {
    Responses(responses::Output),
    Chat(Box<chat::Output>),
    Messages(messages::Output),
}
impl Collector {
    pub fn new(inbound: Protocol) -> Self {
        match inbound {
            Protocol::ChatCompletions => Self::Chat(Default::default()),
            Protocol::Messages => Self::Messages(Default::default()),
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

/// Affinity seeds for supplier selection. A route pinned to one protocol platform
/// uses that platform's rules; a route open to every platform uses the public
/// Responses fields that all adapters understand.
pub fn scheduling(
    provider: Option<&str>,
    headers: &HeaderMap,
    body: &Value,
) -> Result<crate::scheduler::ScheduleRequest> {
    match provider {
        Some(codex2api_common::CHATGPT) => {
            Ok(crate::providers::chatgpt::api::scheduling(headers, body))
        }
        Some(codex2api_common::GROK) => Ok(crate::providers::grok::api::scheduling(headers, body)),
        Some(codex2api_common::ANTIGRAVITY) => Ok(crate::providers::antigravity::api::scheduling(
            headers, body,
        )),
        Some(_) => Err(Error::bad_request("Unsupported supplier platform")),
        None => {
            use crate::scheduler::{body_seed, hash, header_seed, text};
            let seed = header_seed(headers)
                .or_else(|| text(body, "prompt_cache_key"))
                .or_else(|| text(body, "previous_response_id"))
                .or_else(|| body_seed(body));
            Ok(crate::scheduler::ScheduleRequest {
                model: text(body, "model"),
                session_hash: seed.map(|seed| hash(&seed)),
                previous_response: text(body, "previous_response_id"),
                previous_can_move: false,
                ..Default::default()
            })
        }
    }
}
pub fn chatgpt_context(body: &mut Value, headers: &HeaderMap) -> Result<HeaderMap> {
    crate::providers::chatgpt::api::request_context(body, headers)
}

/// Build the supplier request for the selected account's provider.
pub fn prepare_for(provider: &str, input: &InboundRequest) -> Result<PreparedRequest> {
    match provider {
        codex2api_common::CHATGPT => crate::providers::chatgpt::api::prepare(input),
        codex2api_common::GROK => crate::providers::grok::api::prepare(input),
        codex2api_common::ANTIGRAVITY => crate::providers::antigravity::api::prepare(input),
        _ => Err(Error::bad_request("Unsupported supplier platform")),
    }
}

/// Sends one prepared attempt; the body is consumed rather than copied.
pub async fn send(
    pool: &crate::UpstreamPool,
    account: &codex2api_storage::SupplierAccount,
    headers: &HeaderMap,
    upstream: Protocol,
    body: Value,
) -> crate::Result<reqwest::Response> {
    match account.provider_id.as_str() {
        codex2api_common::CHATGPT => {
            crate::providers::chatgpt::api::send(pool, &account.id, headers, upstream, body).await
        }
        codex2api_common::GROK => {
            pool.grok()
                .grok_request_prepared(
                    &account.id,
                    http::Method::POST,
                    upstream.upstream_path(),
                    Some(crate::grok::PreparedBody::new(body)?),
                    headers,
                )
                .await
        }
        codex2api_common::ANTIGRAVITY => pool.antigravity().generate(&account.id, body).await,
        _ => Err(crate::UpstreamError::InvalidRequest(
            "Unsupported supplier platform".into(),
        )),
    }
}
