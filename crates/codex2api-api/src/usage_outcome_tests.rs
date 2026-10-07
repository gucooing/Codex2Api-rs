use super::*;
use serde_json::json;

async fn rows(storage: &Storage, count: usize) -> Vec<UsageRecord> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let rows = storage
                .query_usage(&Default::default())
                .await
                .unwrap()
                .records;
            if rows.len() == count && rows.iter().all(|r| r.status != "in_progress") {
                return rows;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn sse_and_ws_share_event_outcomes_independent_of_http_success() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("outcomes.sqlite"))
        .await
        .unwrap();
    let cases = [
        (
            "billing",
            json!({"type":"error","status":402,"error":{"message":"Payment Required"}}),
            "payment_required",
            Some(402),
        ),
        (
            "rate",
            json!({"type":"response.failed","response":{"error":{"code":"rate_limit_exceeded","message":"retry later"}}}),
            "rate_limit",
            Some(429),
        ),
        (
            "quota",
            json!({"type":"response.failed","response":{"error":{"code":"insufficient_quota","message":"exhausted"}}}),
            "quota_exhausted",
            Some(429),
        ),
        (
            "usage",
            json!({"type":"error","status":429,"error":{"type":"usage_limit_reached","resets_at":2000000000}}),
            "quota_exhausted",
            Some(429),
        ),
        (
            "unknown429",
            json!({"type":"error","status":429,"error":{"message":"rate quota token expired"}}),
            "limit_unknown",
            Some(429),
        ),
        (
            "auth",
            json!({"type":"error","status":401,"error":{"code":"invalid_token"}}),
            "authentication",
            Some(401),
        ),
        (
            "permission",
            json!({"type":"error","status":403,"error":{"code":"policy_denied"}}),
            "permission",
            Some(403),
        ),
        (
            "unknown",
            json!({"type":"response.failed","response":{"error":{"code":"new_failure"}}}),
            "upstream",
            None,
        ),
        (
            "incomplete",
            json!({"type":"response.incomplete","response":{"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"}}}),
            "incomplete",
            None,
        ),
        (
            "content-filter",
            json!({"type":"response.incomplete","response":{"status":"incomplete","incomplete_details":{"reason":"content_filter"}}}),
            "incomplete",
            None,
        ),
    ];
    let mut count = 0;
    for (name, event, kind, status) in cases {
        for transport in ["http", "websocket"] {
            let id = format!("{name}-{transport}");
            let mut log = RequestLog::begin(
                storage.clone(),
                UsageRecord {
                    id: id.clone(),
                    endpoint: "/v1/responses".into(),
                    transport: transport.into(),
                    status: "in_progress".into(),
                    ..Default::default()
                },
                Instant::now(),
            )
            .await
            .unwrap();
            if transport == "http" {
                let body = format!(
                    ": heartbeat\r\n\r\ndata: {{\"type\":\"response.created\",\"response\":{{\"id\":\"r\"}}}}\r\n\r\ndata: {event}\r\n\r\n"
                );
                let chunks = body
                    .as_bytes()
                    .chunks(3)
                    .map(|v| Ok::<_, std::io::Error>(Bytes::copy_from_slice(v)))
                    .collect::<Vec<_>>();
                let response = reqwest::Response::from(
                    http::Response::builder()
                        .status(200)
                        .header("content-type", "text/event-stream")
                        .body(reqwest::Body::wrap_stream(futures::stream::iter(chunks)))
                        .unwrap(),
                );
                let output = axum::body::to_bytes(log.wrap(response), usize::MAX)
                    .await
                    .unwrap();
                assert_eq!(output.as_ref(), body.as_bytes());
            } else {
                log.parse(&serde_json::to_vec(&event).unwrap());
                log.finish("completed");
            }
            count += 1;
            let result = rows(&storage, count).await;
            let record = result.iter().find(|r| r.id == id).unwrap();
            assert_eq!(
                record.status,
                if kind == "incomplete" {
                    "incomplete"
                } else {
                    "failed"
                }
            );
            assert_eq!(record.failure_kind.as_deref(), Some(kind), "{id}");
            if name == "content-filter" {
                assert_eq!(record.error_code.as_deref(), Some("content_filter"));
            }
            assert_eq!(record.failure_status, status, "{id}");
            assert_eq!(record.http_status, (transport == "http").then_some(200));
        }
    }
    storage.close().await;
}

