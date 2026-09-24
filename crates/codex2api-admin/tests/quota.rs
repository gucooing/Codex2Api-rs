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
        assert_eq!(supplier["quota"]["windows"], expected);
        assert_eq!(detail["quota"], supplier["quota"]);
        assert_eq!(detail["value"]["rate_limit"], limits);
    }
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
