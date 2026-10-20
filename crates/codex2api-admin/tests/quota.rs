mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::json;
#[tokio::test]
async fn subscription_expiry_survives_list_detail_quota_refresh_and_state_reset() {
    use codex2api_storage::{QuotaSnapshot, SupplierTokens};
    let f = Fixture::new().await;
    let id = f.state.accounts.create_pending().await.unwrap().account.id;
    // Synthetic ID token: exp=2000000000, subscription_active_until=2026-10-17T08:00:00+08:00.
    let token = "e30.eyJleHAiOjIwMDAwMDAwMDAsImh0dHBzOi8vYXBpLm9wZW5haS5jb20vYXV0aCI6eyJjaGF0Z3B0X3N1YnNjcmlwdGlvbl9hY3RpdmVfdW50aWwiOiIyMDI2LTEwLTE3VDA4OjAwOjAwKzA4OjAwIn19.fixture";
    f.storage
        .upsert_supplier_tokens(SupplierTokens {
            account_id: id.clone(),
            id_token: Some(token.into()),
            ..Default::default()
        })
        .await
        .unwrap();
    f.storage
        .store_account_quota(
            &id,
            &QuotaSnapshot::new(json!({"rate_limit":{"allowed":true}}), chrono::Utc::now()),
        )
        .await
        .unwrap();
    let expiry = json!(
        chrono::DateTime::parse_from_rfc3339("2026-10-17T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    );
    assert_eq!(
        f.get("/admin/api/suppliers").await["items"][0]["subscription_expires_at"],
        expiry
    );
    for suffix in ["", "/quota"] {
        assert_eq!(
            f.get(&format!("/admin/api/suppliers/{id}{suffix}")).await["subscription_expires_at"],
            expiry
        );
    }
    let reset = body(
        f.request(
            "POST",
            &format!("/admin/api/suppliers/{id}/reset-state"),
            json!({}),
        )
        .await,
    )
    .await;
    assert_eq!(reset["subscription_expires_at"], expiry);
    // A credential expiry alone must never become a subscription expiry.
    f.storage
        .upsert_supplier_tokens(SupplierTokens {
            account_id: id.clone(),
            id_token: Some("e30.eyJleHAiOjIwMDAwMDAwMDB9.fixture".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        f.get(&format!("/admin/api/suppliers/{id}")).await["subscription_expires_at"].is_null()
    );
}

#[tokio::test]
async fn administrator_can_reset_quota_offline_without_bypassing_auth_or_manual_disablement() {
    use codex2api_storage::{SupplierStatus, SupplierTokens};
    let f = Fixture::new().await;
    let id = f.state.accounts.create_pending().await.unwrap().account.id;
    f.storage
        .upsert_supplier_tokens(SupplierTokens {
            account_id: id.clone(),
            access_token: Some("fixture".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    f.storage
        .set_account_status(&id, SupplierStatus::Active)
        .await
        .unwrap();
    let auth = f
        .storage
        .supplier_auth_revision(&id)
        .await
        .unwrap()
        .unwrap();
    let until = chrono::Utc::now().timestamp() + 86400;
    f.storage
        .block_supplier(&id, auth, &quota_block(until, "usage_limit_reached"))
        .await
        .unwrap();
    let path = format!("/admin/api/suppliers/{id}/reset-state");
    assert_eq!(
        f.with_auth("POST", &path, json!({}), None, None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.with_auth("POST", &path, json!({}), Some(&f.cookie), None)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.get(&format!("/admin/api/suppliers/{id}")).await["status"],
        "quota_exhausted"
    );
    let response = f.request("POST", &path, json!({})).await;
    assert_eq!(response.status(), StatusCode::OK);
    let reset = body(response).await;
    assert_eq!(reset["status"], "active");
    assert!(reset["block"].is_null());
    f.storage.reject_supplier_auth(&id, auth).await.unwrap();
    f.storage
        .set_account_status(&id, SupplierStatus::Disabled)
        .await
        .unwrap();
    let reset = body(f.request("POST", &path, json!({})).await).await;
    assert_eq!(reset["status"], "disabled");
    assert_eq!(reset["authentication_invalid"], true);
}
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
    let observed = chrono::SubsecRound::trunc_subsecs(chrono::Utc::now(), 6);
    f.storage.store_account_quota(&first.id,&codex2api_storage::QuotaSnapshot::new(json!({"rate_limit":{"primary_window":{"used_percent":18,"limit_window_seconds":604800,"reset_after_seconds":3600},"secondary_window":{"used_percent":42,"limit_window_seconds":18000,"reset_at":2000000000}}}), observed)).await.unwrap();
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
        .reject_supplier_auth(
            &first.id,
            f.storage
                .supplier_auth_revision(&first.id)
                .await
                .unwrap()
                .unwrap(),
        )
        .await
        .unwrap();
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
        StatusCode::OK
    );
    f.storage
        .set_account_status(&first.id, SupplierStatus::Disabled)
        .await
        .unwrap();
    assert_eq!(
        f.get(&format!("/admin/api/suppliers/{}", first.id)).await["status"],
        "disabled"
    );
    let old_revision = f
        .storage
        .supplier_auth_revision(&first.id)
        .await
        .unwrap()
        .unwrap();
    let tokens = f
        .storage
        .load_supplier_tokens(&first.id)
        .await
        .unwrap()
        .unwrap();
    f.storage.upsert_supplier_tokens(tokens).await.unwrap();
    f.storage
        .reject_supplier_auth(&first.id, old_revision)
        .await
        .unwrap();
    assert!(
        !f.storage
            .supplier_health(&first.id)
            .await
            .unwrap()
            .authentication_invalid()
    );
    assert_eq!(
        f.get(&format!("/admin/api/suppliers/{}", first.id)).await["enabled"],
        false
    );
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
    let enabled = f
        .request(
            "POST",
            &format!("/admin/api/suppliers/{}/status", first.id),
            json!({"enabled":true}),
        )
        .await;
    assert_eq!(enabled.status(), StatusCode::OK);
    let restored = f.get(&format!("/admin/api/suppliers/{}", first.id)).await;
    assert_eq!(restored["status"], "active");
    assert!(restored["error_message"].is_null());
}
#[tokio::test]
async fn supplier_quota_preserves_monthly_and_other_durations_without_absent_windows() {
    let f = Fixture::new().await;
    let account = f.state.accounts.create_pending().await.unwrap().account;
    let observed = chrono::SubsecRound::trunc_subsecs(chrono::Utc::now(), 6);
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
                &codex2api_storage::QuotaSnapshot::new(json!({"rate_limit":limits}), observed),
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
async fn quota_reads_persisted_official_data_and_keeps_it_after_refresh_failure() {
    let f = Fixture::new().await;
    let a = f.state.accounts.create_pending().await.unwrap().account;
    let snapshot = codex2api_storage::QuotaSnapshot::new(
        json!({"rate_limit":{"primary_window":{"used_percent":58,"limit_window_seconds":604800,"reset_after_seconds":3600}}}),
        chrono::Utc::now() - chrono::TimeDelta::minutes(5),
    );
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

fn quota_block(until: i64, code: &str) -> codex2api_storage::BlockRequest {
    codex2api_storage::BlockRequest {
        model: None,
        kind: codex2api_storage::BlockKind::Quota,
        until_at: Some(until),
        code: Some(code.into()),
        message: None,
    }
}
