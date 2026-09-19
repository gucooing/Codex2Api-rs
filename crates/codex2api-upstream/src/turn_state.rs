//! Capture eligible state from normal HTTP traffic and reuse it within an account/model.
use crate::{Result, UpstreamClient, X_CODEX_TURN_STATE_HEADER, request::PreparedRequest};
use base64::{
    Engine,
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
};
use codex2api_storage::{Storage, TurnStateCache, TurnStateObservation, TurnStateSettings};
use http::{HeaderValue, Method};

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

struct Inspection<'a> {
    token: Option<&'a str>,
    issued: i64,
    result: &'static str,
    length: i64,
    blocks: i64,
}

// Only a local envelope/age policy. This cannot authenticate or decrypt upstream state.
fn inspect(value: Option<&HeaderValue>, ttl: i64, time: i64) -> Inspection<'_> {
    let mut out = Inspection {
        token: None,
        issued: 0,
        result: "missing_header",
        length: 0,
        blocks: 0,
    };
    let Some(value) = value else {
        return out;
    };
    out.length = value.as_bytes().len() as i64;
    out.result = "invalid_encoding";
    let Ok(token) = value.to_str() else {
        return out;
    };
    if token.len() > 2048 {
        return out;
    }
    let Ok(bytes) = URL_SAFE
        .decode(token)
        .or_else(|_| URL_SAFE_NO_PAD.decode(token))
    else {
        return out;
    };
    out.result = "invalid_envelope";
    if bytes.len() < 73 || bytes[0] != 0x80 || (bytes.len() - 57) % 16 != 0 {
        return out;
    }
    out.blocks = ((bytes.len() - 57) / 16) as i64;
    out.result = "wrong_blocks";
    if out.blocks != 10 {
        return out;
    }
    let issued = u64::from_be_bytes(bytes[1..9].try_into().expect("envelope length"));
    out.result = "invalid_timestamp";
    if !(1577836800..4102444800).contains(&issued) {
        return out;
    }
    out.issued = issued as i64;
    out.result = "future_state";
    if out.issued > time + 30 {
        return out;
    }
    out.result = "expired";
    if out.issued + ttl - 30 <= time {
        return out;
    }
    out.token = Some(token);
    out.result = "accepted";
    out
}

struct TurnContext {
    storage: Storage,
    entry: TurnStateCache,
    settings: TurnStateSettings,
}

impl UpstreamClient {
    pub(crate) async fn send_responses_with_state(
        &self,
        url: &str,
        mut prepared: PreparedRequest,
    ) -> Result<reqwest::Response> {
        let context = match self.prepare_turn_state(&mut prepared).await {
            Ok(context) => context,
            Err(_) => {
                tracing::warn!(account_id=%self.identity().account_id, "turn-state preparation failed; forwarding request");
                None
            }
        };
        let response = self.send_prepared(Method::POST, url, prepared, true).await;
        if let Some(context) = context {
            let time = now();
            let (header, status) = match &response {
                Ok(response) => (
                    response.headers().get(X_CODEX_TURN_STATE_HEADER),
                    i64::from(response.status().as_u16()),
                ),
                Err(_) => (None, 0),
            };
            let mut observed = inspect(header, context.settings.ttl, time);
            if status != 200 {
                observed.token = None;
                observed.result = if status == 0 {
                    "network_error"
                } else {
                    "http_error"
                };
            }
            if context
                .storage
                .record_turn_state_observation(
                    &context.entry,
                    &context.settings,
                    TurnStateObservation {
                        from_client: false,
                        token: observed.token,
                        issued_at: observed.issued,
                        now: time,
                        status,
                        result: observed.result,
                        length: observed.length,
                        blocks: observed.blocks,
                        injected: false,
                    },
                )
                .await
                .is_err()
            {
                tracing::warn!(account_id=%context.entry.account_id, "could not persist response turn-state observation");
            }
        }
        response
    }

