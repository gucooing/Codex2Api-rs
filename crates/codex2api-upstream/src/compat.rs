//! Additional fixed ChatGPT endpoints used by third-party RT clients.
use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method};

use crate::request::{PreparedRequest, decode_body, normalize_protocol_headers};
use crate::{Endpoint, Result, UpstreamClient, UpstreamError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatgptEndpoint {
    AccountsCheck,
    Subscriptions,
    Privacy,
}

impl ChatgptEndpoint {
    pub fn path(self) -> &'static str {
        match self {
            Self::AccountsCheck => "/backend-api/accounts/check/v4-2023-04-27",
            Self::Subscriptions => "/backend-api/subscriptions",
            Self::Privacy => "/backend-api/settings/account_user_setting",
        }
    }

    pub fn method(self) -> Method {
        if self == Self::Privacy {
            Method::PATCH
        } else {
            Method::GET
        }
    }

    pub fn url(self, account_id: &str, query: Option<&str>) -> Result<reqwest::Url> {
        let mut url = reqwest::Url::parse(&format!("https://chatgpt.com{}", self.path())).unwrap();
        let parsed = reqwest::Url::parse(&format!("https://local/?{}", query.unwrap_or("")))
            .map_err(|_| UpstreamError::InvalidRequest("Invalid query string".into()))?;
        let pairs: Vec<_> = parsed.query_pairs().collect();
        if pairs
            .iter()
            .any(|(key, value)| key == "account_id" && value != account_id)
        {
            return Err(UpstreamError::InvalidRequest(
                "Account does not match the bound OAuth account".into(),
            ));
        }
        match self {
            Self::Subscriptions => {
                if account_id.is_empty() {
                    return Err(UpstreamError::InvalidRequest(
                        "Missing bound ChatGPT account".into(),
                    ));
                }
                url.query_pairs_mut().append_pair("account_id", account_id);
            }
            Self::Privacy => {
                let features: Vec<_> = pairs.iter().filter(|(key, _)| key == "feature").collect();
                let values: Vec<_> = pairs.iter().filter(|(key, _)| key == "value").collect();
                if features.len() != 1
                    || features[0].1 != "training_allowed"
                    || values.len() != 1
                    || values[0].1 != "false"
                {
                    return Err(UpstreamError::InvalidRequest(
                        "Expected feature=training_allowed&value=false".into(),
                    ));
                }
                url.query_pairs_mut()
                    .append_pair("feature", "training_allowed")
                    .append_pair("value", "false");
            }
            Self::AccountsCheck => {}
        }
        Ok(url)
    }

    fn prepare(
        self,
        body: &[u8],
        inbound: &HeaderMap,
        installation_id: &str,
    ) -> Result<PreparedRequest> {
        let mut headers = normalize_protocol_headers(inbound, installation_id)?;
        headers.insert("accept", HeaderValue::from_static("application/json"));
        headers.insert("origin", HeaderValue::from_static("https://chatgpt.com"));
        headers.insert("referer", HeaderValue::from_static("https://chatgpt.com/"));
        let body = if self == Self::Privacy && !body.is_empty() {
            headers.insert("content-type", HeaderValue::from_static("application/json"));
            Bytes::from(serde_json::to_vec(&decode_body(body, inbound)?)?)
        } else {
            Bytes::new()
        };
        Ok(PreparedRequest { body, headers })
    }
}

/// Only inert path segments are accepted, matching the third-party client's path guard.
pub fn responses_subpath_url(subpath: &str) -> Result<reqwest::Url> {
    let segments: Vec<_> = subpath.split('/').collect();
    if segments.len() > 8
        || segments.iter().any(|segment| {
            segment.is_empty()
                || segment.len() > 128
                || segment.bytes().all(|b| b == b'.')
                || !segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        })
        || segments[0] == "input_tokens"
    {
        return Err(UpstreamError::InvalidRequest(
            "Invalid Responses subpath".into(),
        ));
    }
    Ok(reqwest::Url::parse(&format!(
        "{}/responses/{subpath}",
        codex2api_version::CHATGPT_CODEX_BASE_URL
    ))
    .unwrap())
}

impl UpstreamClient {
    pub async fn forward_chatgpt(
        &self,
        endpoint: ChatgptEndpoint,
        account_id: &str,
        query: Option<&str>,
        body: Bytes,
        inbound: HeaderMap,
    ) -> Result<reqwest::Response> {
        let url = endpoint.url(account_id, query)?;
        let installation = self.identity().installation_id.clone();
        let prepared =
            tokio::task::spawn_blocking(move || endpoint.prepare(&body, &inbound, &installation))
                .await
                .map_err(|e| UpstreamError::InvalidRequest(e.to_string()))??;
        self.send_prepared(endpoint.method(), url.as_str(), prepared, false)
            .await
    }

