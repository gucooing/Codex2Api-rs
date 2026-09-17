use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method};

use crate::request::{
    PreparedRequest, decode_body, normalize_protocol_headers, normalize_response_identity,
    prepare_responses,
};
use crate::{Result, UpstreamClient};
use codex2api_version::{CHATGPT_CODEX_BASE_URL, CODEX_PACKAGE_VERSION};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endpoint {
    Responses,
    Guardian,
    GuardianClassifier,
    Models,
    Usage,
    MemorySummary,
    Search,
    ImageGeneration,
    ImageEdit,
}

impl Endpoint {
    pub fn codex_path(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::Guardian => "guardian",
            Self::GuardianClassifier => "guardian-classifier",
            Self::Models => "models",
            Self::Usage => "wham/usage",
            Self::MemorySummary => "memories/trace_summarize",
            Self::Search => "alpha/search",
            Self::ImageGeneration => "images/generations",
            Self::ImageEdit => "images/edits",
        }
    }

    pub fn url(self) -> String {
        match self {
            Self::Usage => "https://chatgpt.com/backend-api/wham/usage".into(),
            Self::Models => {
                format!("{CHATGPT_CODEX_BASE_URL}/models?client_version={CODEX_PACKAGE_VERSION}")
            }
            _ => format!("{CHATGPT_CODEX_BASE_URL}/{}", self.codex_path()),
        }
    }

    pub fn is_response(self) -> bool {
        matches!(
            self,
            Self::Responses | Self::Guardian | Self::GuardianClassifier
        )
    }

    pub fn method(self) -> Method {
        if matches!(self, Self::Models | Self::Usage) {
            Method::GET
        } else {
            Method::POST
        }
    }

    fn prepare(
        self,
        body: &[u8],
        inbound: &HeaderMap,
        installation_id: &str,
        timezone: Option<&str>,
    ) -> Result<PreparedRequest> {
        if self.is_response() {
            let mut prepared = prepare_responses(body, inbound, installation_id, timezone)?;
            if self != Self::Responses && !inbound.contains_key("x-codex-routing-hint") {
                prepared.headers.remove("x-codex-routing-hint");
            }
            return Ok(prepared);
        }
        if self.method() == Method::GET {
            return Ok(PreparedRequest {
                body: Bytes::new(),
                headers: normalize_protocol_headers(inbound, installation_id)?,
            });
        }
        let mut value = decode_body(body, inbound)?;
        let mut headers = if value.get("client_metadata").is_some() {
            normalize_response_identity(&mut value, installation_id, inbound)?
        } else {
            normalize_protocol_headers(inbound, installation_id)?
        };
        if !inbound.contains_key("x-codex-routing-hint") {
            headers.remove("x-codex-routing-hint");
        }
        if matches!(self, Self::ImageGeneration | Self::ImageEdit) {
            // ext/image-generation/src/backend.rs: this identifies the calling turn,
            // not the account, and must survive fingerprint replacement.
            if let Some(turn_id) = inbound.get("x-codex-image-turn-id") {
                headers.insert("x-codex-image-turn-id", turn_id.clone());
            }
        }
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        Ok(PreparedRequest {
            body: Bytes::from(serde_json::to_vec(&value)?),
            headers,
        })
    }
}

