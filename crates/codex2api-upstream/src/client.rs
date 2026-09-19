use std::sync::{Arc, RwLock};
use std::time::Duration;

use bytes::Bytes;
use http::header::HeaderMap;
use reqwest::StatusCode;
use serde_json::Value;

use codex2api_accounts::{AccountContext, AccountIdentity};
use codex2api_auth::AuthService;
use codex2api_version::{CHATGPT_CODEX_BASE_URL, RESPONSES_PATH};

use crate::error::{Result, UpstreamError};
use crate::headers::{RequestHeaders, default_headers};
use crate::request::{PreparedRequest, prepare_responses};
use crate::stream::{SseForwardStream, spawn_sse_forward};
use codex2api_auth::transport::{AccountCookieStore, AccountHttpClients};

/// Default idle timeout while waiting for the next SSE event (official-style).
pub const DEFAULT_STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
struct TokenState {
    access_token: String,
    chatgpt_account_id: Option<String>,
}

/// Per-account HTTP client for official Codex servers.
///
/// Each instance owns its `reqwest::Client` (and therefore its cookie jar).
/// Do not clone the inner client across accounts. HTTP uses the pinned client's
/// standard transport defaults; no TLS/JA3 spoofing is performed.
pub struct UpstreamClient {
    http: reqwest::Client,
    identity: AccountIdentity,
    tokens: RwLock<TokenState>,
    auth: Option<AuthService>,
    stream_idle_timeout: Duration,
    pub(crate) cookies: Arc<AccountCookieStore>,
    pub(crate) account_http: Arc<AccountHttpClients>,
}

impl UpstreamClient {
    pub fn new(
        identity: AccountIdentity,
        access_token: String,
        chatgpt_account_id: Option<String>,
    ) -> Result<Self> {
        Self::build(identity, access_token, chatgpt_account_id, None)
    }

    pub fn from_context(ctx: AccountContext, auth: Option<AuthService>) -> Result<Self> {
        let account_id = ctx.account.id.clone();
        let tokens = ctx
            .auth
            .as_ref()
            .and_then(|auth| auth.tokens.as_ref())
            .ok_or_else(|| UpstreamError::MissingAccessToken(account_id.clone()))?;
        if tokens.access_token.is_empty() {
            return Err(UpstreamError::MissingAccessToken(account_id));
        }
        let chatgpt_account_id = tokens
            .account_id
            .clone()
            .filter(|s| !s.is_empty())
            .or(ctx.account.chatgpt_account_id.clone());
        Self::build(
            ctx.identity,
            tokens.access_token.clone(),
            chatgpt_account_id,
            auth,
        )
    }

    pub fn with_auth_service(mut self, auth: AuthService) -> Self {
        self.auth = Some(auth);
        self
    }

    pub fn with_stream_idle_timeout(mut self, timeout: Duration) -> Self {
        self.stream_idle_timeout = timeout;
        self
    }

    fn build(
        identity: AccountIdentity,
        access_token: String,
        chatgpt_account_id: Option<String>,
        auth: Option<AuthService>,
    ) -> Result<Self> {
        let clients = Arc::new(AccountHttpClients::new(&identity)?);
        let http = clients.api.clone();
        Ok(Self {
            http,
            identity,
            tokens: RwLock::new(TokenState {
                access_token,
                chatgpt_account_id,
            }),
            auth,
            stream_idle_timeout: DEFAULT_STREAM_IDLE_TIMEOUT,
            cookies: clients.cookies.clone(),
            account_http: clients,
        })
    }

    pub fn identity(&self) -> &AccountIdentity {
        &self.identity
    }

    #[cfg(test)]
    pub(crate) fn with_direct_test_http(mut self) -> Self {
        // Loopback fixtures must not depend on Windows/PAC/environment proxy settings.
        self.http = codex2api_auth::transport::http_builder()
            .unwrap()
            .no_proxy()
            .cookie_provider(self.cookies.clone())
            .build()
            .unwrap();
        self
    }

    pub(crate) fn auth_service(&self) -> Option<&AuthService> {
        self.auth.as_ref()
    }

    pub(crate) fn use_account_http(&mut self, clients: Arc<AccountHttpClients>) {
        self.http = clients.api.clone();
        self.cookies = clients.cookies.clone();
        self.identity = clients.identity.clone();
        self.account_http = clients;
    }

    pub fn chatgpt_account_id(&self) -> Option<String> {
        self.tokens
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .chatgpt_account_id
            .clone()
    }

