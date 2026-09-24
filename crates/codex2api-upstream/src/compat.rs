//! Fixed ChatGPT endpoints used by OAuth clients.
use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method};

use crate::request::{PreparedRequest, decode_body, normalize_protocol_headers};
use crate::{Endpoint, Result, UpstreamClient, UpstreamError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatgptEndpoint {
    AccountsCheck,
    Subscriptions,
    Privacy,
    FeaturedPlugins,
    Plugins,
    InstalledPlugins,
    SuggestedPlugins,
    AnalyticsEvents,
    StatsigBootstrap,
    Traces,
    Voices,
    ConnectorDirectory,
    SiteStatus,
}

impl ChatgptEndpoint {
    pub fn path(self) -> &'static str {
        match self {
            Self::AccountsCheck => "/backend-api/accounts/check/v4-2023-04-27",
            Self::Subscriptions => "/backend-api/subscriptions",
            Self::Privacy => "/backend-api/settings/account_user_setting",
            Self::FeaturedPlugins => "/backend-api/plugins/featured",
            Self::Plugins => "/backend-api/ps/plugins/list",
            Self::InstalledPlugins => "/backend-api/ps/plugins/installed",
            Self::SuggestedPlugins => "/backend-api/ps/plugins/suggested/codex",
            Self::AnalyticsEvents => "/backend-api/codex/analytics-events/events",
            Self::StatsigBootstrap => "/backend-api/wham/statsig/bootstrap",
            Self::Traces => "/backend-api/o11y/v1/traces",
            Self::Voices => "/backend-api/settings/voices",
            Self::ConnectorDirectory => "/backend-api/connectors/directory/list",
            Self::SiteStatus => "/backend-api/aura/site_status",
        }
    }

    pub fn method(self) -> Method {
        match self {
            Self::Privacy => Method::PATCH,
            Self::AnalyticsEvents | Self::StatsigBootstrap | Self::Traces => Method::POST,
            _ => Method::GET,
        }
    }

    pub fn is_plugin(self) -> bool {
        matches!(
            self,
            Self::FeaturedPlugins | Self::Plugins | Self::InstalledPlugins | Self::SuggestedPlugins
        )
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
                "SupplierAccount does not match the bound OAuth account".into(),
            ));
        }
        match self {
            Self::ConnectorDirectory => {
                for key in ["token", "external_logos"] {
                    let values: Vec<_> = pairs.iter().filter(|(k, _)| k == key).collect();
                    if values.len() > 1
                        || values.first().is_some_and(|(_, v)| {
                            v.len() > 4096
                                || key == "external_logos"
                                    && !matches!(v.as_ref(), "true" | "false")
                        })
                    {
                        return Err(UpstreamError::InvalidRequest(
                            "Invalid connector directory query".into(),
                        ));
                    }
                    if let Some((_, value)) = values.first() {
                        url.query_pairs_mut().append_pair(key, value);
                    }
                }
            }
            Self::SiteStatus => {
                let sites: Vec<_> = pairs.iter().filter(|(k, _)| k == "site_url").collect();
                if sites.len() != 1 || sites[0].1.len() > 8192 {
                    return Err(UpstreamError::InvalidRequest(
                        "Exactly one site_url is required".into(),
                    ));
                }
                let site = reqwest::Url::parse(&sites[0].1)
                    .map_err(|_| UpstreamError::InvalidRequest("Invalid site_url".into()))?;
                if !matches!(site.scheme(), "http" | "https")
                    || site.host_str().is_none()
                    || !site.username().is_empty()
                    || site.password().is_some()
                {
                    return Err(UpstreamError::InvalidRequest(
                        "site_url must be an HTTP(S) URL without credentials".into(),
                    ));
                }
                url.query_pairs_mut().append_pair("site_url", site.as_str());
                if let Some((_, source)) = pairs.iter().find(|(k, _)| k == "url_request_source") {
                    if source.len() > 128 {
                        return Err(UpstreamError::InvalidRequest(
                            "Invalid URL request source".into(),
                        ));
                    }
                    url.query_pairs_mut()
                        .append_pair("url_request_source", source);
                }
                // Local conversation and turn IDs are not supplier-owned identifiers.
            }
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
            Self::AccountsCheck | Self::AnalyticsEvents | Self::StatsigBootstrap | Self::Traces => {
            }
            Self::Voices => {
                for (key, value) in &pairs {
                    if matches!(key.as_ref(), "spoken_language" | "voice_mode") {
                        url.query_pairs_mut().append_pair(key, value);
                    }
                }
            }
            Self::FeaturedPlugins
            | Self::Plugins
            | Self::InstalledPlugins
            | Self::SuggestedPlugins => {
                // Pinned core-plugins/src/remote.rs and remote_legacy.rs query contracts.
                let allowed: &[&str] = match self {
                    Self::FeaturedPlugins => &["platform"],
                    Self::Plugins => &["scope", "limit", "collection", "pageToken"],
                    Self::InstalledPlugins => {
                        &["scope", "limit", "includeDownloadUrls", "pageToken"]
                    }
                    Self::SuggestedPlugins => &["scope"],
                    _ => unreachable!(),
                };
                for (key, value) in &pairs {
                    if allowed.contains(&key.as_ref()) {
                        url.query_pairs_mut().append_pair(key, value);
                    }
                }
            }
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
        if !self.is_plugin()
            && !matches!(
                self,
                Self::AnalyticsEvents | Self::Traces | Self::ConnectorDirectory | Self::SiteStatus
            )
        {
            headers.insert("origin", HeaderValue::from_static("https://chatgpt.com"));
            headers.insert("referer", HeaderValue::from_static("https://chatgpt.com/"));
        } else {
            headers.insert(
                "originator",
                HeaderValue::from_static(codex2api_version::DEFAULT_ORIGINATOR),
            );
            if self.is_plugin() && self != Self::FeaturedPlugins {
                // core-plugins/src/remote.rs::authenticated_request in the pinned snapshot.
                headers.insert("oai-product-sku", HeaderValue::from_static("codex"));
            }
        }
        let body = if matches!(
            self,
            Self::Privacy | Self::AnalyticsEvents | Self::StatsigBootstrap | Self::Traces
        ) && !body.is_empty()
        {
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

fn raw_chatgpt_url(method: &Method, path: &str, query: Option<&str>) -> Result<reqwest::Url> {
    let plugin = path.strip_prefix("/backend-api/ps/plugins/").filter(|id| {
        !id.is_empty()
            && id.len() <= 256
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    });
    let conversation = path
        .strip_prefix("/backend-api/conversation/")
        .is_some_and(|id| {
            !id.is_empty()
                && id.len() <= 256
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        });
    if !(path == "/backend-api/ps/mcp" && matches!(*method, Method::GET | Method::POST)
        || path == "/backend-api/ps/apps/batch" && *method == Method::POST
        || plugin.is_some() && *method == Method::GET
        || conversation && matches!(*method, Method::GET | Method::PATCH))
    {
        return Err(UpstreamError::InvalidRequest(
            "Unsupported ChatGPT endpoint.".into(),
        ));
    }
    let mut url =
        reqwest::Url::parse(&format!("https://chatgpt.com{path}")).expect("validated path");
    let mut query_url = reqwest::Url::parse("https://local/").unwrap();
    query_url.set_query(query);
    if plugin.is_some() {
        for (key, value) in query_url.query_pairs() {
            if key == "includeDownloadUrls" {
                url.query_pairs_mut().append_pair(&key, &value);
            }
        }
    }
    Ok(url)
}

fn prepare_raw_chatgpt(
    method: &Method,
    mcp: bool,
    body: Bytes,
    inbound: HeaderMap,
    installation: &str,
) -> Result<PreparedRequest> {
    let mut inbound = inbound;
    crate::strip_hop_by_hop_headers(&mut inbound);
    let mut headers = normalize_protocol_headers(&inbound, installation)?;
    headers.insert(
        "originator",
        HeaderValue::from_static(codex2api_version::DEFAULT_ORIGINATOR),
    );
    headers.insert("oai-product-sku", HeaderValue::from_static("codex"));
    headers.insert(
        "accept",
        HeaderValue::from_static(if mcp {
            "application/json, text/event-stream"
        } else {
            "application/json"
        }),
    );
    if mcp {
        for name in [
            "accept",
            "mcp-session-id",
            "mcp-protocol-version",
            "last-event-id",
        ] {
            if let Some(value) = inbound.get(name) {
                headers.insert(name, value.clone());
            }
        }
    }
    if matches!(*method, Method::POST | Method::PATCH) {
        headers.insert(
            "content-type",
            inbound
                .get("content-type")
                .cloned()
                .unwrap_or_else(|| HeaderValue::from_static("application/json")),
        );
        if let Some(encoding) = inbound.get("content-encoding") {
            headers.insert("content-encoding", encoding.clone());
        }
    }
    Ok(PreparedRequest { headers, body })
}

impl UpstreamClient {
    /// Native ChatGPT conversation calls observed in the desktop renderer. This
    /// is deliberately separate from Codex Responses and arbitrary URL forwarding.
    pub async fn forward_chatgpt_conversation(
        &self,
        path: &str,
        body: Bytes,
        inbound: HeaderMap,
    ) -> Result<reqwest::Response> {
        if !matches!(
            path,
            "/backend-api/f/conversation/prepare"
                | "/backend-api/f/conversation"
                | "/backend-api/f/conversation/resume"
                | "/backend-api/stop_conversation"
        ) {
            return Err(UpstreamError::InvalidRequest(
                "Unsupported conversation operation".into(),
            ));
        }
        let mut headers = normalize_protocol_headers(&inbound, &self.identity().installation_id)?;
        headers.insert(
            "originator",
            HeaderValue::from_static(codex2api_version::DEFAULT_ORIGINATOR),
        );
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert(
            "accept",
            HeaderValue::from_static("application/json, text/event-stream"),
        );
        headers.insert("origin", HeaderValue::from_static("https://chatgpt.com"));
        headers.insert("referer", HeaderValue::from_static("https://chatgpt.com/"));
        for name in [
            "x-conduit-token",
            "x-oai-turn-trace-id",
            "openai-sentinel-chat-requirements-token",
            "openai-sentinel-proof-token",
            "openai-sentinel-token",
            "openai-sentinel-turnstile-token",
        ] {
            if let Some(v) = inbound.get(name) {
                headers.insert(name, v.clone());
            }
        }
        let value = decode_body(&body, &inbound)?;
        self.send_prepared(
            Method::POST,
            &format!("https://chatgpt.com{path}"),
            PreparedRequest {
                headers,
                body: Bytes::from(serde_json::to_vec(&value)?),
            },
            false,
        )
        .await
    }
    pub async fn forward_raw_chatgpt(
        &self,
        method: Method,
        path: &str,
        query: Option<&str>,
        body: Bytes,
        inbound: HeaderMap,
    ) -> Result<reqwest::Response> {
        let url = raw_chatgpt_url(&method, path, query)?;
        let mcp = path == "/backend-api/ps/mcp";
        let installation = self.identity().installation_id.clone();
        let prepare_method = method.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            prepare_raw_chatgpt(&prepare_method, mcp, body, inbound, &installation)
        })
        .await
        .map_err(|e| UpstreamError::InvalidRequest(e.to_string()))??;
        self.send_prepared(method, url.as_str(), prepared, false)
            .await
    }
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
    fn desktop_service_queries_keep_pagination_and_never_change_upstream_destination() {
        for endpoint in [
            ChatgptEndpoint::ConnectorDirectory,
            ChatgptEndpoint::SiteStatus,
        ] {
            let headers = endpoint
                .prepare(&[], &HeaderMap::new(), "installation")
                .unwrap()
                .headers;
            assert_eq!(headers["originator"], codex2api_version::DEFAULT_ORIGINATOR);
        }
        let url = ChatgptEndpoint::ConnectorDirectory
            .url(
                "supplier",
                Some("token=page+2%2F%2B&external_logos=true&access_token=secret"),
            )
            .unwrap();
        assert_eq!(
            url.as_str(),
            "https://chatgpt.com/backend-api/connectors/directory/list?token=page+2%2F%2B&external_logos=true"
        );
        assert!(
            ChatgptEndpoint::ConnectorDirectory
                .url("supplier", Some("token=a&token=b"))
                .is_err()
        );
        let mut request = reqwest::Url::parse("https://local/").unwrap();
        request.query_pairs_mut().extend_pairs([
            ("site_url", "https://example.test/path?q=value"),
            ("conversation_id", "virtual-thread"),
            ("turn_id", "virtual-turn"),
            ("url_request_source", "codex_browser_use"),
        ]);
        let url = ChatgptEndpoint::SiteStatus
            .url("supplier", request.query())
            .unwrap();
        assert_eq!(url.host_str(), Some("chatgpt.com"));
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(pairs["site_url"], "https://example.test/path?q=value");
        assert!(!pairs.contains_key("conversation_id"));
        for query in [
            "",
            "site_url=file:///etc/passwd",
            "site_url=https://user:password@example.test",
            "site_url=https://a.test&site_url=https://b.test",
        ] {
            assert!(
                ChatgptEndpoint::SiteStatus
                    .url("supplier", Some(query))
                    .is_err()
            );
        }
    }

    #[tokio::test]
    async fn mcp_and_apps_preserve_wire_payloads_session_headers_and_response_stream() {
        use axum::{Router, body::Body, response::Response};
        use codex2api_accounts::{AccountIdentity, HostRuntime};
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let app = Router::new().fallback(move |method: Method, headers: HeaderMap, body: Bytes| {
            let tx = tx.clone();
            async move {
                tx.send((method, headers, body)).await.unwrap();
                Response::builder()
                    .header("content-type", "text/event-stream")
                    .header("mcp-session-id", "upstream-session")
                    .body(Body::from(
                        "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n",
                    ))
                    .unwrap()
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = format!("http://{}/test", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = UpstreamClient::new(
            AccountIdentity::new("fixture", "installation", HostRuntime::generate()),
            "upstream-token".into(),
            Some("upstream-account".into()),
        )
        .unwrap()
        .with_direct_test_http();
        for (method, mcp, payload) in [
            (Method::GET, true, ""),
            (Method::POST, true, ""),
            (
                Method::POST,
                true,
                "{ \"jsonrpc\": \"2.0\", \"method\": \"initialize\", \"id\": 1 }",
            ),
            (
                Method::POST,
                false,
                "{\"app_ids\":[\"fixture-app\"],\"include_tools\":true}",
            ),
        ] {
            let mut headers = HeaderMap::new();
            for (name, value) in [
                ("accept", "application/json, text/event-stream"),
                ("mcp-session-id", "session-fixture"),
                ("mcp-protocol-version", "2025-06-18"),
                ("last-event-id", "event-fixture"),
                ("authorization", "Bearer client-token"),
                ("chatgpt-account-id", "virtual-account"),
                ("cookie", "private=client"),
                ("content-type", "application/json"),
            ] {
                headers.insert(name, value.parse().unwrap());
            }
            let prepared = prepare_raw_chatgpt(
                &method,
                mcp,
                Bytes::copy_from_slice(payload.as_bytes()),
                headers,
                "installation",
            )
            .unwrap();
            let response = client
                .send_prepared(method.clone(), &target, prepared, false)
                .await
                .unwrap();
            assert_eq!(response.headers()["mcp-session-id"], "upstream-session");
            assert_eq!(
                response.text().await.unwrap(),
                "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n"
            );
            let (received, headers, body) = rx.recv().await.unwrap();
            assert_eq!(received, method);
            assert_eq!(body.as_ref(), payload.as_bytes());
            assert_eq!(headers["authorization"], "Bearer upstream-token");
            assert_eq!(headers["chatgpt-account-id"], "upstream-account");
            assert!(!headers.contains_key("cookie"));
            if mcp {
                assert_eq!(headers["accept"], "application/json, text/event-stream");
                assert_eq!(headers["mcp-session-id"], "session-fixture");
                assert_eq!(headers["last-event-id"], "event-fixture");
                assert_eq!(headers["mcp-protocol-version"], "2025-06-18");
            } else {
                assert_eq!(headers["accept"], "application/json");
                assert!(!headers.contains_key("mcp-session-id"));
            }
        }
        server.abort();
    }

    #[test]
    fn raw_destinations_and_connection_scoped_headers_are_restricted() {
        assert!(raw_chatgpt_url(&Method::POST, "/backend-api/ps/apps/batch", None).is_ok());
        for path in [
            "//evil.test",
            "/backend-api/ps/plugins/../mcp",
            "/backend-api/ps/plugins/a%2Fb",
            "/backend-api/ps/plugins/a?x=1",
        ] {
            assert!(raw_chatgpt_url(&Method::GET, path, None).is_err());
        }
        assert!(raw_chatgpt_url(&Method::GET, "/backend-api/ps/apps/batch", None).is_err());
        let url = raw_chatgpt_url(
            &Method::GET,
            "/backend-api/ps/plugins/plugin_fixture",
            Some("includeDownloadUrls=true&token=drop"),
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "https://chatgpt.com/backend-api/ps/plugins/plugin_fixture?includeDownloadUrls=true"
        );
        let mut headers = HeaderMap::new();
        headers.insert("connection", "mcp-session-id".parse().unwrap());
        headers.insert("mcp-session-id", "hop-local".parse().unwrap());
        let prepared =
            prepare_raw_chatgpt(&Method::POST, true, Bytes::new(), headers, "fixture").unwrap();
        assert!(!prepared.headers.contains_key("mcp-session-id"));
    }

    #[test]
    fn plugin_urls_keep_official_query_parameters_and_fixed_destinations() {
        for (endpoint, path, query, expected) in [
            (
                ChatgptEndpoint::FeaturedPlugins,
                "/backend-api/plugins/featured",
                "platform=codex&access_token=drop",
                "platform=codex",
            ),
            (
                ChatgptEndpoint::Plugins,
                "/backend-api/ps/plugins/list",
                "scope=GLOBAL&limit=200&collection=vertical+%26+special&pageToken=next+page%2F%2B&url=https%3A%2F%2Fevil.test",
                "scope=GLOBAL&limit=200&collection=vertical+%26+special&pageToken=next+page%2F%2B",
            ),
            (
                ChatgptEndpoint::InstalledPlugins,
                "/backend-api/ps/plugins/installed",
                "scope=USER&limit=200&includeDownloadUrls=true&pageToken=page-2",
                "scope=USER&limit=200&includeDownloadUrls=true&pageToken=page-2",
            ),
            (
                ChatgptEndpoint::SuggestedPlugins,
                "/backend-api/ps/plugins/suggested/codex",
                "scope=GLOBAL",
                "scope=GLOBAL",
            ),
        ] {
            let url = endpoint.url("bound-account", Some(query)).unwrap();
            assert_eq!(url.host_str(), Some("chatgpt.com"));
            assert_eq!(url.scheme(), "https");
            assert_eq!(url.path(), path);
            assert_eq!(url.query(), Some(expected));
            assert_eq!(endpoint.method(), Method::GET);
            assert!(
                endpoint
                    .url("bound-account", Some("account_id=another-account"))
                    .is_err()
            );
        }
    }

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
            ChatgptEndpoint::FeaturedPlugins,
            ChatgptEndpoint::Plugins,
            ChatgptEndpoint::InstalledPlugins,
            ChatgptEndpoint::SuggestedPlugins,
            ChatgptEndpoint::AnalyticsEvents,
            ChatgptEndpoint::Traces,
            ChatgptEndpoint::Voices,
        ] {
            let query = match endpoint {
                ChatgptEndpoint::Privacy => Some("feature=training_allowed&value=false"),
                ChatgptEndpoint::FeaturedPlugins => Some("platform=codex"),
                ChatgptEndpoint::Plugins => {
                    Some("scope=GLOBAL&limit=200&collection=vertical&pageToken=next%2Bpage")
                }
                ChatgptEndpoint::InstalledPlugins => {
                    Some("limit=200&includeDownloadUrls=true&pageToken=next%2Fpage")
                }
                ChatgptEndpoint::SuggestedPlugins => Some("scope=GLOBAL"),
                ChatgptEndpoint::Voices => {
                    Some("spoken_language=en-US&voice_mode=advanced&private=discard")
                }
                _ => None,
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
            let payload = match endpoint {
                ChatgptEndpoint::AnalyticsEvents => {
                    br#"{"events":[{"event_name":"fixture","properties":{"count":1}}]}"#.as_slice()
                }
                ChatgptEndpoint::Traces => {
                    br#"{"resourceSpans":[{"scopeSpans":[{"spans":[{"name":"fixture-span"}]}]}]}"#
                        .as_slice()
                }
                _ => &[],
            };
            let prepared = endpoint.prepare(payload, &inbound, "installation").unwrap();
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
            if endpoint.is_plugin() {
                assert!(!headers.contains_key("origin"));
                assert!(!headers.contains_key("referer"));
                assert_eq!(
                    headers["user-agent"],
                    client.identity().official_user_agent()
                );
                assert_eq!(headers["originator"], codex2api_version::DEFAULT_ORIGINATOR);
                if endpoint != ChatgptEndpoint::FeaturedPlugins {
                    assert_eq!(headers["oai-product-sku"], "codex");
                }
            } else if matches!(
                endpoint,
                ChatgptEndpoint::AnalyticsEvents | ChatgptEndpoint::Traces
            ) {
                assert!(!headers.contains_key("origin"));
                assert!(!headers.contains_key("oai-product-sku"));
                assert_eq!(headers["content-type"], "application/json");
                assert_eq!(uri.path(), endpoint.path());
            } else {
                assert_eq!(headers["origin"], "https://chatgpt.com");
            }
            assert_eq!(body.as_ref(), payload);
            if endpoint == ChatgptEndpoint::Voices {
                assert_eq!(
                    uri.query(),
                    Some("spoken_language=en-US&voice_mode=advanced")
                );
            }
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
