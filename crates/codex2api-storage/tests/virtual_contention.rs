use codex2api_storage::{Storage, VirtualAccount};
use serde_json::json;

async fn fixture() -> (tempfile::TempDir, Storage) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("contention.sqlite"))
        .await
        .unwrap();
    storage
        .save_virtual_account(&VirtualAccount {
            provider_id: "chatgpt".into(),
            id: "account".into(),
            username: "account".into(),
            password_hash: "unused".into(),
            name: "Test".into(),
            email: "test@example.test".into(),
            plan_type: "pro".into(),
            plan_id: "pro".into(),
            subscription_expires_at: None,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
        .await
        .unwrap();
    (dir, storage)
}

#[tokio::test]
async fn existing_configuration_reads_do_not_acquire_sqlite_write_locks() {
    let (_dir, storage) = fixture().await;
    storage.virtual_quota("account").await.unwrap();
    let key = storage.oauth_signing_key().await.unwrap();
    storage.desktop_support_settings().await.unwrap();
    let tx = storage.pool().begin_with("BEGIN IMMEDIATE").await.unwrap();
    let reads = tokio::time::timeout(std::time::Duration::from_millis(300), async {
        let (quota, secret, support) = tokio::join!(
            storage.virtual_quota("account"),
            storage.oauth_signing_key(),
            storage.desktop_support_settings()
        );
        assert_eq!(quota.unwrap()["plan_type"], "pro");
        assert_eq!(secret.unwrap(), key);
        assert!(support.unwrap().collect_diagnostics);
    })
    .await;
    tx.rollback().await.unwrap();
    assert!(
        reads.is_ok(),
        "Existing configuration reads waited for an unrelated writer"
    );
}

#[tokio::test]
async fn resource_updates_keep_source_and_conversation_events_atomic() {
    let (_dir, storage) = fixture().await;
    storage
        .save_virtual_resource(
            "account",
            "conversation",
            "thread",
            None,
            &json!({"id":"thread","status":"in_progress"}),
        )
        .await
        .unwrap();
    let initial = storage.virtual_events("account", 0).await.unwrap().len();
    storage
        .save_virtual_resource(
            "account",
            "conversation",
            "thread",
            None,
            &json!({"id":"thread","status":"in_progress"}),
        )
        .await
        .unwrap();
    assert_eq!(
        storage.virtual_events("account", 0).await.unwrap().len(),
        initial
    );
    storage
        .save_virtual_resource(
            "account",
            "conversation",
            "thread",
            None,
            &json!({"id":"thread","status":"completed"}),
        )
        .await
        .unwrap();
    assert_eq!(
        storage
            .virtual_events("account", 0)
            .await
            .unwrap()
            .iter()
            .filter(|e| e["payload"]["type"] == "conversation-turn-complete")
            .count(),
        2
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn large_catalog_refresh_keeps_parallel_account_reads_and_logging_available() {
    let (_dir, storage) = fixture().await;
    storage.virtual_quota("account").await.unwrap();
    let apps=(0..4300).map(|i|json!({"id":format!("connector-{i}"),"name":format!("Connector {i}"),"description":"public metadata"})).collect::<Vec<_>>();
    let start = std::time::Instant::now();
    let mut jobs = tokio::task::JoinSet::new();
    let writer = storage.clone();
    jobs.spawn(async move {
        for _ in 0..2 {
            writer
                .save_virtual_connector_catalog("account", &apps)
                .await
                .unwrap();
        }
    });
    for worker in 0..8 {
        let storage = storage.clone();
        jobs.spawn(async move {
            for i in 0..40 {
                assert_eq!(
                    storage.virtual_quota("account").await.unwrap()["plan_type"],
                    "pro"
                );
                storage
                    .record_virtual_request("account", "test-device", "GET", "/test", 200, i)
                    .await
                    .unwrap();
                storage
                    .save_virtual_resource(
                        "account",
                        "test-record",
                        &format!("{worker}-{i}"),
                        None,
                        &json!({"id":i}),
                    )
                    .await
                    .unwrap();
            }
        });
    }
    while let Some(result) = jobs.join_next().await {
        result.unwrap();
    }
    assert_eq!(
        storage
            .virtual_resources("account", "connector_catalog")
            .await
            .unwrap()
            .len(),
        4300
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM virtual_request_logs")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        320
    );
    eprintln!(
        "4300-entry catalog x2, 320 quota reads and 640 concurrent writes: {:?}",
        start.elapsed()
    );
}