    async fn prepare_turn_state(
        &self,
        prepared: &mut PreparedRequest,
    ) -> anyhow::Result<Option<TurnContext>> {
        let Some(auth) = self.auth_service() else {
            return Ok(None);
        };
        let storage = auth.accounts().storage()?.clone();
        let (settings, _) = storage
            .turn_state_settings(&self.identity().account_id)
            .await?;
        if !settings.enabled {
            return Ok(None);
        }
        self.synchronize_auth().await?;
        // Auth refresh can invalidate the configuration generation.
        let (settings, revision) = storage
            .turn_state_settings(&self.identity().account_id)
            .await?;
        if !settings.enabled {
            return Ok(None);
        }
        settings.validate()?;
        let body = prepared.body.clone();
        let headers = prepared.headers.clone();
        let metadata =
            tokio::task::spawn_blocking(move || crate::request_metadata(&body, &headers)).await??;
        let Some(model) = metadata.model.filter(|m| settings.models.contains(m)) else {
            return Ok(None);
        };
        if metadata.generate == Some(false) {
            return Ok(None);
        }
        let Some(owner) = self.chatgpt_account_id().filter(|v| !v.is_empty()) else {
            return Ok(None);
        };
        storage
            .ensure_turn_state_entry(&self.identity().account_id, &model, &owner, &revision)
            .await?;
        let Some(entry) = storage
            .turn_state_entry(&self.identity().account_id, &model, &owner, &revision)
            .await?
        else {
            return Ok(None);
        };
        let time = now();
        let observed = inspect(
            prepared.headers.get(X_CODEX_TURN_STATE_HEADER),
            settings.ttl,
            time,
        );
        // Preserve eligible client state and refresh the cache; use cache only as fallback.
        let cached = entry
            .token
            .as_deref()
            .and_then(|v| HeaderValue::from_str(v).ok());
        let inject = observed.token.is_none()
            && entry.expires_at > time
            && inspect(cached.as_ref(), settings.ttl, time).token.is_some();
        storage
            .record_turn_state_observation(
                &entry,
                &settings,
                TurnStateObservation {
                    from_client: true,
                    token: observed.token,
                    issued_at: observed.issued,
                    now: time,
                    status: 0,
                    result: observed.result,
                    length: observed.length,
                    blocks: observed.blocks,
                    injected: inject,
                },
            )
            .await?;
        if inject {
            prepared
                .headers
                .insert(X_CODEX_TURN_STATE_HEADER, cached.expect("validated cache"));
        }
        Ok(Some(TurnContext {
            storage,
            entry,
            settings,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::{Body, Bytes},
        response::Response,
        routing::post,
    };
    use http::HeaderMap;
    use serde_json::{Value, json};
    use std::{sync::Arc, time::Duration};
    use tokio::sync::Mutex;

    fn token(blocks: usize, time: i64, marker: u8) -> String {
        let mut bytes = vec![marker; 57 + 16 * blocks];
        bytes[0] = 0x80;
        bytes[1..9].copy_from_slice(&time.to_be_bytes());
        URL_SAFE.encode(bytes)
    }

    #[test]
    fn turn_state_classifies_missing_invalid_routed_expired_and_target_headers() {
        let time = 1_900_000_000;
        assert_eq!(inspect(None, 3600, time).result, "missing_header");
        for (value, result, blocks) in [
            ("not-base64!".to_string(), "invalid_encoding", 0),
            (URL_SAFE.encode([0u8; 217]), "invalid_envelope", 0),
            (token(9, time, 1), "wrong_blocks", 9),
            (token(10, time - 3570, 1), "expired", 10),
            (token(10, time + 31, 1), "future_state", 10),
            (token(10, 0, 1), "invalid_timestamp", 10),
            (token(10, time, 1), "accepted", 10),
            (
                token(10, time, 1).trim_end_matches('=').to_owned(),
                "accepted",
                10,
            ),
        ] {
            let header = HeaderValue::from_str(&value).unwrap();
            let checked = inspect(Some(&header), 3600, time);
            assert_eq!(checked.result, result);
            assert_eq!(checked.blocks, blocks);
            assert_eq!(checked.token.is_some(), result == "accepted");
        }
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

    #[derive(Clone)]
    struct Reply {
        state: Option<String>,
        status: u16,
        pending_body: bool,
    }
    struct Seen {
        account: String,
        model: String,
        state: Option<String>,
    }
    struct Harness {
        _dir: tempfile::TempDir,
        storage: Storage,
        a: UpstreamClient,
        b: UpstreamClient,
        url: String,
        reply: Arc<Mutex<Reply>>,
        seen: Arc<Mutex<Vec<Seen>>>,
        server: tokio::task::JoinHandle<()>,
    }
    impl Drop for Harness {
        fn drop(&mut self) {
            self.server.abort();
        }
    }
    impl Harness {
        async fn new(enabled: bool) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let storage = Storage::open(dir.path().join("test.sqlite")).await.unwrap();
            let a = fixture_client(&storage, "account-a").await;
            let b = fixture_client(&storage, "account-b").await;
            let settings = TurnStateSettings {
                enabled,
                models: vec!["gpt-6-astra".into(), "other".into()],
                ..Default::default()
            };
            for id in ["account-a", "account-b"] {
                storage
                    .save_turn_state_settings(id, &settings)
                    .await
                    .unwrap();
            }
            let reply = Arc::new(Mutex::new(Reply {
                state: None,
                status: 200,
                pending_body: false,
            }));
            let seen = Arc::new(Mutex::new(Vec::new()));
            let app = Router::new().route(
                "/responses",
                post({
                    let reply = reply.clone();
                    let seen = seen.clone();
                    move |headers: HeaderMap, bytes: Bytes| {
                        let reply = reply.clone();
                        let seen = seen.clone();
                        async move {
                            assert_eq!(headers["content-encoding"], "zstd");
                            let decoded = zstd::stream::decode_all(bytes.as_ref()).unwrap();
                            let body: Value = serde_json::from_slice(&decoded).unwrap();
                            assert_eq!(body["input"], "real user request");
                            seen.lock().await.push(Seen {
                                account: headers["chatgpt-account-id"].to_str().unwrap().to_owned(),
                                model: body["model"].as_str().unwrap().to_owned(),
                                state: headers
                                    .get(X_CODEX_TURN_STATE_HEADER)
                                    .map(|v| v.to_str().unwrap().to_owned()),
                            });
                            let plan = reply.lock().await.clone();
                            let mut response = Response::builder().status(plan.status);
                            if let Some(state) = plan.state {
                                response = response.header(X_CODEX_TURN_STATE_HEADER, state);
                            }
                            let body = if plan.pending_body {
                                Body::from_stream(futures::stream::pending::<
                                    std::result::Result<Bytes, std::io::Error>,
                                >())
                            } else {
                                Body::from("original stream bytes")
                            };
                            response.body(body).unwrap()
                        }
                    }
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/responses", listener.local_addr().unwrap());
            let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            Self {
                _dir: dir,
                storage,
                a,
                b,
                url,
                reply,
                seen,
                server,
            }
        }
        async fn send(
            &self,
            client: &UpstreamClient,
            model: &str,
            state: Option<&str>,
            compressed: bool,
        ) -> reqwest::Response {
            let mut body = serde_json::to_vec(
                &json!({"model":model, "input":"real user request", "stream":true}),
            )
            .unwrap();
            let mut headers = HeaderMap::new();
            if let Some(state) = state {
                headers.insert(
                    X_CODEX_TURN_STATE_HEADER,
                    HeaderValue::from_str(state).unwrap(),
                );
            }
            if compressed {
                body = zstd::stream::encode_all(body.as_slice(), 1).unwrap();
                headers.insert("content-encoding", HeaderValue::from_static("zstd"));
            }
            tokio::time::timeout(
                Duration::from_secs(3),
                client.forward_to(&self.url, body.into(), headers),
            )
            .await
            .expect("normal request must not wait for a stream or extra request")
            .unwrap()
        }
        async fn entry(&self) -> TurnStateCache {
            self.storage
                .turn_state_entries("account-a")
                .await
                .unwrap()
                .into_iter()
                .find(|e| e.model == "gpt-6-astra")
                .unwrap()
        }
    }

    #[tokio::test]
    async fn turn_state_response_capture_then_cache_replaces_only_missing_or_invalid_state() {
        let h = Harness::new(true).await;
        let target = token(10, now(), 1);
        h.reply.lock().await.state = Some(target.clone());
        h.reply.lock().await.pending_body = true;
        // Capture at response headers, even if the normal SSE body has not completed.
        let response = h.send(&h.a, "gpt-6-astra", None, true).await;
        assert_eq!(response.headers()[X_CODEX_TURN_STATE_HEADER], target);
        assert_eq!(h.entry().await.source, "response");
        drop(response);
        *h.reply.lock().await = Reply {
            state: Some(token(9, now(), 2)),
            status: 200,
            pending_body: false,
        };
        for client_state in [
            None,
            Some("bad-client-state".to_owned()),
            Some(token(9, now(), 3)),
            Some(token(10, now() - 3600, 4)),
        ] {
            let response = h
                .send(&h.a, "gpt-6-astra", client_state.as_deref(), false)
                .await;
            assert_eq!(response.text().await.unwrap(), "original stream bytes");
        }
        let seen = h.seen.lock().await;
        assert_eq!(seen.len(), 5); // One upstream call per normal request, no auxiliary traffic.
        assert!(seen[0].state.is_none());
        assert!(
            seen[1..]
                .iter()
                .all(|s| s.state.as_deref() == Some(target.as_str()))
        );
        let entry = h.entry().await;
        assert_eq!(entry.token.as_deref(), Some(target.as_str()));
        assert_eq!(
            (entry.request_count, entry.response_count, entry.injections),
            (5, 5, 4)
        );
        assert_eq!(entry.response_result, "wrong_blocks");
        assert_eq!(entry.response_blocks, 9);
        assert_eq!(entry.response_captures, 1);
    }

    #[tokio::test]
    async fn turn_state_healthy_traffic_passes_through_and_refreshes_fallback_without_extending_old_state()
     {
        let h = Harness::new(true).await;
        let time = now();
        let initial = token(10, time - 100, 1);
        let newer = token(10, time - 50, 2);
        let response_state = token(10, time, 3);
        h.send(&h.a, "gpt-6-astra", Some(&initial), false).await;
        let initial_expiry = h.entry().await.expires_at;
        h.send(&h.a, "gpt-6-astra", Some(&newer), true).await;
        let entry = h.entry().await;
        assert_eq!(entry.token.as_deref(), Some(newer.as_str()));
        assert_eq!(entry.source, "client");
        assert_eq!(entry.injections, 0);
        assert_eq!(entry.expires_at, initial_expiry + 50);
        // Eligible older client state still passes through; only storage refuses regression.
        h.send(&h.a, "gpt-6-astra", Some(&initial), false).await;
        assert_eq!(h.entry().await.request_result, "older_state");
        assert_eq!(h.entry().await.token.as_deref(), Some(newer.as_str()));
        h.send(&h.a, "gpt-6-astra", Some("invalid"), false).await;
        assert_eq!(h.entry().await.request_result, "invalid_encoding");
        // A newer normal response refreshes the cache even with a healthy client state.
        h.reply.lock().await.state = Some(response_state.clone());
        h.send(&h.a, "gpt-6-astra", Some(&newer), false).await;
        let refreshed = h.entry().await;
        assert_eq!(refreshed.token.as_deref(), Some(response_state.as_str()));
        assert_eq!(refreshed.source, "response");
        assert_eq!(refreshed.expires_at, time + 3570);
        h.send(&h.a, "gpt-6-astra", None, false).await;
        assert_eq!(h.entry().await.expires_at, refreshed.expires_at);
        let seen = h.seen.lock().await;
        assert_eq!(seen.len(), 6);
        for (request, expected) in
            seen.iter()
                .zip([&initial, &newer, &initial, &newer, &newer, &response_state])
        {
            assert_eq!(request.state.as_deref(), Some(expected.as_str()));
        }
        assert_eq!(h.entry().await.injections, 2);
    }

    #[tokio::test]
    async fn turn_state_client_capture_is_immediate_and_accounts_models_are_isolated() {
        let h = Harness::new(true).await;
        let target = token(10, now(), 5);
        h.send(&h.a, "gpt-6-astra", Some(&target), false).await;
        assert_eq!(h.entry().await.source, "client");
        assert_eq!(h.entry().await.request_captures, 1);
        h.send(&h.a, "gpt-6-astra", None, true).await;
        h.send(&h.b, "gpt-6-astra", None, false).await;
        h.send(&h.a, "other", None, false).await;
        h.send(&h.a, "unconfigured", Some("untouched"), false).await;
        let seen = h.seen.lock().await;
        assert_eq!(seen.len(), 5);
        assert_eq!(seen[1].state.as_deref(), Some(target.as_str()));
        assert_eq!(seen[2].account, "account-b");
        assert!(seen[2].state.is_none());
        assert_eq!(seen[3].model, "other");
        assert!(seen[3].state.is_none());
        assert_eq!(seen[4].state.as_deref(), Some("untouched"));
        assert!(
            h.storage
                .turn_state_entries("account-a")
                .await
                .unwrap()
                .iter()
                .all(|e| e.model != "unconfigured")
        );
    }

    #[tokio::test]
    async fn turn_state_failures_and_missing_headers_are_visible_without_poisoning_cache() {
        let h = Harness::new(true).await;
        h.send(&h.a, "gpt-6-astra", None, false).await;
        assert_eq!(h.entry().await.response_result, "missing_header");
        assert_eq!(h.entry().await.response_status, 200);
        *h.reply.lock().await = Reply {
            state: Some(token(10, now(), 1)),
            status: 500,
            pending_body: false,
        };
        h.send(&h.a, "gpt-6-astra", None, false).await;
        assert!(h.entry().await.token.is_none());
        assert_eq!(h.entry().await.response_result, "http_error");
        h.reply.lock().await.status = 200;
        h.reply.lock().await.state = Some(token(10, now() - 3600, 1));
        h.send(&h.a, "gpt-6-astra", None, false).await;
        assert!(h.entry().await.token.is_none());
        assert_eq!(h.entry().await.response_result, "expired");
        h.reply.lock().await.state = Some(token(10, now(), 2));
        h.send(&h.a, "gpt-6-astra", None, false).await;
        assert_eq!(h.entry().await.response_captures, 1);
    }

    #[tokio::test]
    async fn turn_state_disabled_clear_and_expiry_return_to_natural_collection() {
        let h = Harness::new(false).await;
        let target = token(10, now(), 7);
        h.reply.lock().await.state = Some(target.clone());
        h.send(&h.a, "gpt-6-astra", Some("untouched"), false).await;
        assert!(
            h.storage
                .turn_state_entries("account-a")
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(h.seen.lock().await[0].state.as_deref(), Some("untouched"));
        h.storage
            .save_turn_state_settings(
                "account-a",
                &TurnStateSettings {
                    enabled: true,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        h.send(&h.a, "gpt-6-astra", None, false).await;
        install_expired_fixture(&h.storage).await;
        h.reply.lock().await.state = None;
        h.send(&h.a, "gpt-6-astra", None, false).await;
        assert!(h.seen.lock().await[2].state.is_none());
        h.storage.clear_turn_state("account-a").await.unwrap();
        h.send(&h.a, "gpt-6-astra", None, false).await;
        assert!(h.entry().await.token.is_none());
        assert!(h.seen.lock().await[3].state.is_none());
    }

    async fn install_expired_fixture(storage: &Storage) {
        // Use the public storage observation path to install an old fixture without sleeping.
        let (_, revision) = storage.turn_state_settings("account-a").await.unwrap();
        storage.clear_turn_state("account-a").await.unwrap();
        assert_ne!(
            storage.turn_state_settings("account-a").await.unwrap().1,
            revision
        );
        let (settings, revision) = storage.turn_state_settings("account-a").await.unwrap();
        storage
            .ensure_turn_state_entry("account-a", "gpt-6-astra", "account-a", &revision)
            .await
            .unwrap();
        let entry = storage
            .turn_state_entry("account-a", "gpt-6-astra", "account-a", &revision)
            .await
            .unwrap()
            .unwrap();
        storage
            .record_turn_state_observation(
                &entry,
                &settings,
                TurnStateObservation {
                    from_client: false,
                    token: Some(&token(10, now() - 3600, 7)),
                    issued_at: now() - 3600,
                    now: now() - 3600,
                    status: 200,
                    result: "accepted",
                    length: 292,
                    blocks: 10,
                    injected: false,
                },
            )
            .await
            .unwrap();
    }
}
