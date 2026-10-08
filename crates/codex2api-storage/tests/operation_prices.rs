use codex2api_storage::{OperationPrice, Storage, UsageFilter, UsageRecord};

#[tokio::test]
async fn search_charges_once_at_start_price_and_preserves_failures_and_unknowns() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("search.sqlite"))
        .await
        .unwrap();
    let mut price = OperationPrice {
        provider_id: "chatgpt".into(),
        operation: "search".into(),
        price_nano_usd: Some(12_000_000),
        revision: 0,
    };
    assert!(storage.save_operation_price(&price, None).await.unwrap());
    assert!(!storage.save_operation_price(&price, None).await.unwrap());
    for (id, provider, status, expected_cost, expected_status) in [
        (
            "success",
            "chatgpt",
            "completed",
            Some(12_000_000),
            "priced",
        ),
        ("error", "chatgpt", "failed", Some(0), "not_charged"),
        ("partial", "chatgpt", "interrupted", None, "missing_usage"),
        ("other", "grok", "completed", None, "unsupported"),
    ] {
        let mut record = UsageRecord {
            id: id.into(),
            provider_id: provider.into(),
            subject_id: "owner".into(),
            endpoint: "/v1/alpha/search".into(),
            model: Some("gpt-6-astra".into()),
            status: "in_progress".into(),
            ..Default::default()
        };
        storage.insert_usage(&record).await.unwrap();
        record.status = status.into();
        record.http_status = Some(if status == "failed" { 429 } else { 200 });
        storage.finish_usage(&record).await.unwrap();
        storage.finish_usage(&record).await.unwrap();
        let row = storage
            .query_usage(&UsageFilter::default())
            .await
            .unwrap()
            .records
            .into_iter()
            .find(|r| r.id == id)
            .unwrap();
        assert_eq!(row.cost_nano_usd, expected_cost, "{id}");
        assert_eq!(row.billing_status, expected_status, "{id}");
        assert_eq!(row.input_tokens, None);
    }
    let mut record = UsageRecord {
        id: "frozen".into(),
        subject_id: "owner".into(),
        endpoint: "/v1/alpha/search".into(),
        model: Some("gpt-6-astra".into()),
        status: "in_progress".into(),
        ..Default::default()
    };
    storage.insert_usage(&record).await.unwrap();
    price.price_nano_usd = Some(80_000_000);
    assert!(storage.save_operation_price(&price, Some(1)).await.unwrap());
    assert!(!storage.save_operation_price(&price, Some(1)).await.unwrap());
    record.status = "completed".into();
    storage.finish_usage(&record).await.unwrap();
    assert_eq!(
        storage
            .query_usage(&UsageFilter::default())
            .await
            .unwrap()
            .records
            .into_iter()
            .find(|r| r.id == "frozen")
            .unwrap()
            .cost_nano_usd,
        Some(12_000_000)
    );
    price.price_nano_usd = None;
    storage.save_operation_price(&price, Some(2)).await.unwrap();
    record.id = "unpriced".into();
    record.status = "in_progress".into();
    storage.insert_usage(&record).await.unwrap();
    record.status = "completed".into();
    storage.finish_usage(&record).await.unwrap();
    let row = storage
        .query_usage(&UsageFilter::default())
        .await
        .unwrap()
        .records
        .into_iter()
        .find(|r| r.id == "unpriced")
        .unwrap();
    assert_eq!(row.cost_nano_usd, None);
    assert_eq!(row.billing_status, "unpriced");
    storage.close().await;
    let reopened = Storage::open(dir.path().join("search.sqlite"))
        .await
        .unwrap();
    assert_eq!(
        reopened
            .operation_price("chatgpt", "search")
            .await
            .unwrap()
            .unwrap()
            .revision,
        3
    );
    assert!(
        reopened
            .operation_price("grok", "search")
            .await
            .unwrap()
            .is_none()
    );
}