    pub fn default_headers(&self) -> Result<HeaderMap> {
        let tokens = self.tokens.read().unwrap_or_else(|e| e.into_inner());
        default_headers(
            &self.identity,
            &tokens.access_token,
            tokens.chatgpt_account_id.as_deref(),
        )
    }

    pub fn responses_url() -> String {
        responses_url()
    }

    /// Preserve conversation data while rebuilding official identity and transport.
    pub async fn forward_responses(
        &self,
        body: Bytes,
        headers: HeaderMap,
    ) -> Result<reqwest::Response> {
        self.forward_to(&responses_url(), body, headers).await
    }

    pub(crate) async fn forward_to(
        &self,
        url: &str,
        body: Bytes,
        headers: HeaderMap,
    ) -> Result<reqwest::Response> {
        let installation_id = self.identity.installation_id.clone();
        let timezone = self.identity.http_fingerprint.timezone.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            prepare_responses(&body, &headers, &installation_id, timezone.as_deref())
        })
        .await
        .map_err(|e| UpstreamError::InvalidRequest(e.to_string()))??;
        self.send_responses_with_state(url, prepared).await
    }

    pub(crate) async fn send_prepared(
        &self,
        method: http::Method,
        url: &str,
        prepared: PreparedRequest,
        provider_version: bool,
    ) -> Result<reqwest::Response> {
        self.synchronize_auth().await?;
        let mut retried = false;
        loop {
            let (mut upstream_headers, rejected_token) = self.authenticated_headers()?;
            if !provider_version {
                upstream_headers.remove("version");
                upstream_headers.remove("originator");
            }
            upstream_headers.extend(prepared.headers.clone());
            let mut request = self
                .http
                .request(method.clone(), url)
                .headers(upstream_headers);
            if method != http::Method::GET || !prepared.body.is_empty() {
                request = request.body(prepared.body.clone());
            }
            let response = request.send().await?;
            if response.status() == StatusCode::UNAUTHORIZED && !retried && self.auth.is_some() {
                retried = true;
                self.refresh_access_token(&rejected_token).await?;
                continue;
            }
            return Ok(response);
        }
    }

    /// POST `/responses` with extra headers already in a [`HeaderMap`].
    pub async fn post_responses(
        &self,
        body: &Value,
        extra: HeaderMap,
    ) -> Result<reqwest::Response> {
        self.send(
            body,
            RequestHeaders {
                extra,
                ..RequestHeaders::default()
            },
            false,
        )
        .await
    }

    pub async fn post_responses_with(
        &self,
        body: &Value,
        extra: RequestHeaders,
    ) -> Result<reqwest::Response> {
        self.send(body, extra, false).await
    }

    pub async fn post_responses_json(&self, body: &Value, extra: RequestHeaders) -> Result<Value> {
        let response = self.send(body, extra, false).await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(UpstreamError::status(status, text));
        }
        Ok(serde_json::from_str(&text)?)
    }

    /// POST `/responses` as official Codex CLI does: `Accept: text/event-stream`,
    /// then forward SSE frames via `eventsource-stream`.
    pub async fn stream_responses(
        &self,
        body: &Value,
        extra: RequestHeaders,
    ) -> Result<SseForwardStream> {
        let response = self.send(body, extra, true).await?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(UpstreamError::status(status, text));
        }
        Ok(spawn_sse_forward(response, self.stream_idle_timeout))
    }

    async fn send(
        &self,
        body: &Value,
        extra: RequestHeaders,
        _accept_sse: bool,
    ) -> Result<reqwest::Response> {
        self.forward_responses(
            Bytes::from(serde_json::to_vec(body)?),
            extra.to_header_map(),
        )
        .await
    }

    pub(crate) fn authenticated_headers(&self) -> Result<(HeaderMap, String)> {
        let tokens = self.tokens.read().unwrap_or_else(|e| e.into_inner());
        Ok((
            default_headers(
                &self.identity,
                &tokens.access_token,
                tokens.chatgpt_account_id.as_deref(),
            )?,
            tokens.access_token.clone(),
        ))
    }

    pub(crate) async fn synchronize_auth(&self) -> Result<()> {
        let Some(auth) = &self.auth else {
            return Ok(());
        };
        match auth.refresh(&self.identity.account_id, false).await {
            Ok(current) => self.apply_auth(&current),
            Err(error) => {
                // Official proactive refresh keeps the cached auth on transient failure.
                tracing::warn!(%error, "proactive token refresh failed");
                let ctx = auth
                    .accounts()
                    .load_context(&self.identity.account_id)
                    .await?;
                self.apply_auth(ctx.auth.as_ref().ok_or_else(|| {
                    UpstreamError::MissingAccessToken(self.identity.account_id.clone())
                })?)
            }
        }
    }

    pub(crate) async fn refresh_access_token(&self, rejected_token: &str) -> Result<()> {
        let auth = self.auth.as_ref().ok_or(UpstreamError::Unauthorized)?;
        let refreshed = auth
            .refresh_rejected_token(&self.identity.account_id, rejected_token)
            .await
            .map_err(UpstreamError::Refresh)?;
        self.apply_auth(&refreshed)
    }

    fn apply_auth(&self, refreshed: &codex2api_auth::AuthDotJson) -> Result<()> {
        let tokens = refreshed
            .tokens
            .as_ref()
            .ok_or_else(|| UpstreamError::MissingAccessToken(self.identity.account_id.clone()))?;
        if tokens.access_token.is_empty() {
            return Err(UpstreamError::MissingAccessToken(
                self.identity.account_id.clone(),
            ));
        }
        let chatgpt_account_id = tokens
            .account_id
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| refreshed.chatgpt_account_id().map(str::to_string));
        let mut state = self.tokens.write().unwrap_or_else(|e| e.into_inner());
        state.access_token = tokens.access_token.clone();
        state.chatgpt_account_id = chatgpt_account_id;
        Ok(())
    }
}

