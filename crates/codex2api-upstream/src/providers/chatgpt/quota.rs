//! Supplier observations from the pinned codex-api/src/rate_limits.rs contract.
//! These update administrator snapshots before consumer quota rewriting.
use chrono::{DateTime, Utc};
use codex2api_storage::{QuotaSnapshot, Storage};
use futures::StreamExt;
use http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone)]
pub(super) struct QuotaObserver {
    storage: Storage,
    account_id: String,
    revision: i64,
}

impl super::client::UpstreamClient {
    pub(super) fn quota_observer(&self, revision: i64) -> Option<QuotaObserver> {
        Some(QuotaObserver {
            storage: self.auth_service()?.storage().ok()?.clone(),
            account_id: self.identity().account_id.clone(),
            revision,
        })
    }
}

impl QuotaObserver {
    async fn store(&self, updates: Vec<Update>, observed_at: DateTime<Utc>) {
        if updates.is_empty() {
            return;
        }
        if let Err(error) = self
            .storage
            .update_account_quota(&self.account_id, self.revision, observed_at, |previous| {
                merge(previous, updates)
            })
            .await
        {
            tracing::warn!(account_id=%self.account_id, %error, "failed to persist response quota");
        }
    }

    pub(super) async fn headers(&self, headers: &HeaderMap) {
        self.store(header_updates(headers), Utc::now()).await;
    }

    pub(super) async fn event(&self, text: &str) {
        if let Some(update) = event_update(text) {
            self.store(vec![update], Utc::now()).await;
        }
    }

    pub(super) async fn response(&self, mut response: reqwest::Response) -> reqwest::Response {
        self.headers(response.headers()).await;
        if !response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/event-stream"))
        {
            return response;
        }
        let status = response.status();
        let version = response.version();
        let headers = response.headers().clone();
        let extensions = std::mem::take(response.extensions_mut());
        let observer = self.clone();
        let stream = async_stream::stream! {
            let mut source = response.bytes_stream();
            let mut parser = SseQuotaParser::default();
            while let Some(chunk) = source.next().await {
                if let Ok(bytes) = &chunk {
                    let observed_at = Utc::now();
                    let updates = parser.push(bytes);
                    observer.store(updates, observed_at).await;
                }
                // Preserve exact bytes, errors and streaming backpressure.
                yield chunk;
            }
        };
        let mut rebuilt = http::Response::new(reqwest::Body::wrap_stream(stream));
        *rebuilt.status_mut() = status;
        *rebuilt.version_mut() = version;
        *rebuilt.headers_mut() = headers;
        *rebuilt.extensions_mut() = extensions;
        rebuilt.into()
    }
}

#[derive(Default)]
struct Update {
    limit_id: String,
    limit_name: Option<String>,
    primary: Option<Value>,
    secondary: Option<Value>,
    credits: Option<Value>,
    plan_type: Option<String>,
}

impl Update {
    fn has_data(&self) -> bool {
        self.primary.is_some() || self.secondary.is_some() || self.credits.is_some()
    }
}

fn normalize(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace('-', "_")
}

fn window(used: f64, minutes: Option<i64>, reset: Option<i64>) -> Option<Value> {
    if !used.is_finite() || used < 0.0 {
        return None;
    }
    Some(json!({
        "used_percent": used,
        "limit_window_seconds": minutes.filter(|m| *m > 0).and_then(|m| m.checked_mul(60)),
        "reset_at": reset.filter(|r| *r > 0),
    }))
}

