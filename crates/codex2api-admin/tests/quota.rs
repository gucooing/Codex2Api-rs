mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::json;
#[tokio::test]
async fn supplier_list_keeps_official_windows_and_persisted_errors_independent() {
    use codex2api_storage::{SupplierStatus, SupplierTokens};
    let f = Fixture::new().await;
    let first = f.state.accounts.create_pending().await.unwrap().account;
    let other = f.state.accounts.create_pending().await.unwrap().account;
    f.storage
        .set_account_status(&first.id, SupplierStatus::Active)
        .await
        .unwrap();
    f.storage
        .upsert_supplier_tokens(SupplierTokens {
            account_id: first.id.clone(),
            access_token: Some("fixture".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let observed = chrono::Utc::now();
    f.storage.store_account_quota(&first.id,&codex2api_storage::QuotaSnapshot {
        observed_at:observed,
        value:json!({"rate_limit":{"primary_window":{"used_percent":18,"limit_window_seconds":604800,"reset_after_seconds":3600},"secondary_window":{"used_percent":42,"limit_window_seconds":18000,"reset_at":2000000000}}})
    }).await.unwrap();
    for _ in 0..2 {
        let row = f
            .get(&format!("/admin/api/suppliers/{}/quota", first.id))
            .await;
        assert_eq!(row["status"], "active");
        assert_eq!(row["quota"]["windows"][0]["used_percent"], 18.0);
        assert_eq!(row["quota"]["windows"][0]["limit_window_seconds"], 604800);
        assert_eq!(row["quota"]["windows"][1]["used_percent"], 42.0);
        assert_eq!(row["quota"]["windows"][1]["limit_window_seconds"], 18000);
        assert_eq!(
            row["quota"]["windows"][0]["reset_at"],
            observed.timestamp() + 3600
        );
        assert_eq!(row["quota"]["observed_at"], json!(observed));
    }
    // Missing auth would make an official request fail: repeated reads used the cache.
    f.storage
        .record_supplier_error(&first.id, "ChatGPT 官方通信失败（HTTP 503）")
        .await
        .unwrap();
    let revision = f.storage.supplier_health(&first.id).await.unwrap().revision;
    f.storage
        .record_supplier_error(&first.id, "ChatGPT 官方连接失败")
        .await
        .unwrap();
    assert!(
        !f.storage
            .recover_supplier(&first.id, revision)
            .await
            .unwrap()
    );
    let rows = f.get("/admin/api/suppliers").await;
    let row = rows["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == first.id)
        .unwrap();
    assert_eq!(row["status"], "error");
    assert_eq!(row["quota"]["windows"][0]["used_percent"], 18.0);
    assert!(
        f.storage
            .supplier_health(&other.id)
            .await
            .unwrap()
            .error_message
            .is_none()
    );
    assert_eq!(
        f.request(
            "POST",
            &format!("/admin/api/suppliers/{}/status", first.id),
            json!({"enabled":true})
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    f.storage
        .set_account_status(&first.id, SupplierStatus::Disabled)
        .await
        .unwrap();
    assert_eq!(
        f.get(&format!("/admin/api/suppliers/{}", first.id)).await["status"],
        "disabled"
    );
    let latest = f.storage.supplier_health(&first.id).await.unwrap().revision;
    assert!(f.storage.recover_supplier(&first.id, latest).await.unwrap());
    assert_eq!(
        f.storage.require_account(&first.id).await.unwrap().status,
        SupplierStatus::Disabled
    );
    assert!(
        f.storage
            .get_account_quota(&first.id)
            .await
            .unwrap()
            .is_some()
    );
}
#[tokio::test]
async fn supplier_quota_preserves_monthly_and_other_durations_without_absent_windows() {
    let f = Fixture::new().await;
    let account = f.state.accounts.create_pending().await.unwrap().account;
    let observed = chrono::Utc::now();
    for (limits, expected) in [
        (
            json!({"primary_window":{"used_percent":0,"limit_window_seconds":2592000,"reset_after_seconds":2590527},"secondary_window":null}),
            json!([{"id":"primary_window","used_percent":0.0,"limit_window_seconds":2592000,"reset_at":observed.timestamp()+2590527}]),
        ),
        (
            json!({"primary_window":{"used_percent":12,"limit_window_seconds":86400,"reset_at":2000000000},"secondary_window":{"limit_window_seconds":5400}}),
            json!([{"id":"primary_window","used_percent":12.0,"limit_window_seconds":86400,"reset_at":2000000000},{"id":"secondary_window","used_percent":null,"limit_window_seconds":5400,"reset_at":null}]),
        ),
        (
            json!({"primary_window":null,"secondary_window":{"used_percent":23}}),
            json!([{"id":"secondary_window","used_percent":23.0,"limit_window_seconds":null,"reset_at":null}]),
        ),
        (
            json!({"primary_window":null,"secondary_window":null}),
            json!([]),
        ),
    ] {
        f.storage
            .store_account_quota(
                &account.id,
                &codex2api_storage::QuotaSnapshot {
                    observed_at: observed,
                    value: json!({"rate_limit":limits}),
                },
            )
            .await
            .unwrap();
        let supplier = f.get(&format!("/admin/api/suppliers/{}", account.id)).await;
        let detail = f
            .get(&format!(
                "/admin/api/suppliers/{}/official?section=quota",
                account.id
            ))
            .await;
        let mut windows = supplier["quota"]["windows"].clone();
        for window in windows.as_array_mut().unwrap() {
            let usage = window
                .as_object_mut()
                .unwrap()
                .remove("local_usage")
                .unwrap();
            if window["reset_at"].is_number() && window["limit_window_seconds"].is_number() {
                assert_eq!(usage["request_count"], 0);
                assert_eq!(usage["cost_nano_usd"], 0);
                assert_eq!(usage["tokens"], 0);
            } else {
                assert!(usage.is_null());
            }
        }
        assert_eq!(windows, expected);
        assert_eq!(detail["quota"], supplier["quota"]);
        assert_eq!(detail["value"]["rate_limit"], limits);
    }
}

#[tokio::test]
async fn supplier_cycles_sum_settled_prices_with_exact_boundaries_and_account_isolation() {
    use codex2api_storage::{ModelPrice, QuotaSnapshot, UsageRecord};
    let f = Fixture::new().await;
    let account = f.state.accounts.create_pending().await.unwrap().account;
    let other = f.state.accounts.create_pending().await.unwrap().account;
    let observed = chrono::Utc::now();
    let until = (observed.timestamp() + 3600) * 1000;
    let inner_from = until - 18_000_000;
    let outer_from = until - 2_592_000_000;
    let mut price = ModelPrice {
        provider_id: "chatgpt".into(),
        model: "cycle-test".into(),
        tier: "standard".into(),
        min_input_tokens: 0,
        input_rate: 2_000_000,
        cached_rate: 500_000,
        cache_write_rate: 3_000_000,
        output_rate: 4_000_000,
        source: "custom".into(),
        revision: 0,
    };
    assert!(f.storage.save_model_price(&price).await.unwrap());
    let snapshot = QuotaSnapshot {
        observed_at: observed,
        value: json!({"rate_limit": {
            "primary_window": {"limit_window_seconds":18000,"reset_at":until/1000,"used_percent":10},
            "secondary_window": {"limit_window_seconds":2592000,"reset_at":until/1000,"used_percent":20}
        }}),
    };
    f.storage
        .store_account_quota(&account.id, &snapshot)
        .await
        .unwrap();
    for (id, account_id, time) in [
        ("before", &account.id, outer_from - 1),
        ("outer-start", &account.id, outer_from),
        ("before-inner", &account.id, inner_from - 1),
        ("inner-start", &account.id, inner_from),
        ("last", &account.id, until - 1),
        ("next", &account.id, until),
        ("other", &other.id, inner_from),
    ] {
        let mut record = UsageRecord {
            id: id.into(),
            account_id: account_id.clone(),
            subject_id: format!("consumer-{id}"),
            endpoint: "/v1/responses".into(),
            model: Some(price.model.clone()),
            requested_at_ms: time,
            status: "in_progress".into(),
            ..Default::default()
        };
        f.storage.insert_usage(&record).await.unwrap();
        record.status = if id == "last" {
            "client_stopped"
        } else {
            "completed"
        }
        .into();
        record.input_tokens = Some(1_000_000);
        record.cached_tokens = Some(250_000);
        record.cache_write_tokens = Some(100_000);
        record.output_tokens = Some(200_000);
        record.reasoning_tokens = Some(50_000);
        f.storage.finish_usage(&record).await.unwrap();
        f.storage.finish_usage(&record).await.unwrap();
    }
    // Prices now change, but all displayed amounts must retain their settled values.
    price.revision = 1;
    price.input_rate *= 10;
    assert!(f.storage.save_model_price(&price).await.unwrap());
    let path = format!("/admin/api/suppliers/{}", account.id);
    let detail = f.get(&path).await;
    let windows = &detail["quota"]["windows"];
    assert_eq!(
        windows[0]["local_usage"],
        json!({
            "from_ms":inner_from,"until_ms":until,"request_count":2,
            "cost_nano_usd":5_050_000_000_i64,"tokens":2_400_000,
            "unpriced_requests":0,"missing_token_requests":0
        })
    );
    assert_eq!(
        windows[1]["local_usage"]["cost_nano_usd"],
        10_100_000_000_i64
    );
    assert_eq!(windows[1]["local_usage"]["tokens"], 4_800_000);
    for suffix in ["/quota", "/official?section=quota"] {
        assert_eq!(
            f.get(&format!("{path}{suffix}")).await["quota"],
            detail["quota"]
        );
    }
    let list = f.get("/admin/api/suppliers").await;
    let listed = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == account.id)
        .unwrap();
    assert_eq!(listed["quota"], detail["quota"]);
    // Missing prices/usage are visible alongside known subtotals, never free requests.
    for (id, input, output) in [
        ("unpriced", Some(100), Some(20)),
        ("missing", Some(50), None),
        ("pending", None, None),
    ] {
        let mut record = UsageRecord {
            id: id.into(),
            account_id: account.id.clone(),
            endpoint: "/v1/responses".into(),
            model: Some("unpriced-cycle-model".into()),
            requested_at_ms: inner_from + if id == "pending" { 2 } else { 1 },
            status: "in_progress".into(),
            ..Default::default()
        };
        f.storage.insert_usage(&record).await.unwrap();
        if id != "pending" {
            record.input_tokens = input;
            record.output_tokens = output;
            record.status = "failed".into();
            f.storage.finish_usage(&record).await.unwrap();
        }
    }
    let mixed = f.get(&path).await;
    let usage = &mixed["quota"]["windows"][0]["local_usage"];
    assert_eq!(usage["cost_nano_usd"], 5_050_000_000_i64);
    assert_eq!(usage["tokens"], 2_400_170);
    assert_eq!(usage["unpriced_requests"], 3);
    assert_eq!(usage["missing_token_requests"], 2);
    let unknown = f
        .storage
        .supplier_cycle_usage(&account.id, inner_from + 1, inner_from + 2)
        .await
        .unwrap();
    assert_eq!(unknown.cost_nano_usd, None);
    assert_eq!(unknown.tokens, Some(170));
    let pending = f
        .storage
        .supplier_cycle_usage(&account.id, inner_from + 2, inner_from + 3)
        .await
        .unwrap();
    assert_eq!(pending.cost_nano_usd, None);
    assert_eq!(pending.tokens, None);
    // A new official cycle picks up only the request at the former reset boundary.
    let mut next = snapshot;
    next.value["rate_limit"]["primary_window"]["reset_at"] = json!(until / 1000 + 18000);
    f.storage
        .store_account_quota(&account.id, &next)
        .await
        .unwrap();
    let renewed = f.get(&path).await;
    assert_eq!(
        renewed["quota"]["windows"][0]["local_usage"]["request_count"],
        1
    );
    assert_eq!(
        renewed["quota"]["windows"][0]["local_usage"]["cost_nano_usd"],
        2_525_000_000_i64
    );
    // An expired cached window retains its actual boundaries and totals.
    next.observed_at = observed - chrono::TimeDelta::days(40);
    f.storage
        .store_account_quota(&account.id, &next)
        .await
        .unwrap();
    let stale = f.get(&path).await;
    assert_eq!(stale["quota"]["stale"], true);
    assert_eq!(stale["quota"]["windows"], renewed["quota"]["windows"]);
}

#[tokio::test]
async fn quota_reads_persisted_official_data_and_keeps_it_after_refresh_failure() {
    let f = Fixture::new().await;
    let a = f.state.accounts.create_pending().await.unwrap().account;
    let snapshot = codex2api_storage::QuotaSnapshot {
        value: json!({"rate_limit":{"primary_window":{"used_percent":58,"limit_window_seconds":604800,"reset_after_seconds":3600}}}),
        observed_at: chrono::Utc::now() - chrono::TimeDelta::minutes(5),
    };
    f.storage
        .store_account_quota(&a.id, &snapshot)
        .await
        .unwrap();
    let path = format!("/admin/api/suppliers/{}/official?section=quota", a.id);
    let r = f.get(&path).await;
    assert_eq!(r["value"], snapshot.value);
    assert!(r["observed_at"].is_string());
    let r = f
        .request("GET", &format!("{path}&refresh=true"), json!(null))
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    let value = body(r).await;
    assert!(value["refresh_error"].as_str().unwrap().contains("授权"));
    assert_eq!(
        f.storage.get_account_quota(&a.id).await.unwrap(),
        Some(snapshot)
    );
}