pub fn responses_url() -> String {
    format!(
        "{}{}",
        CHATGPT_CODEX_BASE_URL.trim_end_matches('/'),
        RESPONSES_PATH
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::response::Response;
    use axum::routing::post;
    use codex2api_accounts::HostRuntime;
    use http::HeaderValue;

    #[test]
    fn responses_url_is_official() {
        assert_eq!(
            responses_url(),
            "https://chatgpt.com/backend-api/codex/responses"
        );
    }

    #[tokio::test]
    async fn usage_request_has_backend_headers_without_responses_only_fields() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = Arc::new(tokio::sync::Mutex::new(Some(tx)));
        let app = Router::new().route(
            "/usage",
            axum::routing::get(move |headers: HeaderMap, body: Bytes| {
                let tx = tx.clone();
                async move {
                    tx.lock()
                        .await
                        .take()
                        .unwrap()
                        .send((headers, body))
                        .unwrap();
                    "{}"
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/usage", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let identity = AccountIdentity::new("a", "installation", HostRuntime::generate());
        let expected_ua = identity.official_user_agent();
        let client = UpstreamClient::new(identity, "token".into(), Some("account".into())).unwrap();
        client
            .send_prepared(
                http::Method::GET,
                &url,
                PreparedRequest {
                    body: Bytes::new(),
                    headers: HeaderMap::new(),
                },
                false,
            )
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        let (headers, body) = rx.await.unwrap();
        assert_eq!(headers["authorization"], "Bearer token");
        assert_eq!(headers["chatgpt-account-id"], "account");
        assert_eq!(headers["user-agent"], expected_ua);
        assert!(body.is_empty());
        for name in [
            "originator",
            "version",
            "content-type",
            "content-encoding",
            "x-codex-installation-id",
        ] {
            assert!(!headers.contains_key(name), "{name}");
        }
        server.abort();
    }

    #[tokio::test]
    async fn reconstructs_official_requests_from_plain_and_zstd_bodies() {
        let passthrough = [
            (
                "traceparent",
                "00-0123456789abcdef0123456789abcdef-0123456789abcdef-01",
            ),
            ("tracestate", "vendor=original"),
            ("x-codex-inference-call-id", "inference-original"),
            ("x-oai-attestation", "attestation-original"),
            ("x-openai-internal-codex-residency", "us"),
            ("x-openai-fedramp", "true"),
            ("openai-organization", "org-original"),
            ("openai-project", "proj-original"),
            ("openai-beta", "responses_websockets=2026-02-06"),
            ("x-codex-routing-hint", "model=test;tier=priority"),
        ];
        let json = b"{ \"stream\": true, \"model\": \"test\", \"input\": [], \"unknown\": 1.00 }\n";
        // Same encoder and level as pinned Codex http-client/src/request.rs.
        let compressed = zstd::stream::encode_all(json.as_slice(), 3).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&compressed)
                .unwrap_err()
                .to_string(),
            "expected value at line 1 column 1"
        );
        let (captured_tx, mut captured_rx) = tokio::sync::mpsc::channel(2);
        let app = Router::new().route(
            "/responses",
            post(move |headers: HeaderMap, body: Bytes| {
                let tx = captured_tx.clone();
                async move {
                    tx.send((headers, body)).await.unwrap();
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "text/event-stream")
                        .header("x-codex-turn-state", "next-turn-state")
                        .body(Body::from(
                            ": heartbeat\r\n\r\ndata: {\"type\":\"response.completed\"}\r\n\r\n",
                        ))
                        .unwrap()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let identity =
            AccountIdentity::new("test-account", "test-installation", HostRuntime::generate());
        let expected_ua = identity.user_agent.clone();
        let client = UpstreamClient::new(
            identity,
            "test-token".into(),
            Some("test-chatgpt-account".into()),
        )
        .unwrap();

        for (body, encoding) in [
            (Bytes::from_static(json), None),
            (Bytes::from(compressed), Some("zstd")),
        ] {
            let mut headers = HeaderMap::new();
            for (name, value) in [
                ("authorization", "Bearer proxy-key"),
                ("user-agent", "caller-agent"),
                ("chatgpt-account-id", "caller-account"),
                ("x-codex-installation-id", "caller-installation"),
                ("cookie", "caller=secret"),
                ("version", "999.0.0"),
                ("forwarded", "for=private"),
                ("via", "third-party"),
                ("x-forwarded-for", "private"),
                ("x-custom", "discard"),
                ("content-type", "application/json"),
                ("accept", "text/event-stream"),
                ("session-id", "original-session"),
                ("thread-id", "original-thread"),
                ("x-codex-turn-state", "original-turn-state"),
                ("x-codex-turn-metadata", "{\"turn_id\":\"turn\"}"),
            ] {
                headers.insert(name, HeaderValue::from_static(value));
            }
            for (name, value) in passthrough {
                headers.insert(name, HeaderValue::from_static(value));
            }
            headers.append("tracestate", HeaderValue::from_static("second=original"));
            if let Some(encoding) = encoding {
                headers.insert("content-encoding", HeaderValue::from_static(encoding));
            }
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                client.forward_to(
                    &format!("http://{addr}/responses"),
                    body.clone(),
                    headers.clone(),
                ),
            )
            .await
            .unwrap()
            .unwrap();
            let (received_headers, received_body) = captured_rx.recv().await.unwrap();
            for (name, _) in passthrough {
                assert_eq!(
                    received_headers.get_all(name).iter().collect::<Vec<_>>(),
                    headers.get_all(name).iter().collect::<Vec<_>>(),
                    "{name}"
                );
            }
            let received = crate::request::decode_body(&received_body, &received_headers).unwrap();
            let mut expected: Value = serde_json::from_slice(json).unwrap();
            expected["client_metadata"] =
                serde_json::json!({"x-codex-installation-id":"test-installation"});
            assert_eq!(received, expected);
            assert_eq!(received_headers["content-encoding"], "zstd");
            for name in [
                "content-type",
                "accept",
                "session-id",
                "thread-id",
                "x-codex-turn-state",
            ] {
                assert_eq!(received_headers.get(name), headers.get(name), "{name}");
            }
            assert_eq!(received_headers["authorization"], "Bearer test-token");
            assert_eq!(received_headers["originator"], "codex_cli_rs");
            assert_eq!(received_headers["version"], "0.154.0");
            for name in ["forwarded", "via", "x-forwarded-for", "x-custom"] {
                assert!(!received_headers.contains_key(name), "{name}");
            }
            assert_eq!(received_headers["user-agent"], expected_ua);
            assert_eq!(
                received_headers["chatgpt-account-id"],
                "test-chatgpt-account"
            );
            assert!(!received_headers.contains_key("x-codex-installation-id"));
            assert!(!received_headers.contains_key("cookie"));
            assert_eq!(received_headers["x-client-request-id"], "original-thread");
            assert!(!received_headers.contains_key("x-codex-window-id"));
            assert_eq!(response.headers()["x-codex-turn-state"], "next-turn-state");
            assert_eq!(
                response.bytes().await.unwrap().as_ref(),
                b": heartbeat\r\n\r\ndata: {\"type\":\"response.completed\"}\r\n\r\n"
            );
        }
        server.abort();
    }
}
