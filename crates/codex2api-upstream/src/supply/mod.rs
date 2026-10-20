//! The supply interface the executor sees: a supplier speaks one wire format, accepts
//! a wire request, and judges its own upstream failures. Channel transports and
//! verdict rules live in `providers/<channel>/`; nothing here reads a channel's
//! error vocabulary.
pub mod pool;
pub use pool::{apply_verdict, observe_error};

use http::HeaderMap;
use serde_json::Value;
use std::time::Duration;

/// The official request format a supplier accepts. Three Responses dialects differ
/// in required and accepted fields; each is its own wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Wire {
    /// Official Codex Responses of the pinned client (ChatGPT subscriptions).
    Codex,
    /// xAI Responses as Grok Build sends it.
    Xai,
    /// OpenAI API Responses.
    OpenAiResponses,
    /// OpenAI API Chat Completions and compatible services.
    ChatCompletions,
    /// Anthropic Messages.
    Messages,
    /// Cloud Code Assist `v1internal` Gemini wire (Antigravity).
    Gemini,
}
impl Wire {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Xai => "xai",
            Self::OpenAiResponses => "openai_responses",
            Self::ChatCompletions => "chat_completions",
            Self::Messages => "messages",
            Self::Gemini => "gemini",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "codex" => Self::Codex,
            "xai" => Self::Xai,
            "openai_responses" => Self::OpenAiResponses,
            "chat_completions" => Self::ChatCompletions,
            "messages" => Self::Messages,
            "gemini" => Self::Gemini,
            _ => return None,
        })
    }
    /// The event protocol a generation on this wire streams back.
    pub fn responses_family(self) -> bool {
        matches!(self, Self::Codex | Self::Xai | Self::OpenAiResponses)
    }
}

/// What a wire request asks the supplier to do. Generation is the matrix-converted
/// path; the others are channel operations with their own official bodies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Generate,
    Compact,
    InputTokens,
    Search,
    ImageGeneration,
    ImageEdit,
}
impl Operation {
    pub fn generation(self) -> bool {
        self == Self::Generate
    }
    pub fn image(self) -> bool {
        matches!(self, Self::ImageGeneration | Self::ImageEdit)
    }
}

/// A request in the selected supplier's official wire format.
#[derive(Clone, Debug)]
pub struct WireRequest {
    pub wire: Wire,
    pub operation: Operation,
    pub body: Value,
    /// Dynamic request-scoped headers the channel may forward (never credentials).
    pub headers: HeaderMap,
    pub model: Option<String>,
    pub stream: bool,
}

/// What a channel saw go wrong on one attempt, before any policy.
#[derive(Clone, Debug)]
pub struct UpstreamFailure {
    /// HTTP status of a reply, or `None` for a failure event inside a stream or a
    /// transport fault.
    pub status: Option<u16>,
    pub headers: HeaderMap,
    /// The decoded reply or event body in the channel's own error shape.
    pub body: Value,
    pub transport: Option<TransportFault>,
    pub model: Option<String>,
    /// Whether the caller already received output of this generation.
    pub output_committed: bool,
    pub now: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportFault {
    /// DNS, TCP, TLS or proxy failure before any reply.
    Connect,
    /// No reply headers within the wait.
    Timeout,
    /// The reply stream ended or stalled before the generation's first output.
    Stream,
}

/// How long a supplier is avoided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AvoidScope {
    /// Only this request skips the supplier; nothing is persisted.
    Request,
    /// Until the Unix second.
    Until(i64),
    /// Until reauthorization or an administrator reset.
    Permanent,
}

/// Whom the avoidance applies to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AvoidTarget {
    Account,
    Model(String),
}

/// A channel's judgement of one failed attempt.
#[derive(Clone, Debug)]
pub enum Verdict {
    /// The request itself is wrong: the caller receives this reply, in its own
    /// format, and the supplier keeps serving.
    ClientError { status: u16, body: Value },
    /// Transient: retry the same supplier after `delay`, at most `max` times, then
    /// treat as `fallback`.
    RetrySame {
        delay: Duration,
        max: u32,
        fallback: Box<Verdict>,
    },
    /// Avoid the supplier and switch.
    Avoid {
        scope: AvoidScope,
        target: AvoidTarget,
        kind: codex2api_storage::BlockKind,
        code: Option<String>,
        message: Option<String>,
    },
    /// The generation cannot continue on any supplier.
    Stop { status: u16, body: Value },
}

/// The HTTP statuses that never reach a caller as a supplier's own state: credential,
/// billing, permission and throttling replies, and every server-side failure. A
/// reply with one of them switches suppliers; once no supplier is left it is this
/// service's exhaustion.
pub fn switch_status(status: u16) -> bool {
    matches!(status, 401 | 402 | 403 | 405 | 429 | 500..=599)
}

impl Verdict {
    pub fn avoid(
        scope: AvoidScope,
        target: AvoidTarget,
        kind: codex2api_storage::BlockKind,
        code: Option<&str>,
        message: Option<&str>,
    ) -> Self {
        Self::Avoid {
            scope,
            target,
            kind,
            code: code.map(str::to_owned),
            message: message.map(str::to_owned),
        }
    }
    pub fn avoid_request() -> Self {
        Self::Avoid {
            scope: AvoidScope::Request,
            target: AvoidTarget::Account,
            kind: codex2api_storage::BlockKind::Transport,
            code: None,
            message: None,
        }
    }
    pub fn retry_same(delay: Duration, max: u32, fallback: Verdict) -> Self {
        Self::RetrySame {
            delay,
            max,
            fallback: Box::new(fallback),
        }
    }
}

/// `Retry-After` as a Unix second: delay seconds or an HTTP date.
pub fn retry_after(headers: &HeaderMap, now: i64) -> Option<i64> {
    let value = headers.get("retry-after")?.to_str().ok()?.trim();
    value
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .map(|n| now.saturating_add(n.ceil().min(i64::MAX as f64) as i64))
        .or_else(|| {
            chrono::DateTime::parse_from_rfc2822(value)
                .ok()
                .map(|t| t.timestamp())
        })
        .or_else(|| {
            headers
                .get("retry-after-ms")?
                .to_str()
                .ok()?
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && *n >= 0.0)
                .map(|n| now.saturating_add((n / 1000.0).ceil() as i64))
        })
}

/// The error object of a reply or event, wherever the channel put it.
pub fn error_object(body: &Value) -> &Value {
    body.pointer("/response/error")
        .filter(|v| !v.is_null())
        .or_else(|| body.get("error").filter(|v| !v.is_null()))
        .or_else(|| body.get("detail").filter(|v| !v.is_null()))
        .unwrap_or(body)
}

/// Code, type and message of an error object, lowercased code and type.
pub fn error_fields(body: &Value) -> (String, String, String) {
    let error = error_object(body);
    let text = |key: &str| {
        error
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_owned()
    };
    let code = error
        .get("code")
        .map(|code| match code {
            Value::String(code) => code.trim().to_ascii_lowercase(),
            other => other.to_string(),
        })
        .unwrap_or_default();
    let kind = text("type").to_ascii_lowercase();
    let message = text("message");
    let message = if message.is_empty() {
        error.as_str().unwrap_or_default().to_owned()
    } else {
        message
    };
    (code, kind, message)
}