    pub async fn forward_responses_subpath(
        &self,
        subpath: &str,
        body: Bytes,
        inbound: HeaderMap,
    ) -> Result<reqwest::Response> {
        let url = responses_subpath_url(subpath)?;
        let installation = self.identity().installation_id.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            Endpoint::Compact.prepare(&body, &inbound, &installation, None)
        })
        .await
        .map_err(|e| UpstreamError::InvalidRequest(e.to_string()))??;
        self.send_prepared(Method::POST, url.as_str(), prepared, true)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_destinations_preserve_bound_account_and_reject_path_escape() {
        assert_eq!(
            ChatgptEndpoint::AccountsCheck
                .url("account", None)
                .unwrap()
                .as_str(),
            "https://chatgpt.com/backend-api/accounts/check/v4-2023-04-27"
        );
        assert_eq!(
            ChatgptEndpoint::Subscriptions
                .url(
                    "account",
                    Some("account_id=account&url=https://evil.invalid")
                )
                .unwrap()
                .as_str(),
            "https://chatgpt.com/backend-api/subscriptions?account_id=account"
        );
        for query in ["account_id=other", "account_id=account&account_id=other"] {
            assert!(
                ChatgptEndpoint::Subscriptions
                    .url("account", Some(query))
                    .is_err()
            );
        }
        assert_eq!(
            ChatgptEndpoint::Privacy
                .url("account", Some("feature=training_allowed&value=false"))
                .unwrap()
                .as_str(),
            "https://chatgpt.com/backend-api/settings/account_user_setting?feature=training_allowed&value=false"
        );
        for query in [
            "feature=other&value=false",
            "feature=training_allowed&value=true",
            "feature=training_allowed&value=false&value=true",
        ] {
            assert!(
                ChatgptEndpoint::Privacy
                    .url("account", Some(query))
                    .is_err()
            );
        }
        for suffix in ["compact", "response-id/cancel", "v1.2/operation"] {
            assert_eq!(
                responses_subpath_url(suffix).unwrap().as_str(),
                format!("https://chatgpt.com/backend-api/codex/responses/{suffix}")
            );
        }
        for suffix in [
            "",
            "..",
            "...",
            "../models",
            "a/../b",
            "a//b",
            "/compact",
            "a?url=evil",
            "a#fragment",
            "a\\b",
            "%2e%2e/x",
            "input_tokens",
            "input_tokens/extra",
            "a/b/c/d/e/f/g/h/i",
        ] {
            assert!(responses_subpath_url(suffix).is_err(), "{suffix}");
        }
        assert_eq!(
            Endpoint::InputTokens.url(),
            "https://api.openai.com/v1/responses/input_tokens"
        );
        assert_eq!(
            Endpoint::Compact.url(),
            "https://chatgpt.com/backend-api/codex/responses/compact"
        );
        assert_eq!(
            crate::realtime_url(crate::RealtimeKind::CodexSideband, Some("rtc_test"), None)
                .unwrap()
                .as_str(),
            "wss://chatgpt.com/backend-api/codex/rtc_test"
        );
        assert!(
            crate::realtime_url(
                crate::RealtimeKind::CodexSideband,
                Some("../responses"),
                None
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn compatibility_requests_keep_payloads_and_use_account_credentials() {
        use axum::{Router, body::Body, http::Uri, response::Response};
        use codex2api_accounts::{AccountIdentity, HostRuntime};
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let app = Router::new().fallback(
            move |method: Method, uri: Uri, headers: HeaderMap, body: Bytes| {
                let tx = tx.clone();
                async move {
                    tx.send((method, uri, headers, body)).await.unwrap();
                    Response::builder()
                        .header("content-type", "application/json")
                        .body(Body::from(r#"{"input_tokens":17}"#))
                        .unwrap()
                }
            },
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = UpstreamClient::new(
            AccountIdentity::new("account", "installation", HostRuntime::generate()),
            "official-access".into(),
            Some("bound-account".into()),
        )
        .unwrap()
        .with_direct_test_http();
        let mut inbound = HeaderMap::new();
        inbound.insert(
            "authorization",
            HeaderValue::from_static("Bearer proxy-token"),
        );
        inbound.insert(
            "chatgpt-account-id",
            HeaderValue::from_static("caller-account"),
        );
        for endpoint in [
            ChatgptEndpoint::AccountsCheck,
            ChatgptEndpoint::Subscriptions,
            ChatgptEndpoint::Privacy,
        ] {
            let query = if endpoint == ChatgptEndpoint::Privacy {
                Some("feature=training_allowed&value=false")
            } else {
                None
            };
            let official = endpoint.url("bound-account", query).unwrap();
            let url = format!(
                "{origin}{}{}",
                official.path(),
                official
                    .query()
                    .map(|q| format!("?{q}"))
                    .unwrap_or_default()
            );
            let prepared = endpoint.prepare(&[], &inbound, "installation").unwrap();
            let response = client
                .send_prepared(endpoint.method(), &url, prepared, false)
                .await
                .unwrap();
            assert!(response.status().is_success());
            let (method, uri, headers, body) = rx.recv().await.unwrap();
            assert_eq!(method, endpoint.method());
            assert_eq!(uri.path(), official.path());
            assert_eq!(uri.query(), official.query());
            assert_eq!(headers["authorization"], "Bearer official-access");
            assert_eq!(headers["chatgpt-account-id"], "bound-account");
            assert_eq!(headers["origin"], "https://chatgpt.com");
            assert!(body.is_empty());
        }
        for endpoint in [Endpoint::Compact, Endpoint::InputTokens] {
            let body = serde_json::json!({"model":"model-fixture","input":[{"role":"user","content":"keep exactly"}],"instructions":"instructions"});
            let prepared = endpoint
                .prepare(
                    &serde_json::to_vec(&body).unwrap(),
                    &inbound,
                    "installation",
                    None,
                )
                .unwrap();
            let official = reqwest::Url::parse(&endpoint.url()).unwrap();
            let response = client
                .send_prepared(
                    Method::POST,
                    &format!("{origin}{}", official.path()),
                    prepared,
                    endpoint != Endpoint::InputTokens,
                )
                .await
                .unwrap();
            assert_eq!(
                response.json::<serde_json::Value>().await.unwrap()["input_tokens"],
                17
            );
            let (_, uri, headers, received) = rx.recv().await.unwrap();
            assert_eq!(uri.path(), official.path());
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&received).unwrap(),
                body
            );
            assert_eq!(headers["authorization"], "Bearer official-access");
            assert_eq!(headers["chatgpt-account-id"], "bound-account");
        }
        server.abort();
    }
}
