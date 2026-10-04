use std::collections::HashMap;

use crate::request::{PreparedRequest, decode_body, normalize_protocol_headers};
use crate::{Result, UpstreamClient, UpstreamError};
use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method};

/// WHAM operations used by the pinned official backend client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendEndpoint {
    Usage,
    Accounts,
    Profile,
    Config,
    Settings,
    Credits,
    ConsumeCredit,
    Tasks,
    Task,
    SiblingTurns,
    TaskTurns,
    TaskTurn,
    TaskLogs,
    CancelTask,
    ArchiveTask,
    CreateTask,
    Messages,
    ThreadUsage,
    TurnEstimates,
}

impl BackendEndpoint {
    pub const ALL: [Self; 19] = [
        Self::Usage,
        Self::Accounts,
        Self::Profile,
        Self::Config,
        Self::Settings,
        Self::Credits,
        Self::ConsumeCredit,
        Self::Tasks,
        Self::Task,
        Self::SiblingTurns,
        Self::TaskTurns,
        Self::TaskTurn,
        Self::TaskLogs,
        Self::CancelTask,
        Self::ArchiveTask,
        Self::CreateTask,
        Self::Messages,
        Self::ThreadUsage,
        Self::TurnEstimates,
    ];

    pub fn path(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::Accounts => "accounts/check",
            Self::Profile => "profiles/me",
            Self::Config => "config/bundle",
            Self::Settings => "settings/user",
            Self::Credits => "rate-limit-reset-credits",
            Self::ConsumeCredit => "rate-limit-reset-credits/consume",
            Self::Tasks => "tasks/list",
            Self::Task => "tasks/{task_id}",
            Self::SiblingTurns => "tasks/{task_id}/turns/{turn_id}/sibling_turns",
            Self::CreateTask => "tasks",
            Self::TaskTurns => "tasks/{task_id}/turns",
            Self::TaskTurn => "tasks/{task_id}/turns/{turn_id}",
            Self::TaskLogs => "tasks/{task_id}/turns/{turn_id}/logs",
            Self::CancelTask => "tasks/{task_id}/cancel",
            Self::ArchiveTask => "tasks/{task_id}/archive",
            Self::Messages => "workspace-messages",
            Self::ThreadUsage => "usage/thread_usage/query",
            Self::TurnEstimates => "usage/thread-estimates/query",
        }
    }

    pub fn method(self) -> Method {
        match self {
            Self::ConsumeCredit
            | Self::CreateTask
            | Self::ThreadUsage
            | Self::TurnEstimates
            | Self::CancelTask
            | Self::ArchiveTask => Method::POST,
            _ => Method::GET,
        }
    }

    pub fn url(
        self,
        parameters: &HashMap<String, String>,
        query: Option<&str>,
    ) -> Result<reqwest::Url> {
        let mut url = reqwest::Url::parse("https://chatgpt.com/backend-api/wham/").unwrap();
        {
            let mut segments = url.path_segments_mut().unwrap();
            segments.pop_if_empty();
            for segment in self.path().split('/') {
                let value = if let Some(key) =
                    segment.strip_prefix('{').and_then(|s| s.strip_suffix('}'))
                {
                    parameters
                        .get(key)
                        .map(String::as_str)
                        .ok_or_else(|| UpstreamError::InvalidRequest(format!("Missing {key}")))?
                } else {
                    segment
                };
                validate_segment(value)?;
                segments.push(value);
            }
        }
        if self == Self::Tasks
            && let Some(query) = query
        {
            let parsed = reqwest::Url::parse(&format!("https://local/?{query}"))
                .map_err(|_| UpstreamError::InvalidRequest("Invalid task query".into()))?;
            for (key, value) in parsed.query_pairs() {
                if matches!(
                    key.as_ref(),
                    "limit" | "task_filter" | "environment_id" | "cursor"
                ) {
                    url.query_pairs_mut().append_pair(&key, &value);
                }
            }
        }
        Ok(url)
    }

    pub(crate) fn prepare(
        self,
        body: &[u8],
        inbound: &HeaderMap,
        installation_id: &str,
    ) -> Result<PreparedRequest> {
        let mut headers = normalize_protocol_headers(inbound, installation_id)?;
        if self == Self::Settings {
            headers.insert(
                "cache-control",
                HeaderValue::from_static("no-cache, no-store"),
            );
        }
        if self == Self::Messages {
            headers.insert("cache-control", HeaderValue::from_static("no-store"));
        }
        if self == Self::Usage
            && inbound
                .get("x-openai-codex-luna-reserve")
                .is_some_and(|v| v == "1")
        {
            headers.insert("x-openai-codex-luna-reserve", HeaderValue::from_static("1"));
        }
        let body = if self.method() == Method::POST {
            let value = if body.is_empty() && matches!(self, Self::CancelTask | Self::ArchiveTask) {
                serde_json::json!({})
            } else {
                decode_body(body, inbound)?
            };
            if self == Self::ConsumeCredit
                && value
                    .get("redeem_request_id")
                    .and_then(serde_json::Value::as_str)
                    .is_none_or(|id| id.trim().is_empty())
            {
                return Err(UpstreamError::InvalidRequest(
                    "redeem_request_id is required".into(),
                ));
            }
            headers.insert("content-type", HeaderValue::from_static("application/json"));
            Bytes::from(serde_json::to_vec(&value)?)
        } else {
            Bytes::new()
        };
        Ok(PreparedRequest { body, headers })
    }
}