fn header_updates(headers: &HeaderMap) -> Vec<Update> {
    let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let boolean = |name: &str| match get(name)? {
        s if s.eq_ignore_ascii_case("true") || s == "1" => Some(true),
        s if s.eq_ignore_ascii_case("false") || s == "0" => Some(false),
        _ => None,
    };
    let credits = boolean("x-codex-credits-has-credits")
        .zip(boolean("x-codex-credits-unlimited"))
        .map(|(has_credits, unlimited)| {
            json!({
                "has_credits": has_credits, "unlimited": unlimited,
                "balance": get("x-codex-credits-balance").map(str::trim).filter(|v| !v.is_empty()),
            })
        });
    let mut ids = BTreeSet::from(["codex".to_string()]);
    for name in headers.keys() {
        if let Some(id) = name
            .as_str()
            .strip_prefix("x-")
            .and_then(|s| s.strip_suffix("-primary-used-percent"))
        {
            ids.insert(normalize(id));
        }
    }
    ids.into_iter()
        .filter_map(|limit_id| {
            let prefix = format!("x-{}", limit_id.replace('_', "-"));
            let read_window = |name: &str| {
                let used: f64 = get(&format!("{prefix}-{name}-used-percent"))?
                    .parse()
                    .ok()?;
                let minutes =
                    get(&format!("{prefix}-{name}-window-minutes")).and_then(|v| v.parse().ok());
                let reset = get(&format!("{prefix}-{name}-reset-at")).and_then(|v| v.parse().ok());
                if used == 0.0 && minutes.unwrap_or(0) == 0 && reset.is_none() {
                    return None;
                }
                window(used, minutes, reset)
            };
            let update = Update {
                primary: read_window("primary"),
                secondary: read_window("secondary"),
                limit_name: get(&format!("{prefix}-limit-name"))
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
                credits: (limit_id == "codex").then(|| credits.clone()).flatten(),
                limit_id,
                ..Default::default()
            };
            update.has_data().then_some(update)
        })
        .collect()
}

#[derive(Deserialize)]
struct EventWindow {
    used_percent: f64,
    window_minutes: Option<i64>,
    reset_at: Option<i64>,
}
#[derive(Deserialize)]
struct EventWindows {
    primary: Option<EventWindow>,
    secondary: Option<EventWindow>,
}
#[derive(Deserialize, serde::Serialize)]
struct EventCredits {
    has_credits: bool,
    unlimited: bool,
    balance: Option<String>,
}
#[derive(Deserialize)]
struct RateLimitEvent {
    #[serde(rename = "type")]
    kind: String,
    rate_limits: Option<EventWindows>,
    credits: Option<EventCredits>,
    plan_type: Option<String>,
    metered_limit_name: Option<String>,
    limit_name: Option<String>,
}

fn event_update(text: &str) -> Option<Update> {
    if !text.contains("codex.rate_limits") {
        return None;
    }
    let event: RateLimitEvent = serde_json::from_str(text).ok()?;
    if event.kind != "codex.rate_limits" {
        return None;
    }
    let (primary, secondary) = event
        .rate_limits
        .map(|w| (w.primary, w.secondary))
        .unwrap_or_default();
    let convert = |w: EventWindow| window(w.used_percent, w.window_minutes, w.reset_at);
    let update = Update {
        limit_id: event
            .metered_limit_name
            .or(event.limit_name)
            .map(|s| normalize(&s))
            .unwrap_or_else(|| "codex".into()),
        primary: primary.and_then(convert),
        secondary: secondary.and_then(convert),
        credits: event.credits.map(|c| json!(c)),
        plan_type: event.plan_type,
        ..Default::default()
    };
    update.has_data().then_some(update)
}

fn merge(previous: Option<QuotaSnapshot>, updates: Vec<Update>) -> Value {
    let mut value = previous
        .as_ref()
        .map(|s| s.value.clone())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    // Cached relative resets refer to the original observation, not this update.
    fn anchor(rate: &mut Value, at: i64) {
        for name in ["primary_window", "secondary_window"] {
            if let Some(w) = rate.get_mut(name).and_then(Value::as_object_mut) {
                if w.get("reset_at").and_then(Value::as_i64).is_none()
                    && let Some(seconds) = w
                        .get("reset_after_seconds")
                        .and_then(Value::as_i64)
                        .filter(|s| *s >= 0)
                {
                    w.insert("reset_at".into(), json!(at.checked_add(seconds)));
                }
                w.remove("reset_after_seconds");
            }
        }
    }
    if let Some(previous) = previous {
        if let Some(rate) = value.get_mut("rate_limit") {
            anchor(rate, previous.observed_at.timestamp());
        }
        if let Some(additional) = value
            .get_mut("additional_rate_limits")
            .and_then(Value::as_array_mut)
        {
            for limit in additional {
                if let Some(rate) = limit.get_mut("rate_limit") {
                    anchor(rate, previous.observed_at.timestamp());
                }
            }
        }
    }
    for update in updates {
        if let Some(credits) = update.credits {
            value["credits"] = credits;
        }
        if let Some(plan) = update.plan_type {
            value["plan_type"] = json!(plan);
        }
        if update.primary.is_none() && update.secondary.is_none() {
            continue;
        }
        let target = if update.limit_id == "codex" {
            &mut value
        } else {
            if !value["additional_rate_limits"].is_array() {
                value["additional_rate_limits"] = json!([]);
            }
            let limits = value["additional_rate_limits"].as_array_mut().unwrap();
            let index = limits
                .iter()
                .position(|l| {
                    l["metered_feature"]
                        .as_str()
                        .is_some_and(|s| normalize(s) == update.limit_id)
                })
                .unwrap_or_else(|| {
                    limits.push(json!({"metered_feature": update.limit_id}));
                    limits.len() - 1
                });
            let target = &mut limits[index];
            if let Some(name) = update.limit_name {
                target["limit_name"] = json!(name);
            }
            target
        };
        if !target["rate_limit"].is_object() {
            target["rate_limit"] = json!({});
        }
        let rate = target["rate_limit"].as_object_mut().unwrap();
        // Percentage observations do not assert account availability or clear errors.
        rate.remove("allowed");
        rate.remove("limit_reached");
        if let Some(w) = update.primary {
            rate.insert("primary_window".into(), w);
        }
        if let Some(w) = update.secondary {
            rate.insert("secondary_window".into(), w);
        }
    }
    value
}