#[tokio::test]
async fn broken_http_stream_only_fails_pending_generation_and_keeps_supplier_available() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("transport.sqlite"))
        .await
        .unwrap();
    let supplier = codex2api_accounts::SupplierAccountStore::open(storage.clone())
        .create_pending()
        .await
        .unwrap()
        .account;
    for (index, completed, reset) in [(0, false, false), (1, false, true), (2, true, true)] {
        let log = RequestLog::begin(
            storage.clone(),
            UsageRecord {
                id: index.to_string(),
                account_id: supplier.id.clone(),
                status: "in_progress".into(),
                endpoint: "/v1/responses".into(),
                transport: "http".into(),
                ..Default::default()
            },
            Instant::now(),
        )
        .await
        .unwrap();
        let event = if completed {
            json!({"type":"response.completed","response":{"usage":{"input_tokens":7,"output_tokens":3}}})
        } else {
            json!({"type":"response.created","response":{"id":"r"}})
        };
        let mut chunks = vec![Ok(Bytes::from(format!("data: {event}\n\n")))];
        if reset {
            chunks.push(Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "fixture reset",
            )));
        }
        let response = reqwest::Response::from(
            http::Response::builder()
                .status(200)
                .header("content-type", "text/event-stream")
                .body(reqwest::Body::wrap_stream(futures::stream::iter(chunks)))
                .unwrap(),
        );
        let output = axum::body::to_bytes(log.wrap(response), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&output).contains("response.failed"),
            !completed
        );
        let result = rows(&storage, index + 1).await;
        let record = result.iter().find(|r| r.id == index.to_string()).unwrap();
        assert_eq!(
            record.status,
            if completed { "completed" } else { "failed" }
        );
        assert_eq!(record.failure_status, (!completed).then_some(502));
        if completed {
            assert_eq!(record.input_tokens, Some(7));
            assert_eq!(record.output_tokens, Some(3));
        }
        assert!(
            !storage
                .supplier_health(&supplier.id)
                .await
                .unwrap()
                .authentication_invalid
        );
    }
    storage.close().await;
}

#[tokio::test]
async fn old_rejection_cannot_disable_new_credentials_and_legacy_network_faults_do_not_gate() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("credentials.sqlite"))
        .await
        .unwrap();
    let supplier = codex2api_accounts::SupplierAccountStore::open(storage.clone())
        .create_pending()
        .await
        .unwrap()
        .account;
    sqlx::query("INSERT INTO supplier_health(account_id,error_message,error_at,revision) VALUES(?,'与 ChatGPT 官方连接失败或超时','old',1)")
        .bind(&supplier.id).execute(storage.pool()).await.unwrap();
    let health = storage.supplier_health(&supplier.id).await.unwrap();
    assert!(health.error_message.is_some());
    assert!(!health.authentication_invalid);
    let revision = storage
        .supplier_auth_revision(&supplier.id)
        .await
        .unwrap()
        .unwrap();
    storage
        .reject_supplier_auth(&supplier.id, revision)
        .await
        .unwrap();
    assert!(
        storage
            .supplier_health(&supplier.id)
            .await
            .unwrap()
            .authentication_invalid
    );
    storage
        .upsert_supplier_tokens(codex2api_storage::SupplierTokens {
            account_id: supplier.id.clone(),
            access_token: Some("new-fixture".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    storage
        .reject_supplier_auth(&supplier.id, revision)
        .await
        .unwrap();
    assert!(
        !storage
            .supplier_health(&supplier.id)
            .await
            .unwrap()
            .authentication_invalid
    );
    storage.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn installed_desktop_reads_actual_sse_failure_events() {
    if std::env::var_os("CODEX2API_TEST_CLI").is_none() {
        return;
    }
    use axum::{Router, extract::Path, routing::post};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("desktop.sqlite"))
        .await
        .unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = hits.clone();
    let saved = storage.clone();
    let app = Router::new().route("/{kind}/responses", post(move |Path(kind): Path<String>| {
        let storage = saved.clone(); let hits = counted.clone();
        async move {
            let id = hits.fetch_add(1, Ordering::SeqCst).to_string();
            let log = RequestLog::begin(storage, UsageRecord { id, status:"in_progress".into(),
                endpoint:"/v1/responses".into(), transport:"http".into(), ..Default::default() }, Instant::now()).await.unwrap();
            let code = match kind.as_str() { "quota" => "insufficient_quota", "context" => "context_length_exceeded", _ => "rate_limit_exceeded" };
            let body = format!("data: {{\"type\":\"response.created\",\"response\":{{\"id\":\"fixture\"}}}}\n\ndata: {}\n\n",
                json!({"type":"response.failed","response":{"id":"fixture","status":"failed","error":{"code":code,"message":format!("fixture {code}")}}}));
            let response = reqwest::Response::from(http::Response::builder().status(200).header("content-type","text/event-stream").body(body).unwrap());
            crate::response::forward_response(http::StatusCode::OK, response.headers().clone(), log.wrap(response))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    for (kind, expected) in [
        ("quota", "usageLimitExceeded"),
        ("context", "contextWindowExceeded"),
        ("rate", "rate_limit_exceeded"),
    ] {
        let url = format!("http://{address}/{kind}");
        let result = tokio::task::spawn_blocking(move || {
            std::process::Command::new("python")
                .args(["-X", "utf8"])
                .arg(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../scripts/windows/Test-DesktopResponseOutcomes.py"),
                )
                .arg(url)
                .arg(expected)
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        println!("{}", String::from_utf8_lossy(&result.stdout));
    }
    assert!(
        hits.load(Ordering::SeqCst) >= 3,
        "Native runtime must actually send inference requests"
    );
    server.abort();
    storage.close().await;
}
