use std::time::Duration;

use http::HeaderMap;
use reqwest::cookie::CookieStore;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};
use tungstenite::extensions::{ExtensionsConfig, compression::deflate::DeflateConfig};
use tungstenite::protocol::WebSocketConfig;

use crate::{Endpoint, Result, UpstreamClient, UpstreamError, normalize_response_identity};

pub type UpstreamWebSocket = WebSocketStream<MaybeTlsStream<MaybeTlsStream<TcpStream>>>;

pub fn websocket_config() -> WebSocketConfig {
    let mut extensions = ExtensionsConfig::default();
    extensions.permessage_deflate = Some(DeflateConfig::default());
    let mut config = WebSocketConfig::default();
    config.extensions = extensions;
    config
}

impl UpstreamClient {
    pub async fn connect_websocket(
        &self,
        endpoint: Endpoint,
        inbound: HeaderMap,
    ) -> Result<(
        UpstreamWebSocket,
        HeaderMap,
        Option<crate::WorkspaceConnection>,
    )> {
        if !endpoint.is_response() {
            return Err(UpstreamError::InvalidRequest(
                "Endpoint does not support WebSocket.".into(),
            ));
        }
        self.connect_socket_inner(
            &endpoint.url().replacen("https://", "wss://", 1),
            endpoint.response_headers(inbound),
            None,
            true,
            true,
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn connect_websocket_to(
        &self,
        url: &str,
        inbound: HeaderMap,
    ) -> Result<(UpstreamWebSocket, HeaderMap)> {
        self.connect_socket(url, inbound, None, true).await
    }

    pub async fn connect_realtime(
        &self,
        kind: crate::RealtimeKind,
        call_id: Option<&str>,
        query: Option<&str>,
        inbound: HeaderMap,
    ) -> Result<(UpstreamWebSocket, HeaderMap)> {
        let url = crate::realtime_url(kind, call_id, query)?;
        let sideband = call_id.is_some() || url.query_pairs().any(|(key, _)| key == "call_id");
        self.connect_socket(url.as_str(), inbound, Some(kind), sideband)
            .await
    }

    pub(crate) async fn connect_socket(
        &self,
        url: &str,
        inbound: HeaderMap,
        realtime: Option<crate::RealtimeKind>,
        sideband: bool,
    ) -> Result<(UpstreamWebSocket, HeaderMap)> {
        let (socket, headers, _) = self
            .connect_socket_inner(url, inbound, realtime, sideband, false)
            .await?;
        Ok((socket, headers))
    }

    async fn connect_socket_inner(
        &self,
        url: &str,
        inbound: HeaderMap,
        realtime: Option<crate::RealtimeKind>,
        sideband: bool,
        workspace: bool,
    ) -> Result<(
        UpstreamWebSocket,
        HeaderMap,
        Option<crate::WorkspaceConnection>,
    )> {
        self.synchronize_auth().await?;
        let mut metadata = serde_json::json!({});
        let mut extra =
            normalize_response_identity(&mut metadata, &self.identity().installation_id, &inbound)?;
        if let Some(kind) = realtime {
            crate::realtime::add_realtime_headers(&mut extra, &inbound);
            if matches!(
                kind,
                crate::RealtimeKind::Live | crate::RealtimeKind::CodexSideband
            ) {
                extra.insert(
                    "openai-alpha",
                    http::HeaderValue::from_static("quicksilver=v2"),
                );
            }
        }
        let tls = tokio::task::spawn_blocking(codex2api_auth::transport::websocket_tls_config)
            .await
            .map_err(|e| UpstreamError::Stream(e.to_string()))??;
        let mut retried = false;
        loop {
            let auth = self.request_auth()?;
            let routing = if workspace {
                match self.workspace_route(&auth, false).await {
                    Ok(route) => Some(route),
                    Err(error)
                        if error.is_unauthorized() && !retried && self.auth_service().is_some() =>
                    {
                        retried = true;
                        self.refresh_access_token(&auth.access_token).await?;
                        continue;
                    }
                    Err(error) => {
                        if error.is_unauthorized() {
                            self.record_http_status(401).await;
                        }
                        return Err(error);
                    }
                }
            } else {
                None
            };
            let (mut headers, token) = if realtime.is_some() {
                self.realtime_auth_headers(sideband).await?
            } else {
                (auth.headers, auth.access_token)
            };
            headers.extend(extra.clone());
            let destination = if let Some(route) = &routing {
                route.route.apply(url, &mut headers)?
            } else {
                url.to_owned()
            };
            let cookie_url = destination
                .replacen("wss://", "https://", 1)
                .replacen("ws://", "http://", 1)
                .parse::<reqwest::Url>()
                .map_err(|e| UpstreamError::Stream(e.to_string()))?;
            let mut request = destination
                .as_str()
                .into_client_request()
                .map_err(|e| UpstreamError::Stream(e.to_string()))?;
            request.headers_mut().extend(headers);
            if realtime.is_none() {
                request.headers_mut().entry("openai-beta").or_insert(
                    http::HeaderValue::from_static("responses_websockets=2026-02-06"),
                );
            }
            if let Some(cookies) = self.cookies.cookies(&cookie_url) {
                request.headers_mut().insert("cookie", cookies);
            }
            let connected = tokio::time::timeout(Duration::from_secs(30), async {
                let transport =
                    crate::proxy::connect(&destination, self.account_http.proxy_url(), tls.clone())
                        .await?;
                tokio_tungstenite::client_async_tls_with_config(
                    request,
                    transport,
                    Some(if realtime.is_some() {
                        WebSocketConfig::default()
                    } else {
                        websocket_config()
                    }),
                    Some(Connector::Rustls(tls.clone())),
                )
                .await
            })
            .await;
            let connected = match connected {
                Ok(value) => value,
                Err(_) => {
                    self.record_communication_error("与 ChatGPT 官方 WebSocket 连接超时")
                        .await;
                    return Err(UpstreamError::Stream(
                        "WebSocket connection timed out.".into(),
                    ));
                }
            };
            match connected {
                Ok((socket, response)) => {
                    self.cookies.set_cookies(
                        &mut response.headers().get_all("set-cookie").iter(),
                        &cookie_url,
                    );
                    let connection = routing.and_then(|route| route.connection);
                    if let Some(connection) = &connection {
                        connection.check_current().await?;
                    }
                    return Ok((socket, response.headers().clone(), connection));
                }
                Err(tungstenite::Error::Http(response)) => {
                    self.cookies.set_cookies(
                        &mut response.headers().get_all("set-cookie").iter(),
                        &cookie_url,
                    );
                    if response.status() == http::StatusCode::UNAUTHORIZED
                        && !retried
                        && sideband
                        && self.auth_service().is_some()
                    {
                        retried = true;
                        if self.refresh_access_token(&token).await.is_ok() {
                            continue;
                        }
                        // Keep the rejected handshake's status, body and headers
                        // when auth recovery itself cannot repair the request.
                    }
                    self.record_http_status(response.status().as_u16()).await;
                    return Err(UpstreamError::status_with_headers(
                        response.status(),
                        String::from_utf8_lossy(response.body().as_deref().unwrap_or_default()),
                        response.headers().clone(),
                    ));
                }
                Err(error) => {
                    self.record_communication_error("与 ChatGPT 官方 WebSocket 通信失败")
                        .await;
                    return Err(UpstreamError::Stream(error.to_string()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex2api_accounts::{AccountIdentity, HostRuntime};
    use futures::{SinkExt, StreamExt};

    #[tokio::test]
    async fn failed_secure_handshake_updates_only_the_current_accounts_infrastructure_cookies() {
        if std::env::var_os("CODEX2API_WS_COOKIE_CHILD").is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "websocket::tests::failed_secure_handshake_updates_only_the_current_accounts_infrastructure_cookies", "--nocapture"])
                .env("CODEX2API_WS_COOKIE_CHILD", "1")
                .env("CODEX_CA_CERTIFICATE", concat!(env!("CARGO_MANIFEST_DIR"), "/../codex2api-auth/tests/fixtures/proxy-chatgpt-ca.pem"))
                .output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        use tokio::io::AsyncWriteExt;
        use tokio_rustls::rustls::{
            self,
            pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
        };
        codex2api_auth::transport::ensure_tls_provider();
        let cert = CertificateDer::from_pem_slice(include_bytes!(
            "../../codex2api-auth/tests/fixtures/proxy-chatgpt-server.pem"
        ))
        .unwrap();
        let key = PrivateKeyDer::from_pem_slice(include_bytes!(
            "../../codex2api-auth/tests/fixtures/proxy-server-key.pem"
        ))
        .unwrap();
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(config));
        let (listener, address) = crate::proxy_fixture::listener().await;
        let server = tokio::spawn(async move {
            for index in 0..3 {
                let (mut tcp, _) = listener.accept().await.unwrap();
                assert_eq!(
                    crate::proxy_fixture::http_tunnel(&mut tcp).await,
                    "chatgpt.com:443"
                );
                let mut tls = acceptor.accept(tcp).await.unwrap();
                if index == 0 {
                    crate::proxy_fixture::read_headers(&mut tls).await.unwrap();
                    let body =
                        r#"{"error":{"code":"temporary_denial","message":"fixture denial"}}"#;
                    tls.write_all(format!("HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\nConnection: close\r\nRetry-After: 7\r\nX-Request-Id: request-fixture\r\nX-Error-Json: {{\"error\":{{\"code\":\"temporary_denial\"}}}}\r\nSet-Cookie: __oailb=retry-route; Path=/; Secure\r\nSet-Cookie: chatgpt_session=must-not-store; Path=/; Secure\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
                } else {
                    let mut ws = tokio_tungstenite::accept_hdr_async_with_config(tls, move |request: &tungstenite::handshake::server::Request, response: tungstenite::handshake::server::Response| {
                        if index == 1 {
                            assert_eq!(request.headers()["cookie"], "__oailb=retry-route");
                        } else { assert!(!request.headers().contains_key("cookie")); }
                        Ok(response)
                    }, Some(websocket_config())).await.unwrap();
                    ws.close(None).await.unwrap();
                }
            }
        });
        for account in ["a", "b"] {
            let identity = AccountIdentity::new(
                account,
                format!("install-{account}"),
                HostRuntime::generate(),
            );
            let mut client = UpstreamClient::new(
                identity.clone(),
                format!("token-{account}"),
                Some(account.into()),
            )
            .unwrap();
            client.use_account_http(std::sync::Arc::new(
                codex2api_auth::transport::AccountHttpClients::with_proxy(
                    &identity,
                    Some(&format!("http://user:pass@{address}")),
                )
                .unwrap(),
            ));
            if account == "a" {
                let result = tokio::time::timeout(
                    Duration::from_secs(5),
                    client.connect_websocket_to(
                        "wss://chatgpt.com/backend-api/codex/responses",
                        HeaderMap::new(),
                    ),
                )
                .await
                .unwrap();
                assert!(
                    matches!(result, Err(UpstreamError::Status { status:401, ref body, .. }) if body.contains("temporary_denial"))
                );
                let Err(UpstreamError::Status { headers, .. }) = &result else {
                    unreachable!()
                };
                assert_eq!(headers["retry-after"], "7");
                assert_eq!(headers["x-request-id"], "request-fixture");
                assert_eq!(
                    headers["x-error-json"],
                    r#"{"error":{"code":"temporary_denial"}}"#
                );
                let cookie = client
                    .cookies
                    .cookies(
                        &"https://chatgpt.com/backend-api/codex/responses"
                            .parse()
                            .unwrap(),
                    )
                    .unwrap();
                assert_eq!(cookie, "__oailb=retry-route");
            }
            let (mut ws, _) = tokio::time::timeout(
                Duration::from_secs(5),
                client.connect_websocket_to(
                    "wss://chatgpt.com/backend-api/codex/responses",
                    HeaderMap::new(),
                ),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(matches!(
                ws.next().await,
                Some(Ok(tungstenite::Message::Close(_)))
            ));
        }
        server.await.unwrap();
    }

    #[tokio::test]
    async fn realtime_sideband_uses_realtime_headers_and_preserves_binary_audio() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_hdr_async(
                stream,
                move |req: &tungstenite::handshake::server::Request,
                      response: tungstenite::handshake::server::Response| {
                    tx.send(req.headers().clone()).unwrap();
                    Ok(response)
                },
            )
            .await
            .unwrap();
            socket
                .send(tungstenite::Message::Binary(vec![0xff, 0, 128, 1].into()))
                .await
                .unwrap();
            socket.close(None).await.unwrap();
        });
        let client = UpstreamClient::new(
            AccountIdentity::new("a", "install", HostRuntime::generate()),
            "account-token".into(),
            Some("account-a".into()),
        )
        .unwrap();
        let mut inbound = HeaderMap::new();
        inbound.insert(
            "x-session-id",
            http::HeaderValue::from_static("session-original"),
        );
        inbound.insert("via", http::HeaderValue::from_static("proxy"));
        inbound.insert(
            "x-oai-attestation",
            http::HeaderValue::from_static("attestation-original"),
        );
        inbound.insert(
            "openai-project",
            http::HeaderValue::from_static("proj-original"),
        );
        let (mut socket, _) = tokio::time::timeout(
            Duration::from_secs(5),
            client.connect_socket(
                &format!("ws://{addr}/live/call-1"),
                inbound,
                Some(crate::RealtimeKind::Live),
                true,
            ),
        )
        .await
        .unwrap()
        .unwrap();
        let headers = rx.await.unwrap();
        assert_eq!(headers["authorization"], "Bearer account-token");
        assert_eq!(headers["openai-alpha"], "quicksilver=v2");
        assert_eq!(headers["x-session-id"], "session-original");
        assert_eq!(headers["x-oai-attestation"], "attestation-original");
        assert_eq!(headers["openai-project"], "proj-original");
        assert!(!headers.contains_key("openai-beta"));
        assert!(!headers.contains_key("sec-websocket-extensions"));
        assert!(!headers.contains_key("via"));
        assert_eq!(
            socket.next().await.unwrap().unwrap().into_data().as_ref(),
            &[0xff, 0, 128, 1]
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn websocket_rebuilds_handshake_and_negotiates_official_compression() {
        codex2api_auth::transport::ensure_tls_provider();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_hdr_async_with_config(
                stream,
                move |request: &tungstenite::handshake::server::Request,
                      mut response: tungstenite::handshake::server::Response| {
                    tx.send(request.headers().clone()).unwrap();
                    response.headers_mut().insert(
                        "x-codex-turn-state",
                        http::HeaderValue::from_static("next-state"),
                    );
                    Ok(response)
                },
                Some(websocket_config()),
            )
            .await
            .unwrap();
            socket
                .send(tungstenite::Message::Text(
                    "{\"type\":\"response.completed\"}".into(),
                ))
                .await
                .unwrap();
            socket.close(None).await.unwrap();
        });
        let identity = AccountIdentity::new("a", "install-a", HostRuntime::generate());
        let expected_ua = identity.official_user_agent();
        let client = UpstreamClient::new(
            identity,
            "upstream-token".into(),
            Some("upstream-account".into()),
        )
        .unwrap();
        let mut inbound = HeaderMap::new();
        for (key, value) in [
            ("authorization", "Bearer client"),
            ("version", "999"),
            ("via", "proxy"),
            ("x-forwarded-for", "private"),
            ("thread-id", "thread"),
            ("x-codex-turn-state", "state"),
            ("traceparent", "trace-original"),
            ("x-oai-attestation", "attestation-original"),
            ("x-codex-inference-call-id", "inference-original"),
            (
                "openai-beta",
                "responses_websockets=2026-02-06,caller-option=1",
            ),
            (
                "x-codex-turn-metadata",
                "{\"installation_id\":\"caller\",\"turn_id\":\"turn\"}",
            ),
        ] {
            inbound.insert(key, http::HeaderValue::from_static(value));
        }
        let (mut socket, headers) = tokio::time::timeout(
            Duration::from_secs(10),
            client.connect_websocket_to(&format!("ws://{addr}/responses"), inbound),
        )
        .await
        .unwrap()
        .unwrap();
        let request = rx.await.unwrap();
        assert_eq!(request["authorization"], "Bearer upstream-token");
        assert_eq!(request["version"], "0.157.0");
        assert_eq!(request["user-agent"], expected_ua);
        assert_eq!(request["thread-id"], "thread");
        assert_eq!(request["x-codex-turn-state"], "state");
        assert_eq!(request["traceparent"], "trace-original");
        assert_eq!(request["x-oai-attestation"], "attestation-original");
        assert_eq!(request["x-codex-inference-call-id"], "inference-original");
        assert_eq!(
            request["openai-beta"],
            "responses_websockets=2026-02-06,caller-option=1"
        );
        assert!(
            request["sec-websocket-extensions"]
                .to_str()
                .unwrap()
                .contains("permessage-deflate")
        );
        assert!(!request.contains_key("via"));
        assert!(!request.contains_key("x-forwarded-for"));
        let metadata: serde_json::Value =
            serde_json::from_str(request["x-codex-turn-metadata"].to_str().unwrap()).unwrap();
        assert_eq!(metadata["installation_id"], "install-a");
        assert_eq!(metadata["turn_id"], "turn");
        assert_eq!(headers["x-codex-turn-state"], "next-state");
        let event = socket.next().await.unwrap().unwrap();
        assert_eq!(
            event.into_text().unwrap(),
            "{\"type\":\"response.completed\"}"
        );
        server.await.unwrap();
    }
}