pub(crate) fn validate_segment(value: &str) -> Result<()> {
    if value.is_empty()
        || matches!(value, "." | "..")
        || value.contains(['/', '\\', '%', '?', '#'])
        || value.chars().any(char::is_control)
    {
        return Err(UpstreamError::InvalidRequest(
            "Invalid path identifier".into(),
        ));
    }
    Ok(())
}

impl UpstreamClient {
    pub async fn forward_backend(
        &self,
        endpoint: BackendEndpoint,
        parameters: &HashMap<String, String>,
        query: Option<&str>,
        body: Bytes,
        inbound: HeaderMap,
    ) -> Result<reqwest::Response> {
        let url = endpoint.url(parameters, query)?;
        let installation_id = self.identity().installation_id.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            endpoint.prepare(&body, &inbound, &installation_id)
        })
        .await
        .map_err(|e| UpstreamError::InvalidRequest(e.to_string()))??;
        self.send_prepared(endpoint.method(), url.as_str(), prepared, false)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn task_identifiers_and_queries_cannot_change_the_destination() {
        let params = HashMap::from([
            ("task_id".into(), "task-1".into()),
            ("turn_id".into(), "turn-2".into()),
        ]);
        assert_eq!(
            BackendEndpoint::SiblingTurns
                .url(&params, None)
                .unwrap()
                .as_str(),
            "https://chatgpt.com/backend-api/wham/tasks/task-1/turns/turn-2/sibling_turns"
        );
        let url = BackendEndpoint::Tasks
            .url(
                &HashMap::new(),
                Some("limit=10&cursor=a%2Bb&host=evil&access_token=secret"),
            )
            .unwrap();
        assert_eq!(url.query(), Some("limit=10&cursor=a%2Bb"));
        for id in ["..", "../account", "%2e%2e", "a?query=x"] {
            assert!(
                BackendEndpoint::Task
                    .url(&HashMap::from([("task_id".into(), id.into())]), None)
                    .is_err()
            );
        }
    }
    #[test]
    fn mutations_preserve_idempotency_and_payloads() {
        let raw = br#"{"redeem_request_id":"same-on-retry","credit_id":"credit-1"}"#;
        let p = BackendEndpoint::ConsumeCredit
            .prepare(raw, &HeaderMap::new(), "installation")
            .unwrap();
        assert_eq!(p.body, Bytes::from_static(raw));
        assert!(
            BackendEndpoint::ConsumeCredit
                .prepare(b"{}", &HeaderMap::new(), "installation")
                .is_err()
        );
        assert_eq!(
            BackendEndpoint::Settings
                .prepare(b"", &HeaderMap::new(), "installation")
                .unwrap()
                .headers["cache-control"],
            "no-cache, no-store"
        );
    }

    #[tokio::test]
    async fn backend_http_calls_use_account_auth_and_preserve_bodies() {
        use axum::{Router, body::Body, http::Uri, response::Response};
        use codex2api_accounts::{AccountIdentity, HostRuntime};
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let app = Router::new().fallback(
            move |method: Method, uri: Uri, headers: HeaderMap, body: Bytes| {
                let tx = tx.clone();
                async move {
                    tx.send((method, uri, headers, body)).await.unwrap();
                    Response::builder()
                        .header("content-type", "application/json")
                        .header("connection", "close")
                        .body(Body::from("{\"official\":true}"))
                        .unwrap()
                }
            },
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = UpstreamClient::new(
            AccountIdentity::new("a", "install", HostRuntime::generate()),
            "account-token".into(),
            Some("account-a".into()),
        )
        .unwrap()
        .with_direct_test_http();
        let params = HashMap::from([
            ("task_id".into(), "task-1".into()),
            ("turn_id".into(), "turn-1".into()),
        ]);
        for endpoint in BackendEndpoint::ALL {
            let body = serde_json::json!({"redeem_request_id":"stable-id", "thread_ids":["thread-1"], "new_task":{"environment_id":"env"}, "input_items":[]});
            let mut inbound = HeaderMap::new();
            inbound.insert(
                "authorization",
                HeaderValue::from_static("Bearer proxy-key"),
            );
            inbound.insert("x-forwarded-for", HeaderValue::from_static("private"));
            inbound.insert(
                "x-oai-attestation",
                HeaderValue::from_static("attestation-original"),
            );
            inbound.insert("traceparent", HeaderValue::from_static("trace-original"));
            inbound.insert("x-openai-fedramp", HeaderValue::from_static("true"));
            inbound.insert(
                "x-codex-turn-metadata",
                HeaderValue::from_static(r#"{"installation_id":"caller","turn_id":"turn"}"#),
            );
            let prepared = endpoint
                .prepare(&serde_json::to_vec(&body).unwrap(), &inbound, "install")
                .unwrap();
            let url = endpoint.url(&params, None).unwrap();
            let response = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                client.send_prepared(
                    endpoint.method(),
                    &format!("{base}{}", url.path()),
                    prepared,
                    false,
                ),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(
                response.json::<serde_json::Value>().await.unwrap()["official"],
                true
            );
            let (method, uri, headers, received) = rx.recv().await.unwrap();
            assert_eq!(method, endpoint.method());
            assert_eq!(uri.path(), url.path());
            assert_eq!(headers["authorization"], "Bearer account-token");
            assert_eq!(headers["chatgpt-account-id"], "account-a");
            for name in ["x-oai-attestation", "traceparent", "x-openai-fedramp"] {
                assert_eq!(headers[name], inbound[name], "{endpoint:?}: {name}");
            }
            let turn: serde_json::Value =
                serde_json::from_str(headers["x-codex-turn-metadata"].to_str().unwrap()).unwrap();
            assert_eq!(
                turn,
                serde_json::json!({"installation_id":"install","turn_id":"turn"})
            );
            for key in ["originator", "version", "x-forwarded-for", "cookie"] {
                assert!(!headers.contains_key(key), "{key}");
            }
            if endpoint.method() == Method::POST {
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&received).unwrap(),
                    body
                );
            } else {
                assert!(received.is_empty());
            }
        }
        server.abort();
    }
}
