use crate::request::{
    PreparedRequest, decode_body, normalize_protocol_headers, normalize_response_identity,
};
use crate::{Result, UpstreamClient, UpstreamError};
use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealtimeKind {
    Realtime,
    Live,
    CodexSideband,
    /// Desktop renderer-owned call creation, separate from the CLI Codex route.
    Wham,
}

pub fn realtime_url(
    kind: RealtimeKind,
    call_id: Option<&str>,
    query: Option<&str>,
) -> Result<reqwest::Url> {
    if kind == RealtimeKind::Wham {
        return Err(UpstreamError::InvalidRequest(
            "WHAM realtime creates calls; attach using the returned call ID".into(),
        ));
    }
    if kind == RealtimeKind::CodexSideband {
        let id = call_id
            .ok_or_else(|| UpstreamError::InvalidRequest("Missing realtime call ID".into()))?;
        crate::backend::validate_segment(id)?;
        let mut url = reqwest::Url::parse("wss://chatgpt.com/backend-api/codex/").unwrap();
        url.path_segments_mut().unwrap().pop_if_empty().push(id);
        append_realtime_query(&mut url, query)?;
        return Ok(url);
    }
    let path = if kind == RealtimeKind::Live {
        "live"
    } else {
        "realtime"
    };
    let mut url = reqwest::Url::parse(&format!("wss://api.openai.com/v1/{path}")).unwrap();
    if let Some(id) = call_id {
        crate::backend::validate_segment(id)?;
        url.path_segments_mut().unwrap().push(id);
    }
    append_realtime_query(&mut url, query)?;
    Ok(url)
}

pub(crate) fn append_realtime_query(url: &mut reqwest::Url, query: Option<&str>) -> Result<()> {
    if let Some(query) = query {
        let parsed = reqwest::Url::parse(&format!("https://local/?{query}"))
            .map_err(|_| UpstreamError::InvalidRequest("Invalid realtime query".into()))?;
        for (key, value) in parsed.query_pairs() {
            if matches!(
                key.as_ref(),
                "model" | "intent" | "architecture" | "call_id"
            ) {
                url.query_pairs_mut().append_pair(&key, &value);
            }
        }
    }
    Ok(())
}

pub(crate) async fn prepare_call(
    body: Bytes,
    inbound: &HeaderMap,
    installation_id: &str,
) -> Result<PreparedRequest> {
    let content_type = inbound
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json");
    let mut headers = normalize_protocol_headers(inbound, installation_id)?;
    add_realtime_headers(&mut headers, inbound);
    if content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .eq_ignore_ascii_case("application/sdp")
    {
        std::str::from_utf8(&body)
            .map_err(|_| UpstreamError::InvalidRequest("Invalid SDP text".into()))?;
        headers.insert("content-type", HeaderValue::from_static("application/sdp"));
        return Ok(PreparedRequest { body, headers });
    }
    let mut value = if content_type
        .to_ascii_lowercase()
        .starts_with("multipart/form-data")
    {
        let boundary = multer::parse_boundary(content_type)
            .map_err(|_| UpstreamError::InvalidRequest("Invalid multipart boundary".into()))?;
        let stream = futures::stream::once(async move { Ok::<_, std::io::Error>(body) });
        let mut multipart = multer::Multipart::new(stream, boundary);
        let mut sdp = None;
        let mut session = None;
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|_| UpstreamError::InvalidRequest("Invalid realtime multipart body".into()))?
        {
            let name = field.name().unwrap_or("").to_string();
            let text = field
                .text()
                .await
                .map_err(|_| UpstreamError::InvalidRequest("Invalid realtime field".into()))?;
            match name.as_str() {
                "sdp" if sdp.is_none() => sdp = Some(text),
                "session" if session.is_none() => {
                    session = Some(serde_json::from_str::<Value>(&text).map_err(|_| {
                        UpstreamError::InvalidRequest("Invalid session JSON".into())
                    })?)
                }
                _ => {
                    return Err(UpstreamError::InvalidRequest(
                        "Unexpected or duplicate realtime field".into(),
                    ));
                }
            }
        }
        json!({"sdp":sdp.ok_or_else(|| UpstreamError::InvalidRequest("sdp is required".into()))?,
            "session":session.ok_or_else(|| UpstreamError::InvalidRequest("session is required".into()))?})
    } else {
        decode_body(&body, inbound)?
    };
    if value.get("sdp").and_then(Value::as_str).is_none()
        || !value.get("session").is_some_and(Value::is_object)
    {
        return Err(UpstreamError::InvalidRequest(
            "Realtime call requires sdp and session".into(),
        ));
    }
    let session = value.get_mut("session").unwrap();
    session.as_object_mut().unwrap().remove("id");
    if session.get("client_metadata").is_some() {
        normalize_response_identity(session, installation_id, &HeaderMap::new())?;
    }
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    Ok(PreparedRequest {
        body: Bytes::from(serde_json::to_vec(&value)?),
        headers,
    })
}

