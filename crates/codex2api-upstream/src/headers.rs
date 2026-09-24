use http::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use uuid::Uuid;

use codex2api_accounts::AccountIdentity;

use crate::error::Result;

/// Official header names used by logged-in Codex CLI.
pub const ORIGINATOR_HEADER: &str = "originator";
pub const CHATGPT_ACCOUNT_ID_HEADER: &str = "ChatGPT-Account-ID";
pub const X_CODEX_INSTALLATION_ID_HEADER: &str = "x-codex-installation-id";
pub const SESSION_ID_HEADER: &str = "session-id";
pub const THREAD_ID_HEADER: &str = "thread-id";
pub const X_CLIENT_REQUEST_ID_HEADER: &str = "x-client-request-id";
pub const X_CODEX_TURN_METADATA_HEADER: &str = "x-codex-turn-metadata";
pub const X_CODEX_WINDOW_ID_HEADER: &str = "x-codex-window-id";
pub const X_CODEX_TURN_STATE_HEADER: &str = "x-codex-turn-state";
pub const X_OPENAI_SUBAGENT_HEADER: &str = "x-openai-subagent";
pub const X_CODEX_PARENT_THREAD_ID_HEADER: &str = "x-codex-parent-thread-id";
pub const ACCEPT_EVENT_STREAM: &str = "text/event-stream";

/// Per-request headers matching official Codex CLI.
///
/// These are request-scoped (session/thread/turn/window/request id). They must
/// not be frozen as account-level constants.
#[derive(Debug, Clone, Default)]
pub struct RequestHeaders {
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    /// When unset, official CLI copies `thread_id` into `x-client-request-id`.
    pub client_request_id: Option<String>,
    /// JSON payload for `x-codex-turn-metadata` (already serialized).
    pub turn_metadata: Option<String>,
    pub window_id: Option<String>,
    /// Extra headers such as `x-openai-subagent` or `x-codex-parent-thread-id`.
    /// Applied first; structured fields overwrite the official names.
    pub extra: HeaderMap,
}

impl RequestHeaders {
    /// Official-style ids for a new thread: session, thread, client request,
    /// and `{thread_id}:0` window id.
    pub fn new_thread() -> Self {
        let id = Uuid::new_v4().to_string();
        Self {
            session_id: Some(id.clone()),
            thread_id: Some(id.clone()),
            client_request_id: Some(id.clone()),
            window_id: Some(format!("{id}:0")),
            turn_metadata: None,
            extra: HeaderMap::new(),
        }
    }

    pub fn with_session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self
    }

    pub fn with_thread_id(mut self, id: impl Into<String>) -> Self {
        let id = id.into();
        if self.client_request_id.is_none() {
            self.client_request_id = Some(id.clone());
        }
        self.thread_id = Some(id);
        self
    }

    pub fn with_client_request_id(mut self, id: impl Into<String>) -> Self {
        self.client_request_id = Some(id.into());
        self
    }

    pub fn with_window_id(mut self, id: impl Into<String>) -> Self {
        self.window_id = Some(id.into());
        self
    }

    pub fn with_turn_metadata(mut self, json: impl Into<String>) -> Self {
        self.turn_metadata = Some(json.into());
        self
    }

    pub fn with_turn_metadata_value(mut self, value: &Value) -> Result<Self> {
        self.turn_metadata = Some(serde_json::to_string(value)?);
        Ok(self)
    }

    pub fn with_extra(mut self, extra: HeaderMap) -> Self {
        self.extra = extra;
        self
    }

    /// Build the extra HeaderMap official CLI attaches besides default identity.
    ///
    /// Order matches `ResponsesClient::stream_request`:
    /// extra → `x-client-request-id` (thread id) → `session-id` / `thread-id`,
    /// then compatibility `x-codex-window-id` / `x-codex-turn-metadata`.
    pub fn to_header_map(&self) -> HeaderMap {
        let mut headers = self.extra.clone();
        let client_request_id = self
            .client_request_id
            .as_deref()
            .or(self.thread_id.as_deref());
        if let Some(id) = client_request_id {
            insert_header(&mut headers, X_CLIENT_REQUEST_ID_HEADER, id);
        }
        if let Some(id) = self.session_id.as_deref() {
            insert_header(&mut headers, SESSION_ID_HEADER, id);
        }
        if let Some(id) = self.thread_id.as_deref() {
            insert_header(&mut headers, THREAD_ID_HEADER, id);
        }
        if let Some(id) = self.window_id.as_deref() {
            insert_header(&mut headers, X_CODEX_WINDOW_ID_HEADER, id);
        }
        if let Some(meta) = self.turn_metadata.as_deref() {
            insert_header(&mut headers, X_CODEX_TURN_METADATA_HEADER, meta);
        }
        headers
    }
}

/// Default identity + auth headers sent on every Codex request.
pub fn default_headers(
    identity: &AccountIdentity,
    access_token: &str,
    chatgpt_account_id: Option<&str>,
) -> Result<HeaderMap> {
    let mut headers = codex2api_auth::transport::identity_headers(identity)?;
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {access_token}"))?,
    );
    if let Some(account_id) = chatgpt_account_id.filter(|s| !s.is_empty()) {
        headers.insert(
            CHATGPT_ACCOUNT_ID_HEADER,
            HeaderValue::from_str(account_id)?,
        );
    }
    headers.insert(
        "version",
        HeaderValue::from_static(codex2api_version::CODEX_PACKAGE_VERSION),
    );
    Ok(headers)
}

