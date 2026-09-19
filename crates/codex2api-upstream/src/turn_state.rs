//! Opt-in, HTTP-only cross-turn routing experiment. Not part of the official contract.
use crate::{Result, UpstreamClient, X_CODEX_TURN_STATE_HEADER, request::PreparedRequest};
use base64::{
    Engine,
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
};
use codex2api_storage::{Storage, TurnStateCache, TurnStateProbeResult, TurnStateSettings};
use futures::StreamExt;
use http::{HeaderMap, HeaderValue, Method};
use serde_json::{Value, json};
use std::time::Duration;

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

// Envelope/age check only, never a signature check or a model-quality signal.
fn issued_at(token: &str, ttl: i64, time: i64) -> Option<i64> {
    if token.len() > 2048 {
        return None;
    }
    let bytes = URL_SAFE
        .decode(token)
        .or_else(|_| URL_SAFE_NO_PAD.decode(token))
        .ok()?;
    if bytes.len() != 57 + 16 * 10 || bytes[0] != 0x80 {
        return None;
    }
    let issued = i64::try_from(u64::from_be_bytes(bytes[1..9].try_into().ok()?)).ok()?;
    if !(1577836800..4102444800).contains(&issued)
        || issued > time + 30
        || issued + ttl - 30 <= time
    {
        return None;
    }
    Some(issued)
}

struct Probe {
    token: Option<String>,
    issued: i64,
    status: i64,
    result: &'static str,
    delay: i64,
}
impl Probe {
    fn failed(result: &'static str, delay: i64) -> Self {
        Self {
            token: None,
            issued: 0,
            status: 0,
            result,
            delay,
        }
    }
}

impl UpstreamClient {
    pub(crate) async fn send_responses_with_state(
        &self,
        url: &str,
        mut prepared: PreparedRequest,
    ) -> Result<reqwest::Response> {
        let context = self.prepare_turn_state(url, &mut prepared).await;
        let response = self
            .send_prepared(Method::POST, url, prepared, true)
            .await?;
        if let Some((storage, entry, ttl)) = context {
            if let Some(state) = response
                .headers()
                .get(X_CODEX_TURN_STATE_HEADER)
                .and_then(|v| v.to_str().ok())
            {
                let valid = response.status() == 200 && issued_at(state, ttl, now()).is_some();
                if storage.observe_turn_state(&entry, valid).await.is_err() {
                    tracing::warn!("turn-state observation could not be saved");
                }
            }
        }
        Ok(response)
    }

    async fn prepare_turn_state(
        &self,
        url: &str,
        prepared: &mut PreparedRequest,
    ) -> Option<(Storage, TurnStateCache, i64)> {
        let storage = self.auth_service()?.accounts().storage().ok()?.clone();
        let (settings, _) = storage
            .turn_state_settings(&self.identity().account_id)
            .await
            .ok()?;
        if !settings.enabled || settings.validate().is_err() {
            return None;
        }
        self.synchronize_auth().await.ok()?;
        let (settings, revision) = storage
            .turn_state_settings(&self.identity().account_id)
            .await
            .ok()?;
        if !settings.enabled || settings.validate().is_err() {
            return None;
        }
        let body = prepared.body.clone();
        let headers = prepared.headers.clone();
        let metadata =
            tokio::task::spawn_blocking(move || crate::request_metadata(&body, &headers))
                .await
                .ok()?
                .ok()?;
        let model = metadata.model.as_deref()?;
        if !settings.models.iter().any(|m| m == model) || metadata.generate == Some(false) {
            return None;
        }
        // Only the selected upstream account determines cache ownership.
        let owner = self.chatgpt_account_id().filter(|v| !v.is_empty())?;
        storage
            .ensure_turn_state_entry(&self.identity().account_id, model, &owner, &revision)
            .await
            .ok()?;
        let entry = storage
            .turn_state_entry(&self.identity().account_id, model, &owner, &revision)
            .await
            .ok()??;
        if prepared.headers.contains_key(X_CODEX_TURN_STATE_HEADER) {
            let _ = storage.record_turn_state_use(&entry, false).await;
            return None;
        }
        if let Ok(Some(lease)) = storage
            .claim_turn_state_probe(&entry, now(), settings.cooldown)
            .await
        {
            let probe = match tokio::time::timeout(
                Duration::from_secs(20),
                self.probe_turn_state(url, model, &settings),
            )
            .await
            {
                Ok(probe) => probe,
                Err(_) => Probe::failed("timeout", settings.cooldown),
            };
            let time = now();
            let _ = storage
                .finish_turn_state_probe(
                    &entry,
                    &lease,
                    TurnStateProbeResult {
                        token: probe.token.as_deref(),
                        issued_at: probe.issued,
                        expires_at: probe.issued + settings.ttl - 30,
                        refresh_at: probe.issued + settings.ttl - settings.renew,
                        now: time,
                        status: probe.status,
                        result: probe.result,
                        next_probe_at: time.saturating_add(probe.delay),
                    },
                )
                .await;
        }
        // Reload after the probe so clear/disable/reauth during the await cannot resurrect it.
        let entry = storage
            .turn_state_entry(&self.identity().account_id, model, &owner, &revision)
            .await
            .ok()??;
        let token = entry.token.as_deref()?;
        if entry.expires_at <= now() || issued_at(token, settings.ttl, now()).is_none() {
            return None;
        }
        let value = HeaderValue::from_str(token).ok()?;
        prepared.headers.insert(X_CODEX_TURN_STATE_HEADER, value);
        let _ = storage.record_turn_state_use(&entry, true).await;
        Some((storage, entry, settings.ttl))
    }