/// Extract the model from the same SDP/JSON/multipart decoder used for forwarding.
#[derive(Default)]
pub struct RealtimeSessionModels {
    pub model: Option<String>,
    pub transcription_model: Option<String>,
}

pub async fn realtime_call_models(
    body: Bytes,
    inbound: &HeaderMap,
) -> Result<RealtimeSessionModels> {
    let prepared = prepare_call(body, inbound, "").await?;
    if prepared
        .headers
        .get("content-type")
        .is_some_and(|v| v == "application/sdp")
    {
        return Ok(RealtimeSessionModels::default());
    }
    let value: Value = serde_json::from_slice(&prepared.body)?;
    if value
        .pointer("/session/audio/input/transcription")
        .is_some_and(|v| !v.is_null())
        && value
            .pointer("/session/audio/input/transcription/model")
            .is_none_or(|v| !v.is_string())
    {
        return Err(UpstreamError::InvalidRequest(
            "An explicit transcription model is required".into(),
        ));
    }
    let model = |path| match value.pointer(path) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(model)) => Ok(Some(model.clone())),
        _ => Err(UpstreamError::InvalidRequest(
            "Session model must be a string".into(),
        )),
    };
    Ok(RealtimeSessionModels {
        model: model("/session/model")?,
        transcription_model: model("/session/audio/input/transcription/model")?,
    })
}

pub(crate) fn add_realtime_headers(headers: &mut HeaderMap, inbound: &HeaderMap) {
    if let Some(id) = inbound.get("x-session-id") {
        headers.insert("x-session-id", id.clone());
    }
    if let Some(alpha) = inbound
        .get("openai-alpha")
        .filter(|v| *v == "quicksilver=v1" || *v == "quicksilver=v2")
    {
        headers.insert("openai-alpha", alpha.clone());
    }
}

impl UpstreamClient {
    pub async fn forward_realtime_call(
        &self,
        kind: RealtimeKind,
        query: Option<&str>,
        body: Bytes,
        inbound: HeaderMap,
    ) -> Result<reqwest::Response> {
        let url = realtime_call_url(kind, query)?;
        let prepared = prepare_call(body, &inbound, &self.identity().installation_id).await?;
        self.send_prepared(
            Method::POST,
            url.as_str(),
            prepared,
            kind != RealtimeKind::Wham,
        )
        .await
    }

    pub(crate) async fn realtime_auth_headers(
        &self,
        sideband: bool,
    ) -> Result<(HeaderMap, String)> {
        if sideband {
            return self.authenticated_headers();
        }
        let auth = self.auth_service().ok_or_else(|| {
            UpstreamError::InvalidRequest("Standalone realtime requires an account API key".into())
        })?;
        let ctx = auth
            .accounts()
            .load_context(&self.identity().account_id)
            .await?;
        let api_key = ctx.auth.as_ref().and_then(|a| a.openai_api_key.as_deref()).filter(|k| !k.is_empty())
            .ok_or_else(|| UpstreamError::InvalidRequest("Standalone realtime requires an account API key; this account has no exchanged API key".into()))?;
        let mut headers = self.default_headers()?;
        headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {api_key}"))?,
        );
        headers.remove("chatgpt-account-id");
        Ok((headers, api_key.into()))
    }
}