/// Remove connection-local headers before forwarding across an HTTP hop.
pub fn strip_hop_by_hop_headers(headers: &mut HeaderMap) {
    let connection_headers: Vec<HeaderName> = headers
        .get_all(http::header::CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|name| name.trim().parse().ok())
        .collect();
    for name in connection_headers {
        headers.remove(name);
    }
    for name in [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "proxy-connection",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ] {
        headers.remove(name);
    }
}

/// Forward allowlisted headers used by the pinned official Codex client.
/// SupplierAccount identity, credentials, cookies and transport headers are generated separately.
pub(crate) fn protocol_headers(inbound: &HeaderMap) -> HeaderMap {
    let mut inbound = inbound.clone();
    strip_hop_by_hop_headers(&mut inbound);
    let mut headers = HeaderMap::new();
    for name in [
        SESSION_ID_HEADER,
        THREAD_ID_HEADER,
        X_CLIENT_REQUEST_ID_HEADER,
        X_CODEX_WINDOW_ID_HEADER,
        X_CODEX_TURN_METADATA_HEADER,
        X_CODEX_TURN_STATE_HEADER,
        X_CODEX_PARENT_THREAD_ID_HEADER,
        X_OPENAI_SUBAGENT_HEADER,
        "x-codex-beta-features",
        "x-openai-internal-codex-responses-lite",
        "x-openai-memgen-request",
        "x-codex-routing-hint",
        "x-responsesapi-include-timing-metrics",
        "traceparent",
        "tracestate",
        "x-codex-inference-call-id",
        "x-oai-attestation",
        "x-openai-internal-codex-residency",
        "x-openai-fedramp",
        "openai-organization",
        "openai-project",
        "openai-beta",
    ] {
        for value in inbound.get_all(name) {
            headers.append(name, value.clone());
        }
    }
    headers
}

/// Official helper: skip headers that are not valid HTTP field values.
pub fn insert_header(headers: &mut HeaderMap, name: &str, value: &str) {
    if let (Ok(header_name), Ok(header_value)) =
        (name.parse::<HeaderName>(), HeaderValue::from_str(value))
    {
        headers.insert(header_name, header_value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex2api_accounts::HostRuntime;

    #[test]
    fn identity_uses_pinned_version_and_account_auth() {
        let identity = AccountIdentity::new("a", "installation", HostRuntime::generate());
        let headers = default_headers(&identity, "token", Some("account")).unwrap();
        assert_eq!(headers["originator"], "codex_cli_rs");
        assert_eq!(headers["version"], "0.154.0");
        assert_eq!(headers["authorization"], "Bearer token");
        assert_eq!(headers["chatgpt-account-id"], "account");
        assert_eq!(
            headers[http::header::USER_AGENT],
            identity.official_user_agent()
        );
        // Pinned Responses carries installation identity in client_metadata.
        assert!(!headers.contains_key(X_CODEX_INSTALLATION_ID_HEADER));
    }

    #[test]
    fn discards_untrusted_headers_but_keeps_session_state() {
        let mut inbound = HeaderMap::new();
        for name in [
            "authorization",
            "user-agent",
            "originator",
            "version",
            "cookie",
            "x-codex-installation-id",
            "chatgpt-account-id",
            "via",
            "forwarded",
            "x-forwarded-for",
            "x-forwarded-host",
            "x-real-ip",
            "x-api-key",
            "x-custom",
            "connection",
            "content-encoding",
        ] {
            inbound.insert(name, HeaderValue::from_static("untrusted"));
        }
        inbound.insert(
            X_CODEX_TURN_STATE_HEADER,
            HeaderValue::from_static("turn-state"),
        );
        inbound.insert(THREAD_ID_HEADER, HeaderValue::from_static("thread"));
        let headers = protocol_headers(&inbound);
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[X_CODEX_TURN_STATE_HEADER], "turn-state");
        assert_eq!(headers[THREAD_ID_HEADER], "thread");
    }

    #[test]
    fn excludes_connection_scoped_values_from_the_allowlist() {
        let mut inbound = HeaderMap::new();
        inbound.append("connection", HeaderValue::from_static("Traceparent"));
        inbound.append("connection", HeaderValue::from_static("OpenAI-Project"));
        inbound.insert("traceparent", HeaderValue::from_static("connection-local"));
        inbound.insert(
            "openai-project",
            HeaderValue::from_static("connection-local"),
        );
        inbound.insert("x-oai-attestation", HeaderValue::from_static("attestation"));
        assert_eq!(
            protocol_headers(&inbound),
            HeaderMap::from_iter([(
                HeaderName::from_static("x-oai-attestation"),
                HeaderValue::from_static("attestation"),
            )])
        );
    }

    #[test]
    fn removes_all_connection_specific_response_headers() {
        let mut headers = HeaderMap::new();
        headers.append("connection", HeaderValue::from_static("X-First"));
        headers.append(
            "connection",
            HeaderValue::from_static("x-second, Keep-Alive"),
        );
        for name in ["x-first", "x-second", "keep-alive", "upgrade", "trailer"] {
            headers.insert(name, HeaderValue::from_static("local"));
        }
        headers.insert("content-encoding", HeaderValue::from_static("zstd"));
        strip_hop_by_hop_headers(&mut headers);
        assert_eq!(headers.len(), 1);
    }
}