    async fn probe_turn_state(
        &self,
        url: &str,
        model: &str,
        settings: &TurnStateSettings,
    ) -> Probe {
        let body = json!({"model":model,"instructions":"Reply with OK.","input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"Reply with OK."}]}],"stream":true,"store":false,"parallel_tool_calls":true,"include":["reasoning.encrypted_content"]});
        let prepared = match crate::request::prepare_responses(
            &serde_json::to_vec(&body).expect("JSON value"),
            &HeaderMap::new(),
            &self.identity().installation_id,
            self.identity().http_fingerprint.timezone.as_deref(),
        ) {
            Ok(prepared) => prepared,
            Err(_) => return Probe::failed("invalid_probe", settings.cooldown),
        };
        // Bypass experiment dispatch to avoid recursion. Uses this account's auth and transport.
        let response = match self.send_probe_prepared(url, prepared).await {
            Ok(response) => response,
            Err(_) => return Probe::failed("network_error", settings.cooldown),
        };
        let status = i64::from(response.status().as_u16());
        let mut result = Probe {
            status,
            ..Probe::failed("http_error", settings.cooldown)
        };
        if status == 401 || status == 403 {
            result.delay = settings.ttl;
        }
        if status == 429 {
            result.delay = retry_delay(response.headers(), settings.cooldown, now());
        }
        if status != 200 {
            return result;
        }
        let token = response
            .headers()
            .get(X_CODEX_TURN_STATE_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        if issued_at(&token, settings.ttl, now()).is_none() {
            result.result = "invalid_state";
            return result;
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(chunk) if bytes.len() + chunk.len() <= 1024 * 1024 => {
                    bytes.extend_from_slice(&chunk)
                }
                _ => {
                    result.result = "incomplete_stream";
                    return result;
                }
            }
        }
        if completed_sse(&bytes) {
            if let Some(issued) = issued_at(&token, settings.ttl, now()) {
                result.token = Some(token);
                result.issued = issued;
                result.result = "accepted";
                return result;
            }
        }
        result.result = "incomplete_stream";
        result
    }
}

fn retry_delay(headers: &HeaderMap, fallback: i64, time: i64) -> i64 {
    let Some(value) = headers.get("retry-after").and_then(|v| v.to_str().ok()) else {
        return fallback;
    };
    value
        .parse::<i64>()
        .ok()
        .or_else(|| {
            chrono::DateTime::parse_from_rfc2822(value)
                .ok()
                .map(|date| date.timestamp().saturating_sub(time))
        })
        .unwrap_or(fallback)
        .max(fallback)
}

fn completed_sse(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut completed = false;
    // Only dispatch terminated frames. Text inside a delta cannot impersonate an event.
    for frame in normalized.split_inclusive("\n\n") {
        if frame.trim().is_empty() {
            continue;
        }
        if !frame.ends_with("\n\n") {
            return false;
        }
        let data = frame
            .lines()
            .filter_map(|line| {
                line.strip_prefix("data:")
                    .map(|s| s.strip_prefix(' ').unwrap_or(s))
            })
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&data) else {
            return false;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("response.completed") => {
                if value
                    .pointer("/response/status")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s != "completed")
                {
                    return false;
                }
                completed = true;
            }
            Some("response.failed" | "response.incomplete" | "error") => return false,
            _ => {}
        }
    }
    completed
}

