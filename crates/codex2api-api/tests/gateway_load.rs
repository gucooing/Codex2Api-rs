//! In-process acceptance: real routers, policies, scheduling, encoding and SQLite.
//! No sockets: only upstream network transmission is replaced with a memory transport.
#[path = "../../codex2api-storage/test-support/accounts.rs"]
mod account_fixture;
use account_fixture::AccountFixture;
use axum::{
    body::{Body, Bytes},
    http::Request,
};
use codex2api_storage::{NewSupplierAccount, PlatformAccount, Storage};
use futures::StreamExt;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tower::ServiceExt;

#[derive(Default)]
struct Round {
    active: AtomicUsize,
    peak: AtomicUsize,
    arrived: AtomicUsize,
    released: AtomicBool,
    gate: tokio::sync::Notify,
    arrivals: Mutex<Vec<u64>>,
    start: Mutex<Option<Instant>>,
}
impl Round {
    fn release(&self) {
        self.released.store(true, Ordering::Release);
        self.gate.notify_waiters();
    }
    async fn wait(&self) {
        let ready = self.gate.notified();
        tokio::pin!(ready);
        ready.as_mut().enable();
        if !self.released.load(Ordering::Acquire) {
            ready.await;
        }
    }
}

struct MemoryUpstream {
    current: Arc<Mutex<Arc<Round>>>,
    concurrency: usize,
    provider: &'static str,
}
impl codex2api_upstream::HttpTransport for MemoryUpstream {
    fn send(&self, request: reqwest::RequestBuilder) -> codex2api_upstream::HttpFuture {
        let round = self.current.lock().unwrap().clone();
        let concurrency = self.concurrency;
        let provider = self.provider;
        Box::pin(async move {
            let request = request.build()?;
            assert_eq!(
                request.headers()["authorization"],
                "Bearer fixture-supplier"
            );
            if request.method() == reqwest::Method::GET {
                let id = request.headers()["chatgpt-account-id"].to_str().unwrap();
                let body = json!({"accounts":[{"id":id,"workspace_backend_origin":"NO_CONSTRAINT","account_routing_override":"NO_CONSTRAINT"}]}).to_string();
                return Ok(http::Response::builder()
                    .header("content-type", "application/json")
                    .body(reqwest::Body::from(body))
                    .unwrap()
                    .into());
            }
            let body = codex2api_upstream::decode_body(
                request.body().unwrap().as_bytes().unwrap(),
                request.headers(),
            )?;
            let active = round.active.fetch_add(1, Ordering::AcqRel) + 1;
            round.peak.fetch_max(active, Ordering::Relaxed);
            round
                .arrivals
                .lock()
                .unwrap()
                .push(round.start.lock().unwrap().unwrap().elapsed().as_micros() as u64);
            if round.arrived.fetch_add(1, Ordering::AcqRel) + 1 == concurrency {
                round.release();
            }
            let id = uuid::Uuid::new_v4().to_string();
            let mut first = format!(
                "data: {}\n\n",
                json!({"type":"response.output_text.delta","response_id":id,"delta":"ok"})
            );
            if provider == "chatgpt" {
                first.push_str(&format!("data: {}\n\n", json!({"type":"codex.rate_limits","plan_type":"plus","rate_limits":{
                    "primary":{"used_percent":13,"window_minutes":300,"reset_at":2000000000_i64},
                    "secondary":{"used_percent":21,"window_minutes":10080,"reset_at":2000000000_i64}
                }})));
            }
            let last = format!(
                "data: {}\n\n",
                json!({"type":"response.completed","response":{"id":id,"status":"completed","model":body["model"],"output":[],"usage":{"input_tokens":2,"output_tokens":1}}})
            );
            let stream =
                futures::stream::once(async move { Ok::<_, std::io::Error>(Bytes::from(first)) })
                    .chain(futures::stream::once(async move {
                        round.wait().await;
                        round.active.fetch_sub(1, Ordering::AcqRel);
                        Ok(Bytes::from(last))
                    }));
            let mut response =
                http::Response::builder().header("content-type", "text/event-stream");
            if provider == "chatgpt" {
                response = response
                    .header("x-codex-primary-used-percent", "12.5")
                    .header("x-codex-primary-window-minutes", "300")
                    .header("x-codex-primary-reset-at", "2000000000");
            }
            Ok(response
                .body(reqwest::Body::wrap_stream(stream))
                .unwrap()
                .into())
        })
    }
}

