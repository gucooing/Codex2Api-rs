mod common;

use axum::http::StatusCode;
use codex2api_storage::UsageRecord;
use common::Fixture;
use serde_json::{Value, json};

async fn record(
    f: &Fixture,
    id: &str,
    owner: &str,
    at: i64,
    input: Option<i64>,
    cached: Option<i64>,
    status: &str,
) {
    let mut record = UsageRecord {
        id: id.into(),
        subject_id: owner.into(),
        subject_name: "Shared display name".into(),
        account_id: "supplier".into(),
        account_name: "Supplier".into(),
        model: Some("requested-model".into()),
        actual_model: Some("actual-model".into()),
        requested_at_ms: at,
        status: "in_progress".into(),
        ..Default::default()
    };
    f.storage.insert_usage(&record).await.unwrap();
    record.input_tokens = input;
    record.cached_tokens = cached;
    record.output_tokens = input.map(|_| 20);
    record.reasoning_tokens = input.map(|_| 10);
    record.cache_write_tokens = input.map(|_| 5);
    record.status = status.into();
    f.storage.finish_usage(&record).await.unwrap();
}

#[tokio::test]
async fn overview_aggregates_full_ledger_with_weighted_cache_and_exact_account_filters() {
    let f = Fixture::new().await;
    let start = chrono::DateTime::parse_from_rfc3339("2026-09-20T16:00:00Z")
        .unwrap()
        .timestamp_millis();
    // More than one page: the dashboard must not aggregate just visible records.
    for index in 0..25 {
        record(
            &f,
            &format!("a-{index}"),
            "a",
            start + index,
            Some(100),
            Some(80),
            "completed",
        )
        .await;
    }
    record(
        &f,
        "large",
        "a",
        start + 3_600_000,
        Some(2500),
        Some(0),
        "client_stopped",
    )
    .await;
    record(&f, "other", "b", start, Some(9000), Some(9000), "failed").await;
    record(
        &f,
        "before",
        "a",
        start - 1,
        Some(9000),
        Some(9000),
        "completed",
    )
    .await;
    record(
        &f,
        "until",
        "a",
        start + 86_400_000,
        Some(9000),
        Some(9000),
        "completed",
    )
    .await;
    let range = "from=2026-09-21T00:00&until=2026-09-22T00:00&tz_offset=-480";
    let stats = f
        .get(&format!(
            "/admin/api/overview/usage?{range}&virtual_account=a&group_by=hour"
        ))
        .await;
    let summary = &stats["summary"];
    assert_eq!(summary["request_count"], 26);
    assert_eq!(summary["completed_requests"], 26);
    assert_eq!(summary["input_tokens"], 5000);
    assert_eq!(summary["output_tokens"], 520);
    assert_eq!(summary["total_tokens"], 5520); // Cached/reasoning are not counted twice.
    assert_eq!(summary["cache_rate"], 40.0); // Weighted totals, not mean per-request ratios.
    assert_eq!(stats["rows"][0]["key"], "2026-09-21T00:00");
    assert_eq!(stats["rows"][1]["key"], "2026-09-21T01:00");
    assert_eq!(stats["rows"].as_array().unwrap().len(), 2);
    assert_eq!(stats["from_ms"], start);
    assert_eq!(stats["model_usage"][0]["bucket"], "2026-09-21T00:00");
    assert_eq!(stats["model_usage"][0]["model"], "actual-model");
    assert_eq!(stats["model_usage"][0]["total_tokens"], 3000);
    assert_eq!(stats["model_usage"][1]["total_tokens"], 2520);
    let by_account = f
        .get(&format!(
            "/admin/api/overview/usage?{range}&group_by=virtual_account"
        ))
        .await;
    assert_eq!(by_account["rows"].as_array().unwrap().len(), 2); // Same label, different identity.
    assert_eq!(by_account["model_usage"][0]["bucket"], "a");
    assert_eq!(by_account["model_usage"][0]["total_tokens"], 5520);
    assert_eq!(by_account["model_usage"][1]["bucket"], "b");
    assert_eq!(by_account["model_usage"][1]["total_tokens"], 9020);
    let by_model = f
        .get(&format!(
            "/admin/api/overview/usage?{range}&model=requested&group_by=model&status=completed"
        ))
        .await;
    assert_eq!(by_model["rows"].as_array().unwrap().len(), 1);
    assert_eq!(by_model["rows"][0]["key"], "actual-model");
    assert_eq!(by_model["summary"]["request_count"], 26);
    assert_eq!(by_model["model_usage"][0]["bucket"], "actual-model");
    assert_eq!(by_model["model_usage"][0]["total_tokens"], 5520);
    let failed = f
        .get(&format!(
            "/admin/api/overview/usage?{range}&model=actual&status=failed"
        ))
        .await;
    assert_eq!(failed["summary"]["failed_requests"], 1);
    assert_eq!(failed["summary"]["total_tokens"], 9020); // Actual reported usage survives failure.
}

#[tokio::test]
async fn overview_preserves_unknowns_empty_results_and_persisted_costs() {
    let f = Fixture::new().await;
    let start = chrono::DateTime::parse_from_rfc3339("2026-09-21T00:00:00Z")
        .unwrap()
        .timestamp_millis();
    record(&f, "known", "a", start, Some(100), Some(80), "completed").await;
    record(&f, "unknown", "a", start, None, None, "failed").await;
    // Persisted settlement is authoritative even if the current price differs.
    sqlx::query("UPDATE usage_records SET cost_nano_usd=123456789 WHERE id='known'")
        .execute(f.storage.pool())
        .await
        .unwrap();
    let range = "from=2026-09-21T00:00&until=2026-09-22T00:00";
    let stats = f.get(&format!("/admin/api/overview/usage?{range}")).await;
    assert_eq!(stats["summary"]["total_tokens"], 120);
    assert!(stats["summary"]["cache_rate"].is_null());
    assert_eq!(stats["summary"]["missing_token_requests"], 1);
    assert_eq!(stats["summary"]["missing_cache_requests"], 1);
    assert_eq!(stats["summary"]["cost_nano_usd"], 123456789);
    assert_eq!(stats["summary"]["unpriced_requests"], 1);
    let unknown = f
        .get(&format!("/admin/api/overview/usage?{range}&status=failed"))
        .await;
    for field in [
        "total_tokens",
        "input_tokens",
        "output_tokens",
        "cached_tokens",
        "cache_rate",
        "cost_nano_usd",
    ] {
        assert!(unknown["summary"][field].is_null(), "{field}");
    }
    let empty = f
        .get(&format!(
            "/admin/api/overview/usage?{range}&virtual_account=missing"
        ))
        .await;
    assert_eq!(empty["summary"]["request_count"], 0);
    assert_eq!(empty["summary"]["total_tokens"], 0);
    assert!(empty["summary"]["cache_rate"].is_null());
    assert!(empty["rows"].as_array().unwrap().is_empty());
    for query in [
        "group_by=invalid",
        "tz_offset=9999",
        "from=bad",
        "from=2026-09-22T00:00&until=2026-09-21T00:00",
        "from=2025-01-01T00:00&until=2026-09-21T00:00",
        "group_by=hour&from=2026-01-01T00:00&until=2026-03-01T00:00",
    ] {
        assert_eq!(
            f.request(
                "GET",
                &format!("/admin/api/overview/usage?{query}"),
                Value::Null
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST,
            "{query}"
        );
    }
    assert_eq!(
        f.with_auth("GET", "/admin/api/overview/usage", json!(null), None, None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}
