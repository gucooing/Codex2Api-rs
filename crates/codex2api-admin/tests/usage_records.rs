mod common;

#[tokio::test]
async fn log_and_diagnostic_pages_report_totals_and_jump_without_crossing_accounts() {
    let f = common::Fixture::new().await;
    let a = f.consumer("paged-logs").await;
    let b = f.consumer("other-logs").await;
    let owner = a["id"].as_str().unwrap();
    for index in 0..105 {
        f.storage
            .record_virtual_request(
                owner,
                "fixture",
                "GET",
                &format!("/page/{index:03}"),
                if index < 55 { 200 } else { 500 },
                1,
            )
            .await
            .unwrap();
        f.storage
            .record_desktop_diagnostic(
                &format!("page-{index}"),
                "fixture",
                Some(owner),
                1,
                &serde_json::json!([]),
            )
            .await
            .unwrap();
    }
    f.storage
        .record_virtual_request(
            b["id"].as_str().unwrap(),
            "fixture",
            "GET",
            "/other-only",
            200,
            1,
        )
        .await
        .unwrap();
    for page in [1, 3, 2, 999] {
        let actual = page.min(3);
        let logs = f
            .get(&format!(
                "/admin/api/consumers/{owner}/logs?page={page}&page_size=50"
            ))
            .await;
        let diagnostics = f
            .get(&format!("/admin/api/diagnostics?page={page}&page_size=50"))
            .await;
        for value in [&logs, &diagnostics] {
            assert_eq!(value["total"], 105);
            assert_eq!(value["page"], actual);
            assert_eq!(value["page_size"], 50);
            assert_eq!(
                value["items"].as_array().unwrap().len(),
                if actual == 3 { 5 } else { 50 }
            );
        }
        assert_eq!(
            logs["items"][0]["path"],
            format!("/page/{:03}", 104 - (actual - 1) * 50)
        );
        assert!(!logs.to_string().contains("other-only"));
    }
    for size in [10, 20, 30, 50] {
        for path in [
            format!("/admin/api/consumers/{owner}/logs"),
            "/admin/api/diagnostics".into(),
        ] {
            let page = f.get(&format!("{path}?page=2&page_size={size}")).await;
            assert_eq!(page["page_size"], size);
            assert_eq!(page["items"].as_array().unwrap().len(), size as usize);
        }
    }
    for path in [
        format!("/admin/api/consumers/{owner}/logs"),
        "/admin/api/diagnostics".into(),
        "/admin/api/usage".into(),
    ] {
        let default = f.get(&path).await;
        assert_eq!(default["page_size"], 20);
        for size in [0, 1, 25, 100] {
            assert_eq!(
                f.request(
                    "GET",
                    &format!("{path}?page_size={size}"),
                    serde_json::Value::Null
                )
                .await
                .status(),
                axum::http::StatusCode::BAD_REQUEST
            );
        }
    }
    let filtered = f
        .get(&format!(
            "/admin/api/consumers/{owner}/logs?page=2&page_size=50&result=success&method=GET&path=/page/"
        ))
        .await;
    assert_eq!(filtered["total"], 55);
    assert_eq!(filtered["items"].as_array().unwrap().len(), 5);
    let empty = f
        .get(&format!(
            "/admin/api/consumers/{owner}/logs?page=3&path=missing"
        ))
        .await;
    assert_eq!(empty["total"], 0);
    assert_eq!(empty["page"], 1);
    assert!(empty["items"].as_array().unwrap().is_empty());
}
use axum::http::StatusCode;
use common::*;
use serde_json::json;
#[tokio::test]
async fn usage_json_filters_history_and_retains_reasoning_billing_and_unknowns() {
    let f = Fixture::new().await;
    for (i, model, status) in [(0, "first", "completed"), (1, "second", "failed")] {
        f.storage
            .insert_usage(&codex2api_storage::UsageRecord {
                id: format!("usage-{i}"),
                account_id: "supplier".into(),
                account_name: "Supplier".into(),
                subject_id: "consumer".into(),
                subject_name: "Consumer".into(),
                model: Some(model.into()),
                actual_model: Some(format!("returned-{model}")),
                upstream_request_id: Some(format!("req-{model}")),
                error_code: (status == "failed").then(|| "rate_limit_exceeded".into()),
                error_message: (status == "failed").then(|| "Too many requests".into()),
                reasoning_effort: Some("high".into()),
                reasoning_tokens: Some(10),
                input_tokens: Some(100),
                output_tokens: Some(20),
                requested_at_ms: 1_789_516_800_000,
                status: status.into(),
                ..Default::default()
            })
            .await
            .unwrap();
    }
    let all = f.get("/admin/api/usage").await;
    assert_eq!(all["total"], 2);
    assert_eq!(all["page_size"], 20);
    assert!(all.get("consumers").is_none());
    assert!(all.get("suppliers").is_none());
    assert_eq!(all["records"][0]["reasoning_effort"], "high");
    assert!(all["records"][0]["cost_nano_usd"].is_null());
    let filtered = f
        .get(
            "/admin/api/usage?model=second&status=failed&virtual_account=consumer&account=Supplier",
        )
        .await;
    assert_eq!(filtered["total"], 1);
    assert_eq!(filtered["records"][0]["model"], "second");
    assert_eq!(filtered["records"][0]["actual_model"], "returned-second");
    assert_eq!(filtered["records"][0]["upstream_request_id"], "req-second");
    assert_eq!(filtered["records"][0]["error_code"], "rate_limit_exceeded");
    assert_eq!(filtered["records"][0]["error_message"], "Too many requests");
    assert_eq!(
        f.get("/admin/api/usage?model=returned-second").await["total"],
        1
    );
    for path in [
        "/admin/api/usage?status=invalid",
        "/admin/api/usage?from=bad",
        "/admin/api/usage?from=2026-09-22T00:00&until=2026-09-21T00:00",
    ] {
        assert_eq!(
            f.request("GET", path, json!(null)).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn account_selectors_search_live_lists_and_filter_usage_by_exact_ids() {
    let f = Fixture::new().await;
    let mut suppliers = Vec::new();
    for index in 0..7 {
        let consumer = codex2api_storage::VirtualAccount {
            id: format!("consumer-{index}"),
            provider_id: "chatgpt".into(),
            username: format!("User-{index}"),
            name: "Same display name".into(),
            email: format!("mail-{index}@example.test"),
            password_hash: "fixture".into(),
            plan_id: "plus".into(),
            plan_type: "plus".into(),
            enabled: true,
            subscription_expires_at: None,
            created_at: "2026-09-22T00:00:00Z".into(),
        };
        f.storage.save_virtual_account(&consumer).await.unwrap();
        let supplier = f.state.accounts.create_pending().await.unwrap().account;
        f.storage
            .update_account(
                &supplier.id,
                codex2api_storage::SupplierAccountUpdate {
                    display_name: Some(format!("Supplier-{index}")),
                    email: Some(format!("upstream-{index}@example.test")),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        f.storage
            .insert_usage(&codex2api_storage::UsageRecord {
                id: format!("history-{index}"),
                account_id: supplier.id.clone(),
                account_name: "Same historic supplier name".into(),
                subject_id: consumer.id,
                subject_name: "Same historic consumer name".into(),
                status: "completed".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        suppliers.push(supplier.id);
    }
    for path in ["/admin/api/consumers", "/admin/api/suppliers"] {
        assert_eq!(f.get(path).await["items"].as_array().unwrap().len(), 7);
        assert_eq!(
            f.get(&format!("{path}?limit=5")).await["items"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
        assert_eq!(
            f.get(&format!("{path}?search=")).await["items"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
        assert!(
            f.get(&format!("{path}?search=history&limit=5")).await["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            f.get(&format!("{path}?search=%25_%27&limit=5")).await["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        for limit in ["0", "101", "-1"] {
            assert_eq!(
                f.request("GET", &format!("{path}?limit={limit}"), json!(null))
                    .await
                    .status(),
                StatusCode::BAD_REQUEST
            );
        }
    }
    for (path, search, id) in [
        ("consumers", "sEr-6", "consumer-6"),
        ("consumers", "AIL-6@EXAMPLE", "consumer-6"),
        ("suppliers", "PLIER-6", suppliers[6].as_str()),
        ("suppliers", "REAM-6@EXAMPLE", suppliers[6].as_str()),
    ] {
        let found = f
            .get(&format!("/admin/api/{path}?search={search}&limit=5"))
            .await;
        assert_eq!(found["items"].as_array().unwrap().len(), 1);
        assert_eq!(found["items"][0]["id"], id);
    }
    let filtered = f
        .get(&format!(
            "/admin/api/usage?supplier_id={}&virtual_account=consumer-6",
            suppliers[6]
        ))
        .await;
    assert_eq!(filtered["total"], 1);
    assert_eq!(filtered["records"][0]["id"], "history-6");
    assert_eq!(
        f.get("/admin/api/usage?supplier_id=Same%20historic%20supplier%20name")
            .await["total"],
        0
    );
    assert_eq!(
        f.get("/admin/api/usage?supplier_id=missing").await["total"],
        0
    );
    f.storage
        .delete_virtual_account("consumer-6")
        .await
        .unwrap();
    assert!(
        f.get("/admin/api/consumers?search=User-6&limit=5").await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        f.get("/admin/api/usage?virtual_account=consumer-6").await["total"],
        1
    );
    assert!(
        f.get("/admin/api/suppliers?provider_id=grok&limit=5").await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        f.get("/admin/api/suppliers?for_routing=true&limit=5").await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn failure_filter_includes_incomplete_and_interrupted_in_counts_and_pages() {
    let f = Fixture::new().await;
    for index in 0..58 {
        let status = match index {
            55 => "completed",
            56 => "in_progress",
            57 => "client_stopped",
            _ => ["failed", "incomplete", "interrupted"][index % 3],
        };
        let mut record = codex2api_storage::UsageRecord {
            id: format!("status-{index:02}"),
            subject_id: "consumer".into(),
            status: "in_progress".into(),
            requested_at_ms: index as i64,
            ..Default::default()
        };
        f.storage.insert_usage(&record).await.unwrap();
        if status != "in_progress" {
            record.status = status.into();
            record.http_status = Some(200);
            f.storage.finish_usage(&record).await.unwrap();
        }
    }
    let first = f
        .get("/admin/api/usage?page_size=50&status=failed&virtual_account=consumer")
        .await;
    let second = f
        .get("/admin/api/usage?page_size=50&status=failed&virtual_account=consumer&page=2")
        .await;
    assert_eq!(first["total"], 55);
    assert_eq!(second["total"], 55);
    assert_eq!(first["records"].as_array().unwrap().len(), 50);
    assert_eq!(second["records"].as_array().unwrap().len(), 5);
    let mut raw_states = std::collections::BTreeSet::new();
    for page in [&first, &second] {
        for record in page["records"].as_array().unwrap() {
            assert_eq!(record["http_status"], 200);
            raw_states.insert(record["status"].as_str().unwrap());
        }
    }
    assert_eq!(
        raw_states,
        ["failed", "incomplete", "interrupted"]
            .into_iter()
            .collect()
    );
    assert_eq!(f.get("/admin/api/usage?status=completed").await["total"], 2);
    assert_eq!(
        f.get("/admin/api/usage?status=in_progress").await["total"],
        1
    );
    assert_eq!(
        f.get("/admin/api/usage?status=failed&virtual_account=other")
            .await["total"],
        0
    );
}