fn percentiles(mut values: Vec<u64>) -> Value {
    values.sort_unstable();
    let at = |p: usize| {
        values
            .get(values.len().saturating_sub(1) * p / 100)
            .copied()
            .unwrap_or(0) as f64
            / 1000.0
    };
    json!({"p50_ms":at(50),"p95_ms":at(95),"p99_ms":at(99),"max_ms":at(100)})
}

#[cfg(windows)]
fn cpu_seconds() -> Option<f64> {
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetProcessTimes(
            process: *mut std::ffi::c_void,
            created: *mut FileTime,
            exited: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
    let (mut created, mut exited, mut kernel, mut user) = (
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
    );
    // Read-only process accounting; no handles or buffers escape this call.
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    };
    assert_ne!(ok, 0);
    Some(
        (((kernel.high as u64) << 32 | kernel.low as u64)
            + ((user.high as u64) << 32 | user.low as u64)) as f64
            / 10_000_000.0,
    )
}
#[cfg(not(windows))]
fn cpu_seconds() -> Option<f64> {
    None
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "1000 supplier / 3000 concurrent request performance acceptance; run with --release"]
async fn thousand_suppliers_three_thousand_requests() {
    let provider = if std::env::var("CODEX2API_LOAD_PROVIDER").as_deref() == Ok("chatgpt") {
        "chatgpt"
    } else {
        "grok"
    };
    let model = if provider == "chatgpt" {
        "gpt-5.5"
    } else {
        "grok-4.7"
    };
    let concurrency = std::env::var("CODEX2API_LOAD_REQUESTS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(3000);
    let supplier_count = 1000;
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("load.sqlite")).await.unwrap();
    let mut free = storage.plan_fixture(provider).await.unwrap();
    free.config = json!({"rpm_limit":0,"concurrency_limit":0,"model_access":"all","models":[],"spending_windows":[{"duration_seconds":2592000,"cost_limit_usd":"1000"}]});
    storage
        .save_virtual_plan(&free, Some(free.revision))
        .await
        .unwrap();
    let owner = PlatformAccount {
        id: "load-owner".into(),
        provider_id: provider.into(),
        username: "load-owner".into(),
        password_hash: "fixture".into(),
        name: "Load fixture".into(),
        email: "fixture@example.test".into(),
        plan_id: free.id,
        plan_type: free.plan_type.clone(),
        subscription_expires_at: None,
        enabled: true,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    storage.save_account_fixture(&owner).await.unwrap();
    let device = storage
        .create_virtual_device(&owner, "fixture-refresh", &Default::default())
        .await
        .unwrap()
        .unwrap();
    storage
        .register_virtual_access(
            &device,
            "fixture-refresh",
            "fixture-access",
            chrono::Utc::now().timestamp() + 3600,
        )
        .await
        .unwrap();
    if provider == "grok" {
        sqlx::query("UPDATE virtual_access_tokens SET scopes='api:access'")
            .execute(storage.pool())
            .await
            .unwrap();
    }
    storage
        .sync_supported_models(&[codex2api_core::SupportedModel {
            provider_id: provider.into(),
            model: model.into(),
            kind: "text".into(),
        }])
        .await
        .unwrap();
    storage
        .save_supplier_tag("load-pool", provider, "Load fixture")
        .await
        .unwrap();
    for n in 0..supplier_count {
        let id = format!("supplier-{n:04}");
        let (mut account, tokens) = if provider == "grok" {
            let identity = codex2api_accounts::providers::grok::GrokIdentity::generate();
            (
                NewSupplierAccount::pending_identity(
                    identity.installation_id.clone(),
                    identity.originator.clone(),
                    identity.user_agent.clone(),
                    identity.os_type.clone(),
                    identity.os_version.clone(),
                    identity.arch.clone(),
                    "",
                    identity.fingerprint_json().unwrap(),
                ),
                codex2api_auth::grok::GrokCredentials {
                    access_token: "fixture-supplier".into(),
                    refresh_token: None,
                    id_token: None,
                    expires_at: Some(chrono::Utc::now().timestamp() + 3600),
                    profile: json!({"userId":id}),
                }
                .to_supplier_tokens(&id),
            )
        } else {
            let identity = codex2api_accounts::AccountIdentity::new(
                id.clone(),
                uuid::Uuid::new_v4().to_string(),
                codex2api_accounts::HostRuntime {
                    originator: "codex_cli_rs".into(),
                    user_agent: format!(
                        "codex_cli_rs/{} (Linux 6; x86_64) fixture",
                        codex2api_version::CODEX_PACKAGE_VERSION
                    ),
                    os_type: "Linux".into(),
                    os_version: "6".into(),
                    arch: "x86_64".into(),
                },
            );
            (
                NewSupplierAccount::pending_identity(
                    identity.installation_id.clone(),
                    identity.originator.clone(),
                    identity.user_agent.clone(),
                    identity.os_type.clone(),
                    identity.os_version.clone(),
                    identity.arch.clone(),
                    "",
                    identity.fingerprint_json().unwrap(),
                ),
                codex2api_storage::SupplierTokens {
                    access_token: Some("fixture-supplier".into()),
                    auth_mode: Some("chatgpt".into()),
                    last_refresh: Some(chrono::Utc::now().to_rfc3339()),
                    ..Default::default()
                },
            )
        };
        account.id = Some(id.clone());
        account.provider_id = provider.into();
        account.chatgpt_account_id = Some(format!("user:{id}"));
        account.chatgpt_user_id = Some(id.clone());
        storage
            .save_authorized_account(account, tokens, None)
            .await
            .unwrap();
        storage.add_supplier_model(&id, model).await.unwrap();
        storage
            .edit_supplier_tags(&[id], &["load-pool".into()], false)
            .await
            .unwrap();
    }
    storage
        .save_pool_route(&owner.id, provider, Some("load-pool"), Some(1))
        .await
        .unwrap();
    assert_eq!(
        storage
            .pool_candidates("load-pool", provider, Some(model))
            .await
            .unwrap()
            .len(),
        supplier_count
    );
    storage.invalidate_runtime_cache();

    let current = Arc::new(Mutex::new(Arc::new(Round::default())));
    let accounts = codex2api_accounts::SupplierAccountStore::open(storage.clone());
    let auth = codex2api_auth::AuthService::new(accounts.clone()).unwrap();
    let pool =
        codex2api_upstream::UpstreamPool::new(auth).with_transport(Arc::new(MemoryUpstream {
            current: current.clone(),
            concurrency,
            provider,
        }));
    let api = codex2api_api::router(codex2api_api::ApiState::new(
        storage.clone(),
        accounts,
        pool,
    ));
    let mut reports = Vec::new();
    let mut all_passed = true;
    for phase in ["cold", "warm-new-sessions", "warm-sticky"] {
        let phase_started_at = chrono::Utc::now();
        let round = Arc::new(Round::default());
        *round.start.lock().unwrap() = Some(Instant::now());
        *current.lock().unwrap() = round.clone();
        let release = round.clone();
        let watchdog = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            release.release();
        });
        let sampling = Arc::new(AtomicBool::new(true));
        let running = sampling.clone();
        let sampler = tokio::spawn(async move {
            let mut delays = Vec::new();
            let cores = std::thread::available_parallelism().unwrap().get() as f64;
            let mut cpu_start = cpu_seconds();
            let mut cpu_at = Instant::now();
            let mut cpu_peak = cpu_start.map(|_| 0.0_f64);
            while running.load(Ordering::Relaxed) {
                let expected = tokio::time::Instant::now() + Duration::from_millis(10);
                tokio::time::sleep_until(expected).await;
                delays.push(
                    tokio::time::Instant::now()
                        .saturating_duration_since(expected)
                        .as_micros() as u64,
                );
                if cpu_at.elapsed() >= Duration::from_millis(100) {
                    let cpu = cpu_seconds();
                    if let (Some(cpu), Some(start), Some(peak)) = (cpu, cpu_start, &mut cpu_peak) {
                        *peak = peak
                            .max((cpu - start) / cpu_at.elapsed().as_secs_f64() / cores * 100.0);
                    }
                    cpu_start = cpu;
                    cpu_at = Instant::now();
                }
            }
            (delays, cpu_peak)
        });
        let start = Instant::now();
        let cpu_start = cpu_seconds();
        let mut requests = tokio::task::JoinSet::new();
        for n in 0..concurrency {
            let app = api.clone();
            let session = if phase == "cold" {
                format!("cold-{n}")
            } else {
                format!("warm-{n}")
            };
            requests.spawn(async move {
                let began = Instant::now();
                let request = Request::builder().method("POST")
                    .uri(if provider == "chatgpt" { "/v1/responses" } else { "/grok/v1/responses" })
                    .header("authorization", "Bearer fixture-access").header("content-type", "application/json")
                    .header("session-id", session)
                    .body(Body::from(json!({"model":model,"stream":true,"input":[{"role":"user","content":"load fixture"}]}).to_string())).unwrap();
                let response = app.oneshot(request).await.unwrap();
                let status = response.status().as_u16();
                let result = match axum::body::to_bytes(response.into_body(), 1024 * 1024).await {
                    Ok(body) if status == 200 && String::from_utf8_lossy(&body).contains("response.completed") => Ok(()),
                    Ok(body) => Err(format!("{status}: {}", String::from_utf8_lossy(&body).chars().take(180).collect::<String>())),
                    Err(error) => Err(error.to_string()),
                };
                (began.elapsed().as_micros() as u64, result)
            });
        }
        let mut latencies = Vec::new();
        let mut failures = std::collections::BTreeMap::<String, usize>::new();
        while let Some(result) = requests.join_next().await {
            let (latency, result) = result.unwrap();
            latencies.push(latency);
            if let Err(error) = result {
                *failures.entry(error).or_default() += 1;
            }
        }
        let response_seconds = start.elapsed().as_secs_f64();
        let settled = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let pending: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM usage_records WHERE status='in_progress'",
                )
                .fetch_one(storage.pool())
                .await
                .unwrap();
                if pending == 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .is_ok();
        let (usage_records, completed_records): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*),COALESCE(SUM(status='completed'),0) FROM usage_records",
        )
        .fetch_one(storage.pool())
        .await
        .unwrap();
        let expected_records = ((reports.len() + 1) * concurrency) as i64;
        let settled =
            settled && usage_records == expected_records && completed_records == expected_records;
        let (quota_snapshots, quota_complete) = if provider == "chatgpt" {
            let (snapshots, used): (i64, i64) = sqlx::query_as(
                "SELECT COUNT(*),(SELECT COUNT(DISTINCT account_id) FROM usage_records WHERE requested_at_ms>=?) FROM account_quota_cache WHERE observed_at>=? AND json_extract(response_json,'$.rate_limit.primary_window.used_percent')=13 AND json_extract(response_json,'$.rate_limit.secondary_window.used_percent')=21",
            ).bind(phase_started_at.timestamp_millis()).bind(phase_started_at).fetch_one(storage.pool()).await.unwrap();
            (Some(snapshots), snapshots == used)
        } else {
            (None, true)
        };
        let elapsed = start.elapsed().as_secs_f64();
        let cpu = cpu_seconds().zip(cpu_start).map(|(end, start)| end - start);
        sampling.store(false, Ordering::Relaxed);
        let (delays, cpu_peak) = sampler.await.unwrap();
        watchdog.abort();
        let failed: usize = failures.values().sum();
        let peak = round.peak.load(Ordering::Relaxed);
        all_passed &= failed == 0 && peak == concurrency && settled && quota_complete;
        let cores = std::thread::available_parallelism().unwrap().get();
        let report = json!({"transport":"in-process; no sockets","provider":provider,"phase":phase,"suppliers":supplier_count,"requests":concurrency,"successful":concurrency-failed,"peak_upstream_concurrency":peak,"response_seconds":response_seconds,"usage_settled":settled,"usage_records":usage_records,"completed_usage_records":completed_records,"quota_snapshots":quota_snapshots,"quota_complete":quota_complete,"wall_seconds":elapsed,"process_cpu_seconds":cpu,"logical_cpus":cores,"average_machine_cpu_percent":cpu.map(|cpu| cpu/elapsed/cores as f64*100.0),"peak_100ms_machine_cpu_percent":cpu_peak,"throughput_rps":concurrency as f64/elapsed,"response_latency":percentiles(latencies),"admission_latency":percentiles(round.arrivals.lock().unwrap().clone()),"runtime_timer_delay":percentiles(delays),"failures":failures});
        eprintln!("{report}");
        reports.push(report);
        let output =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/performance");
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(
            output.join(format!("gateway-load-{provider}.json")),
            serde_json::to_vec_pretty(&reports).unwrap(),
        )
        .unwrap();
        if !settled {
            break;
        }
    }
    assert!(
        all_passed,
        "all requests must be admitted concurrently and complete; see target/performance/gateway-load-{provider}.json"
    );
}