#[cfg(test)]
mod tests {
    use super::*;
    fn token(blocks: usize, time: i64) -> String {
        let mut bytes = vec![0; 57 + 16 * blocks];
        bytes[0] = 0x80;
        bytes[1..9].copy_from_slice(&time.to_be_bytes());
        URL_SAFE.encode(bytes)
    }
    #[test]
    fn envelope_expiry_future_and_padding() {
        let time = 1_900_000_000;
        let good = token(10, time);
        assert_eq!(good.len(), 292);
        assert_eq!(issued_at(&good, 3600, time), Some(time));
        assert_eq!(
            issued_at(good.trim_end_matches('='), 3600, time),
            Some(time)
        );
        for value in [
            token(11, time),
            token(10, time - 3570),
            token(10, time + 31),
            "invalid".into(),
            "x".repeat(2049),
        ] {
            assert!(issued_at(&value, 3600, time).is_none());
        }
        assert!(issued_at(&good, 3600, time + 3569).is_some());
    }
    #[test]
    fn only_complete_real_sse_events_are_accepted() {
        assert!(completed_sse(b"event: response.completed\r\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\r\n\r\n"));
        for input in [
            "data: {\"type\":\"response.completed\"}",
            "event: response.completed\n\ndata: {\"type\":\"response.failed\"}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"response.completed\"}\n\n",
            "data: {\"type\":\"response.completed\"}\n\ndata: {\"type\":\"error\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"incomplete\"}}\n\n",
        ] {
            assert!(!completed_sse(input.as_bytes()));
        }
    }
    #[test]
    fn retry_after_honors_seconds_and_http_dates() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", HeaderValue::from_static("900"));
        assert_eq!(retry_delay(&headers, 300, 0), 900);
        headers.insert(
            "retry-after",
            HeaderValue::from_static("Thu, 01 Jan 1970 00:10:00 GMT"),
        );
        assert_eq!(retry_delay(&headers, 300, 0), 600);
    }

    async fn fixture_client(storage: &Storage, id: &str) -> UpstreamClient {
        use codex2api_accounts::{AccountStore, AuthDotJson, TokenData};
        use codex2api_storage::{AccountStatus, NewAccount};
        let mut new = NewAccount::pending_identity(
            id,
            "codex_cli_rs",
            "test",
            "test",
            "test",
            "test",
            "",
            "{}",
        );
        new.id = Some(id.into());
        new.status = AccountStatus::Active;
        new.chatgpt_account_id = Some(id.into());
        storage.create_account(new).await.unwrap();
        let accounts = AccountStore::open(storage.clone());
        let auth = AuthDotJson::chatgpt(
            TokenData {
                access_token: format!("token-{id}"),
                account_id: Some(id.into()),
                ..Default::default()
            },
            Some(chrono::Utc::now()),
        );
        accounts.save_auth_for_account(id, &auth).await.unwrap();
        UpstreamClient::from_context(
            accounts.load_context(id).await.unwrap(),
            Some(codex2api_auth::AuthService::new(accounts).unwrap()),
        )
        .unwrap()
        .with_direct_test_http()
    }

    fn prepared(model: &str, state: Option<&str>, compressed: bool) -> PreparedRequest {
        let body =
            serde_json::to_vec(&json!({"model":model,"input":"private user task","stream":true}))
                .unwrap();
        let mut headers = HeaderMap::new();
        if let Some(state) = state {
            headers.insert(
                X_CODEX_TURN_STATE_HEADER,
                HeaderValue::from_str(state).unwrap(),
            );
        }
        let body = if compressed {
            headers.insert("content-encoding", HeaderValue::from_static("zstd"));
            zstd::stream::encode_all(body.as_slice(), 1).unwrap()
        } else {
            body
        };
        crate::request::prepare_responses(&body, &headers, "installation", None).unwrap()
    }

    #[tokio::test]
    async fn turn_state_http_probe_reuse_isolation_priority_and_fallback() {
        use axum::{
            Router,
            body::{Body, Bytes},
            response::Response,
            routing::post,
        };
        use std::sync::{
            Arc,
            atomic::{AtomicU16, Ordering},
        };
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("upstream.sqlite"))
            .await
            .unwrap();
        let client_a = fixture_client(&storage, "account-a").await;
        let client_b = fixture_client(&storage, "account-b").await;
        let records = Arc::new(tokio::sync::Mutex::new(Vec::<(
            String,
            String,
            bool,
            Option<String>,
        )>::new()));
        let mode = Arc::new(AtomicU16::new(200));
        let observed = records.clone();
        let server_mode = mode.clone();
        let app = Router::new().route("/responses",post(move |headers: HeaderMap, body: Bytes| {
            let records = observed.clone(); let mode = server_mode.clone();
            async move {
                let body = crate::request::decode_body(&body, &headers).unwrap();
                let probe = body.get("instructions").and_then(Value::as_str) == Some("Reply with OK.");
                let owner = headers["chatgpt-account-id"].to_str().unwrap().to_owned();
                assert_eq!(headers["authorization"],format!("Bearer token-{owner}"));
                let model = body["model"].as_str().unwrap().to_owned();
                let state = headers.get(X_CODEX_TURN_STATE_HEADER).map(|v| v.to_str().unwrap().to_owned());
                records.lock().await.push((owner.clone(),model.clone(),probe,state.clone()));
                if probe {
                    assert!(!String::from_utf8(serde_json::to_vec(&body).unwrap()).unwrap().contains("private user task"));
                    assert!(state.is_none());
                    let status = mode.load(Ordering::SeqCst);
                    if status != 200 && status != 201 {
                        return Response::builder().status(status).header("retry-after","900").body(Body::empty()).unwrap();
                    }
                    let mut raw = URL_SAFE.decode(token(10,now())).unwrap();
                    raw[25] = if owner == "account-a" { 1 } else { 2 };
                    raw[26] = if model == "gpt-6-astra" { 1 } else { 2 };
                    let state = URL_SAFE.encode(raw);
                    let body = if status == 201 { "data: {\"type\":\"response.failed\"}\n\n" } else { "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n" };
                    Response::builder().header(X_CODEX_TURN_STATE_HEADER,state).body(Body::from(body)).unwrap()
                } else {
                    assert_eq!(body["input"],"private user task");
                    Response::builder().header(X_CODEX_TURN_STATE_HEADER,state.unwrap_or_default()).body(Body::from("untouched streaming response")).unwrap()
                }
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/responses", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        // Default disabled: exactly one real request, no probe.
        client_a
            .send_responses_with_state(&url, prepared("gpt-6-astra", None, false))
            .await
            .unwrap();
        assert_eq!(records.lock().await.len(), 1);
        let settings = TurnStateSettings {
            enabled: true,
            models: vec!["gpt-6-astra".into(), "other".into()],
            ..Default::default()
        };
        for id in ["account-a", "account-b"] {
            storage
                .save_turn_state_settings(id, &settings)
                .await
                .unwrap();
        }
        let response = client_a
            .send_responses_with_state(&url, prepared("gpt-6-astra", None, true))
            .await
            .unwrap();
        let state_a = response.headers()[X_CODEX_TURN_STATE_HEADER]
            .to_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            response.text().await.unwrap(),
            "untouched streaming response"
        );
        assert_eq!(state_a.len(), 292);
        let response = client_a
            .send_responses_with_state(&url, prepared("gpt-6-astra", None, false))
            .await
            .unwrap();
        assert_eq!(response.headers()[X_CODEX_TURN_STATE_HEADER], state_a);
        let response = client_b
            .send_responses_with_state(&url, prepared("gpt-6-astra", None, false))
            .await
            .unwrap();
        assert_ne!(response.headers()[X_CODEX_TURN_STATE_HEADER], state_a);
        let response = client_a
            .send_responses_with_state(&url, prepared("other", None, false))
            .await
            .unwrap();
        assert_ne!(response.headers()[X_CODEX_TURN_STATE_HEADER], state_a);
        let response = client_a
            .send_responses_with_state(
                &url,
                prepared("gpt-6-astra", Some("client-owned-state"), false),
            )
            .await
            .unwrap();
        assert_eq!(
            response.headers()[X_CODEX_TURN_STATE_HEADER],
            "client-owned-state"
        );
        let response = client_a
            .send_responses_with_state(&url, prepared("unconfigured", None, false))
            .await
            .unwrap();
        assert_eq!(response.headers()[X_CODEX_TURN_STATE_HEADER], "");
        assert_eq!(records.lock().await.iter().filter(|r| r.2).count(), 3);
        // Concurrent first requests share a single probe lease.
        storage.clear_turn_state("account-a").await.unwrap();
        let (a, b) = tokio::join!(
            client_a.send_responses_with_state(&url, prepared("gpt-6-astra", None, false)),
            client_a.send_responses_with_state(&url, prepared("gpt-6-astra", None, false))
        );
        a.unwrap();
        b.unwrap();
        assert_eq!(records.lock().await.iter().filter(|r| r.2).count(), 4);
        // HTTP failures and incomplete SSE never cache and never replay a real request.
        for status in [401, 403, 429, 201] {
            storage.clear_turn_state("account-a").await.unwrap();
            mode.store(status, Ordering::SeqCst);
            let before = records.lock().await.iter().filter(|r| !r.2).count();
            let response = client_a
                .send_responses_with_state(&url, prepared("gpt-6-astra", None, false))
                .await
                .unwrap();
            assert_eq!(response.headers()[X_CODEX_TURN_STATE_HEADER], "");
            let entries = storage.turn_state_entries("account-a").await.unwrap();
            assert!(entries[0].token.is_none());
            if status == 429 {
                assert!(entries[0].next_probe_at >= entries[0].last_probe_at + 900);
            }
            client_a
                .send_responses_with_state(&url, prepared("gpt-6-astra", None, false))
                .await
                .unwrap();
            assert_eq!(
                records.lock().await.iter().filter(|r| !r.2).count(),
                before + 2
            );
        }
        server.abort();
    }
}