impl UpstreamClient {
    pub async fn forward_endpoint(
        &self,
        endpoint: Endpoint,
        body: Bytes,
        inbound: HeaderMap,
    ) -> Result<reqwest::Response> {
        let installation_id = self.identity().installation_id.clone();
        let timezone = self.identity().http_fingerprint.timezone.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            endpoint.prepare(&body, &inbound, &installation_id, timezone.as_deref())
        })
        .await
        .map_err(|e| crate::UpstreamError::InvalidRequest(e.to_string()))??;
        self.send_prepared(
            endpoint.method(),
            &endpoint.url(),
            prepared,
            endpoint != Endpoint::Usage,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Uri, response::Response};
    use codex2api_accounts::{AccountIdentity, HostRuntime};
    use serde_json::{Value, json};

    #[test]
    fn endpoints_and_version_are_official_and_not_caller_selected() {
        assert_eq!(
            Endpoint::Models.url(),
            "https://chatgpt.com/backend-api/codex/models?client_version=0.154.0"
        );
        assert_eq!(
            Endpoint::Usage.url(),
            "https://chatgpt.com/backend-api/wham/usage"
        );
        assert_eq!(
            Endpoint::Responses.url(),
            "https://chatgpt.com/backend-api/codex/responses"
        );
        assert_eq!(
            Endpoint::MemorySummary.codex_path(),
            "memories/trace_summarize"
        );
    }

    #[test]
    fn all_endpoints_preserve_official_headers_without_replacing_routing_hints() {
        let mut inbound = HeaderMap::new();
        for (name, value) in [
            ("traceparent", "trace-original"),
            ("x-oai-attestation", "attestation-original"),
            ("openai-project", "proj-original"),
            ("x-codex-routing-hint", "model=test;tier=priority"),
            ("authorization", "Bearer caller"),
            (
                "x-codex-turn-metadata",
                r#"{"installation_id":"caller","turn_id":"turn"}"#,
            ),
        ] {
            inbound.insert(name, HeaderValue::from_static(value));
        }
        for endpoint in [
            Endpoint::Responses,
            Endpoint::Guardian,
            Endpoint::GuardianClassifier,
            Endpoint::Models,
            Endpoint::Usage,
            Endpoint::MemorySummary,
            Endpoint::Search,
            Endpoint::ImageGeneration,
            Endpoint::ImageEdit,
        ] {
            let prepared = endpoint
                .prepare(br#"{"model":"test","input":[]}"#, &inbound, "account", None)
                .unwrap();
            for name in [
                "traceparent",
                "x-oai-attestation",
                "openai-project",
                "x-codex-routing-hint",
            ] {
                assert_eq!(
                    prepared.headers[name], inbound[name],
                    "{endpoint:?}: {name}"
                );
            }
            assert!(!prepared.headers.contains_key("authorization"));
            let turn: Value =
                serde_json::from_str(prepared.headers["x-codex-turn-metadata"].to_str().unwrap())
                    .unwrap();
            assert_eq!(turn, json!({"installation_id":"account","turn_id":"turn"}));
            if endpoint.method() == Method::GET {
                assert!(prepared.body.is_empty());
            }
        }
    }

    #[tokio::test]
    async fn search_and_images_preserve_payloads_and_official_turn_headers() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(3);
        let app = Router::new().fallback(move |uri: Uri, headers: HeaderMap, body: Bytes| {
            let tx = tx.clone();
            async move {
                let image = uri.path().contains("/images/");
                tx.send((uri, headers, body)).await.unwrap();
                // Keep each loopback fixture exchange independent of pooled-socket resets.
                let mut response = Response::builder()
                    .header("content-type", "application/json")
                    .header("connection", "close");
                if image {
                    response = response.header("x-codex-imagegen-request-id", "image-request");
                }
                response.body(Body::from(if image {
                    r#"{"created":1,"data":[{"b64_json":"aW1hZ2U=","generation_id":"image-1"}]}"#
                } else {
                    r#"{"output":"search result","encrypted_output":"ciphertext","results":[]}"#
                })).unwrap()
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = UpstreamClient::new(
            AccountIdentity::new("account", "account-installation", HostRuntime::generate()),
            "account-token".into(),
            Some("chatgpt-account".into()),
        )
        .unwrap()
        .with_direct_test_http();
        for (endpoint, path, body) in [
            (
                Endpoint::Search,
                "alpha/search",
                json!({
                    "id":"search-session", "model":"test", "input":"find this",
                    "commands":{"search_query":[{"q":"query","domains":["example.com"]}]},
                    "max_output_tokens":2500
                }),
            ),
            (
                Endpoint::ImageGeneration,
                "images/generations",
                json!({
                    "model":"test-image", "prompt":"a landscape", "n":1, "size":"1024x1024", "quality":"high"
                }),
            ),
            (
                Endpoint::ImageEdit,
                "images/edits",
                json!({
                    "model":"test-image", "prompt":"change the background", "images":[{"image_url":"data:image/png;base64,aW1hZ2U="}],
                    "background":"transparent", "quality":"auto"
                }),
            ),
        ] {
            assert_eq!(
                endpoint.url(),
                format!("https://chatgpt.com/backend-api/codex/{path}")
            );
            let mut inbound = HeaderMap::new();
            inbound.insert(
                "x-codex-turn-metadata",
                HeaderValue::from_static(r#"{"installation_id":"caller","turn_id":"turn-1"}"#),
            );
            inbound.insert(
                "x-codex-image-turn-id",
                HeaderValue::from_static("image-turn-1"),
            );
            inbound.insert(
                "authorization",
                HeaderValue::from_static("Bearer proxy-key"),
            );
            inbound.insert("x-forwarded-for", HeaderValue::from_static("private"));
            let prepared = endpoint
                .prepare(
                    &serde_json::to_vec(&body).unwrap(),
                    &inbound,
                    "account-installation",
                    None,
                )
                .unwrap();
            let url = format!("{base}/backend-api/codex/{path}");
            let response = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                client.send_prepared(endpoint.method(), &url, prepared, true),
            )
            .await
            .unwrap()
            .unwrap();
            let (uri, headers, received) = rx.recv().await.unwrap();
            assert_eq!(uri.path(), format!("/backend-api/codex/{path}"));
            assert_eq!(serde_json::from_slice::<Value>(&received).unwrap(), body);
            assert_eq!(headers["content-type"], "application/json");
            assert!(!headers.contains_key("content-encoding"));
            assert!(!headers.contains_key("x-forwarded-for"));
            assert_eq!(headers["authorization"], "Bearer account-token");
            let turn: Value =
                serde_json::from_str(headers["x-codex-turn-metadata"].to_str().unwrap()).unwrap();
            assert_eq!(
                turn,
                json!({"installation_id":"account-installation", "turn_id":"turn-1"})
            );
            if endpoint == Endpoint::Search {
                assert!(!headers.contains_key("x-codex-image-turn-id"));
                let output = response.json::<Value>().await.unwrap();
                assert_eq!(output["encrypted_output"], "ciphertext");
            } else {
                assert_eq!(headers["x-codex-image-turn-id"], "image-turn-1");
                assert_eq!(
                    response.headers()["x-codex-imagegen-request-id"],
                    "image-request"
                );
                let output = response.json::<Value>().await.unwrap();
                assert_eq!(output["data"][0]["b64_json"], "aW1hZ2U=");
                assert_eq!(output["data"][0]["generation_id"], "image-1");
            }
        }
        server.abort();
    }
}