/// A bounded, passive SSE tap. Oversized/non-quota events are still forwarded intact.
#[derive(Default)]
struct SseQuotaParser {
    line: Vec<u8>,
    line_nonempty: bool,
    data: Vec<u8>,
    skip: bool,
    cr: bool,
}
impl SseQuotaParser {
    fn push(&mut self, bytes: &[u8]) -> Vec<Update> {
        let mut updates = Vec::new();
        for &byte in bytes {
            if byte == b'\n' && self.cr {
                self.cr = false;
                continue;
            }
            self.cr = byte == b'\r';
            if byte == b'\n' || byte == b'\r' {
                if !self.line_nonempty {
                    if !self.skip
                        && let Ok(text) = std::str::from_utf8(&self.data)
                        && let Some(update) = event_update(text)
                    {
                        updates.push(update);
                    }
                    self.data.clear();
                    self.skip = false;
                } else if !self.skip
                    && let Some(data) = self.line.strip_prefix(b"data:")
                {
                    self.data
                        .extend_from_slice(data.strip_prefix(b" ").unwrap_or(data));
                    self.data.push(b'\n');
                }
                self.line.clear();
                self.line_nonempty = false;
            } else {
                self.line_nonempty = true;
                if !self.skip && self.line.len() + self.data.len() < 64 * 1024 {
                    self.line.push(byte);
                } else {
                    self.skip = true;
                }
            }
        }
        updates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_headers_preserve_partial_windows_and_separate_named_limits() {
        let mut headers = HeaderMap::new();
        for (name, value) in [
            ("x-codex-primary-used-percent", "12.5"),
            ("x-codex-primary-window-minutes", "43200"),
            ("x-codex-primary-reset-at", "2000000000"),
            ("x-codex-other-primary-used-percent", "100"),
            ("x-codex-other-primary-window-minutes", "300"),
        ] {
            headers.insert(name, value.parse().unwrap());
        }
        let previous = QuotaSnapshot {
            observed_at: DateTime::from_timestamp(1000, 0).unwrap(),
            value: json!({"plan_type":"plus", "rate_limit":{"allowed":false,
                "primary_window":{"used_percent":99,"reset_after_seconds":10},
                "secondary_window":{"used_percent":30,"limit_window_seconds":604800,"reset_after_seconds":100}},
                "credits":{"has_credits":true,"unlimited":false,"balance":"5"}}),
        };
        let value = merge(Some(previous), header_updates(&headers));
        assert_eq!(value["rate_limit"]["primary_window"]["used_percent"], 12.5);
        assert_eq!(
            value["rate_limit"]["primary_window"]["limit_window_seconds"],
            2592000
        );
        assert_eq!(value["rate_limit"]["secondary_window"]["reset_at"], 1100);
        assert!(
            value["rate_limit"]["primary_window"]
                .get("reset_after_seconds")
                .is_none()
        );
        assert!(value["rate_limit"].get("allowed").is_none());
        assert_eq!(
            value["additional_rate_limits"][0]["metered_feature"],
            "codex_other"
        );
        assert_eq!(value["credits"]["balance"], "5");
        assert_eq!(value["plan_type"], "plus");
        assert!(header_updates(&HeaderMap::new()).is_empty());
        headers.insert("x-codex-primary-used-percent", "NaN".parse().unwrap());
        assert!(
            header_updates(&headers)
                .iter()
                .all(|u| u.limit_id != "codex")
        );
    }

    #[test]
    fn fragmented_sse_ignores_invalid_and_oversized_events_and_resumes() {
        let event = r#"{"type":"codex.rate_limits","rate_limits":{"primary":{"used_percent":18.5,"window_minutes":300,"reset_at":2000000000}}}"#;
        for separator in ["\n", "\r\n", "\r"] {
            let wire = format!(
                "data: {}{separator}{separator}data: {event}{separator}{separator}data: {{\"type\":\"codex.rate_limits\",\"rate_limits\":{{\"primary\":{{\"used_percent\":\"bad\"}}}}}}{separator}{separator}",
                "x".repeat(70000)
            );
            let mut parser = SseQuotaParser::default();
            let updates = wire
                .as_bytes()
                .chunks(7)
                .flat_map(|chunk| parser.push(chunk))
                .collect();
            let value = merge(None, updates);
            assert_eq!(value["rate_limit"]["primary_window"]["used_percent"], 18.5);
        }
        assert!(
            event_update(r#"{"type":"response.output_text.delta","delta":"codex.rate_limits"}"#)
                .is_none()
        );
        assert!(event_update(r#"{"type":"codex.rate_limits"}"#).is_none());
    }

    #[tokio::test]
    async fn observations_are_persisted_before_forwarding_and_stale_credentials_are_ignored() {
        let path =
            std::env::temp_dir().join(format!("response-quota-{}.sqlite", uuid::Uuid::new_v4()));
        let storage = Storage::open(&path).await.unwrap();
        let accounts = codex2api_accounts::SupplierAccountStore::open(storage.clone());
        let account = accounts.create_pending().await.unwrap().account;
        let other = accounts.create_pending().await.unwrap().account;
        let revision = storage
            .supplier_auth_revision(&account.id)
            .await
            .unwrap()
            .unwrap();
        let observer = QuotaObserver {
            storage: storage.clone(),
            account_id: account.id.clone(),
            revision,
        };
        let event = r#"{"type":"codex.rate_limits","rate_limits":{"primary":{"used_percent":22,"window_minutes":300,"reset_at":2000000000}}}"#;
        let wire = format!("data: {event}\n\ndata: [DONE]\n\n");
        let chunks = wire
            .as_bytes()
            .chunks(5)
            .map(|b| Ok::<_, std::io::Error>(bytes::Bytes::copy_from_slice(b)))
            .collect::<Vec<_>>();
        let mut response: reqwest::Response = http::Response::builder()
            .status(200)
            .header("content-type", "text/event-stream")
            .header("x-codex-primary-used-percent", "10")
            .header("x-codex-primary-window-minutes", "300")
            .body(reqwest::Body::wrap_stream(futures::stream::iter(chunks)))
            .unwrap()
            .into();
        response
            .extensions_mut()
            .insert(crate::SupplierAuthRevision(revision));
        let response = observer.response(response).await;
        assert_eq!(
            storage
                .get_account_quota(&account.id)
                .await
                .unwrap()
                .unwrap()
                .value["rate_limit"]["primary_window"]["used_percent"],
            10.0
        );
        assert_eq!(
            response
                .extensions()
                .get::<crate::SupplierAuthRevision>()
                .unwrap()
                .0,
            revision
        );
        assert_eq!(response.text().await.unwrap(), wire);
        let current = storage
            .get_account_quota(&account.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            current.value["rate_limit"]["primary_window"]["used_percent"],
            22.0
        );
        assert!(
            storage
                .get_account_quota(&other.id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            !storage
                .update_account_quota(
                    &account.id,
                    revision,
                    current.observed_at - chrono::TimeDelta::seconds(1),
                    |_| panic!("older observation")
                )
                .await
                .unwrap()
        );
        let stale = QuotaObserver {
            revision: revision - 1,
            ..observer.clone()
        };
        stale.event(&event.replace("22", "99")).await;
        assert_eq!(
            storage
                .get_account_quota(&account.id)
                .await
                .unwrap()
                .unwrap(),
            current
        );
        assert!(
            !storage
                .supplier_health(&account.id)
                .await
                .unwrap()
                .authentication_invalid
        );
        storage.close().await;
        drop(accounts);
        drop(observer);
        drop(stale);
        drop(storage);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn http_and_websocket_transport_capture_headers_and_events() {
        use axum::{Router, extract::WebSocketUpgrade, response::IntoResponse, routing::get};
        use codex2api_accounts::{AuthDotJson, SupplierAccountStore, TokenData};
        let path =
            std::env::temp_dir().join(format!("quota-transport-{}.sqlite", uuid::Uuid::new_v4()));
        let storage = Storage::open(&path).await.unwrap();
        let accounts = SupplierAccountStore::open(storage.clone());
        let account = accounts.create_pending().await.unwrap().account;
        accounts
            .save_auth_for_account(
                &account.id,
                &AuthDotJson::chatgpt(
                    TokenData {
                        id_token: "fixture".into(),
                        access_token: "fixture".into(),
                        refresh_token: "fixture".into(),
                        account_id: Some(account.id.clone()),
                    },
                    Some(Utc::now()),
                ),
            )
            .await
            .unwrap();
        let client = crate::UpstreamClient::from_context(
            accounts.load_context(&account.id).await.unwrap(),
            Some(codex2api_auth::AuthService::new(accounts.clone()).unwrap()),
        )
        .unwrap()
        .with_direct_test_http();
        let event = r#"{"type":"codex.rate_limits","rate_limits":{"secondary":{"used_percent":45,"window_minutes":10080,"reset_at":2000000000}}}"#;
        let app = Router::new()
            .route(
                "/http",
                get(|| async {
                    (
                        [
                            ("x-codex-primary-used-percent", "25"),
                            ("x-codex-primary-window-minutes", "300"),
                        ],
                        "unchanged",
                    )
                }),
            )
            .route(
                "/socket",
                get(move |upgrade: WebSocketUpgrade| async move {
                    let mut response = upgrade
                        .on_upgrade(move |mut socket| async move {
                            socket
                                .send(axum::extract::ws::Message::Text(event.into()))
                                .await
                                .unwrap();
                            socket
                                .send(axum::extract::ws::Message::Binary(
                                    event.replace("45", "46").into_bytes().into(),
                                ))
                                .await
                                .unwrap();
                            let _ = socket.recv().await;
                        })
                        .into_response();
                    response
                        .headers_mut()
                        .insert("x-codex-primary-used-percent", "30".parse().unwrap());
                    response
                        .headers_mut()
                        .insert("x-codex-primary-window-minutes", "300".parse().unwrap());
                    response
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let response = client
            .send_prepared(
                http::Method::GET,
                &format!("http://{address}/http"),
                crate::request::PreparedRequest {
                    body: bytes::Bytes::new(),
                    headers: HeaderMap::new(),
                },
                true,
            )
            .await
            .unwrap();
        assert_eq!(response.text().await.unwrap(), "unchanged");
        assert_eq!(
            storage
                .get_account_quota(&account.id)
                .await
                .unwrap()
                .unwrap()
                .value["rate_limit"]["primary_window"]["used_percent"],
            25.0
        );
        let (mut socket, _) = client
            .connect_websocket_to(&format!("ws://{address}/socket"), HeaderMap::new())
            .await
            .unwrap();
        assert_eq!(
            storage
                .get_account_quota(&account.id)
                .await
                .unwrap()
                .unwrap()
                .value["rate_limit"]["primary_window"]["used_percent"],
            30.0
        );
        assert_eq!(
            socket.next().await.unwrap().unwrap().to_text().unwrap(),
            event
        );
        assert_eq!(
            storage
                .get_account_quota(&account.id)
                .await
                .unwrap()
                .unwrap()
                .value["rate_limit"]["secondary_window"]["used_percent"],
            45.0
        );
        assert_eq!(
            socket.next().await.unwrap().unwrap().to_text().unwrap(),
            event.replace("45", "46")
        );
        assert_eq!(
            storage
                .get_account_quota(&account.id)
                .await
                .unwrap()
                .unwrap()
                .value["rate_limit"]["secondary_window"]["used_percent"],
            46.0
        );
        socket.close(None).await.unwrap();
        server.abort();
        storage.close().await;
        drop(client);
        drop(accounts);
        drop(storage);
        let _ = std::fs::remove_file(path);
    }
}