fn realtime_call_url(kind: RealtimeKind, query: Option<&str>) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(if kind == RealtimeKind::Wham {
        "https://chatgpt.com/backend-api/wham/realtime/calls"
    } else {
        "https://chatgpt.com/backend-api/codex/realtime/calls"
    })
    .unwrap();
    append_realtime_query(&mut url, query)?;
    if kind == RealtimeKind::Live {
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(k, _)| k != "intent" && k != "architecture")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        url.set_query(None);
        url.query_pairs_mut()
            .extend_pairs(pairs)
            .append_pair("intent", "quicksilver")
            .append_pair("architecture", "avas");
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn desktop_wham_calls_keep_the_actual_request_and_return_sdp_and_call_id() {
        use codex2api_accounts::{AccountIdentity, HostRuntime};
        use std::io::Write;
        use std::sync::Arc;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};

        let archive = std::env::var("CODEX2API_TEST_DESKTOP_ASAR").ok();
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/windows/Test-DesktopRealtimeContract.cjs");
        let requests = if let Some(archive) = &archive {
            let output = std::process::Command::new("node")
                .arg(&script)
                .arg(archive)
                .arg("--requests")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            serde_json::from_slice::<Value>(&output.stdout).unwrap()["requests"]
                .as_array()
                .unwrap()
                .clone()
        } else {
            ["quicksilver=v1", "quicksilver=v2"].into_iter().map(|alpha| json!({
                "path":"/wham/realtime/calls?intent=quicksilver&architecture=avas",
                "headers":{"openai-alpha":alpha,"thread-id":"thread-fixture","session-id":"session-fixture"},
                "body":{"sdp":"v=0\r\ns=offer\r\n","session":{"model":"gpt-realtime-1.5","audio":{"output":{"voice":"marin"}}}}
            })).collect()
        };
        crate::proxy_fixture::server_tls();
        let cert = CertificateDer::from_pem_slice(include_bytes!(
            "../../codex2api-auth/tests/fixtures/proxy-chatgpt-server.pem"
        ))
        .unwrap();
        let key = PrivateKeyDer::from_pem_slice(include_bytes!(
            "../../codex2api-auth/tests/fixtures/proxy-server-key.pem"
        ))
        .unwrap();
        let tls = tokio_rustls::TlsAcceptor::from(Arc::new(
            tokio_rustls::rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![cert], key)
                .unwrap(),
        ));
        let (listener, address) = crate::proxy_fixture::listener().await;
        let (tx, mut rx) = tokio::sync::mpsc::channel(2);
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut tcp, _) = listener.accept().await.unwrap();
                assert_eq!(
                    crate::proxy_fixture::http_tunnel(&mut tcp).await,
                    "chatgpt.com:443"
                );
                let mut socket = tls.accept(tcp).await.unwrap();
                let headers = crate::proxy_fixture::read_headers(&mut socket)
                    .await
                    .unwrap()
                    .to_ascii_lowercase();
                let size: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                let mut body = vec![0; size];
                socket.read_exact(&mut body).await.unwrap();
                tx.send((headers, serde_json::from_slice::<Value>(&body).unwrap()))
                    .await
                    .unwrap();
                let answer = "v=0\r\ns=fixture\r\n";
                socket.write_all(format!("HTTP/1.1 201 Created\r\nContent-Type: application/sdp\r\nContent-Length: {}\r\nLocation: /backend-api/wham/realtime/calls/rtc_fixture\r\nConnection: close\r\n\r\n{answer}", answer.len()).as_bytes()).await.unwrap();
            }
        });
        let mut client = UpstreamClient::new(
            AccountIdentity::new("fixture", "installation-fixture", HostRuntime::generate()),
            "supplier-fixture".into(),
            Some("workspace-fixture".into()),
        )
        .unwrap();
        let proxy = format!("http://user:pass@{address}");
        let mut clients = codex2api_auth::transport::AccountHttpClients::with_proxy(
            client.identity(),
            Some(&proxy),
        )
        .unwrap();
        // Keep fixture trust local to this client, so parallel tests retain
        // their own trust roots and transport configuration.
        clients.api = codex2api_auth::transport::http_builder()
            .unwrap()
            .use_rustls_tls()
            .proxy(reqwest::Proxy::all(proxy).unwrap())
            .add_root_certificate(
                reqwest::Certificate::from_pem(include_bytes!(
                    "../../codex2api-auth/tests/fixtures/proxy-chatgpt-ca.pem"
                ))
                .unwrap(),
            )
            .cookie_provider(clients.cookies.clone())
            .build()
            .unwrap();
        client.use_account_http(Arc::new(clients));
        for request in requests {
            let mut headers = HeaderMap::new();
            for (key, value) in request["headers"].as_object().unwrap() {
                headers.insert(
                    http::HeaderName::from_bytes(key.as_bytes()).unwrap(),
                    HeaderValue::from_str(value.as_str().unwrap()).unwrap(),
                );
            }
            let response = client
                .forward_realtime_call(
                    RealtimeKind::Wham,
                    request["path"]
                        .as_str()
                        .unwrap()
                        .split_once('?')
                        .map(|(_, q)| q),
                    Bytes::from(request["body"].to_string()),
                    headers.clone(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), 201);
            let location = response.headers()["location"].to_str().unwrap().to_owned();
            let answer = response.text().await.unwrap();
            let (wire, body) = rx.recv().await.unwrap();
            assert!(wire.starts_with(
                "post /backend-api/wham/realtime/calls?intent=quicksilver&architecture=avas "
            ));
            assert!(wire.contains("authorization: bearer supplier-fixture"));
            assert!(wire.contains("chatgpt-account-id: workspace-fixture"));
            assert!(!wire.contains("\r\nversion:") && !wire.contains("\r\noriginator:"));
            assert!(wire.contains(&format!(
                "openai-alpha: {}",
                headers["openai-alpha"].to_str().unwrap()
            )));
            assert_eq!(body, request["body"]);
            assert_eq!(answer, "v=0\r\ns=fixture\r\n");
            if let Some(archive) = &archive {
                let mut child = std::process::Command::new("node")
                    .arg(&script)
                    .arg(archive)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .unwrap();
                child.stdin.take().unwrap().write_all(json!({"status":201,"body":answer,"location":location,"call_id":"rtc_fixture"}).to_string().as_bytes()).unwrap();
                let output = child.wait_with_output().unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        server.await.unwrap();
    }
    #[tokio::test]
    async fn realtime_call_supports_sdp_json_and_official_multipart() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-oai-attestation",
            HeaderValue::from_static("attestation-original"),
        );
        headers.insert("traceparent", HeaderValue::from_static("trace-original"));
        headers.insert("content-type", HeaderValue::from_static("application/sdp"));
        let raw = Bytes::from_static(b"v=0\r\ns=call\r\n");
        let result = prepare_call(raw.clone(), &headers, "account")
            .await
            .unwrap();
        assert_eq!(result.body, raw);
        assert_eq!(
            result.headers["x-oai-attestation"],
            headers["x-oai-attestation"]
        );
        assert_eq!(result.headers["traceparent"], headers["traceparent"]);
        let json = json!({"sdp":"v=0\r\n", "session":{"id":"remove-on-create", "type":"realtime", "instructions":"preserve", "client_metadata":{"x-codex-installation-id":"caller"}}});
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        let result = prepare_call(Bytes::from(json.to_string()), &headers, "account")
            .await
            .unwrap();
        assert_eq!(
            result.headers["x-oai-attestation"],
            headers["x-oai-attestation"]
        );
        assert_eq!(result.headers["traceparent"], headers["traceparent"]);
        let result: Value = serde_json::from_slice(&result.body).unwrap();
        assert!(result["session"].get("id").is_none());
        assert_eq!(result["session"]["instructions"], "preserve");
        assert_eq!(
            result["session"]["client_metadata"]["x-codex-installation-id"],
            "account"
        );
        headers.insert(
            "content-type",
            HeaderValue::from_static("multipart/form-data; boundary=codex-realtime-call-boundary"),
        );
        let body = Bytes::from_static(b"--codex-realtime-call-boundary\r\nContent-Disposition: form-data; name=\"sdp\"\r\nContent-Type: application/sdp\r\n\r\nv=0\r\n--codex-realtime-call-boundary\r\nContent-Disposition: form-data; name=\"session\"\r\nContent-Type: application/json\r\n\r\n{\"type\":\"realtime\"}\r\n--codex-realtime-call-boundary--\r\n");
        let result = prepare_call(body, &headers, "account").await.unwrap();
        assert_eq!(
            result.headers["x-oai-attestation"],
            headers["x-oai-attestation"]
        );
        assert_eq!(result.headers["traceparent"], headers["traceparent"]);
        assert_eq!(
            serde_json::from_slice::<Value>(&result.body).unwrap(),
            json!({"sdp":"v=0","session":{"type":"realtime"}})
        );
        assert!(
            prepare_call(Bytes::from_static(b"bad"), &headers, "account")
                .await
                .is_err()
        );
    }
    #[test]
    fn realtime_sideband_paths_queries_and_account_identity_are_bounded() {
        let url = realtime_url(
            RealtimeKind::Live,
            Some("call-1"),
            Some("model=test&intent=quicksilver&authorization=caller"),
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "wss://api.openai.com/v1/live/call-1?model=test&intent=quicksilver"
        );
        assert!(realtime_url(RealtimeKind::Live, Some("../other"), None).is_err());
        assert_eq!(
            realtime_url(RealtimeKind::Realtime, None, Some("call_id=call-1"))
                .unwrap()
                .path(),
            "/v1/realtime"
        );
        assert_eq!(
            realtime_call_url(
                RealtimeKind::Wham,
                Some("intent=quicksilver&architecture=avas&authorization=caller")
            )
            .unwrap()
            .as_str(),
            "https://chatgpt.com/backend-api/wham/realtime/calls?intent=quicksilver&architecture=avas",
        );
        assert_eq!(
            realtime_call_url(RealtimeKind::Realtime, None)
                .unwrap()
                .as_str(),
            "https://chatgpt.com/backend-api/codex/realtime/calls",
        );
        assert!(realtime_url(RealtimeKind::Wham, None, None).is_err());
    }
}
