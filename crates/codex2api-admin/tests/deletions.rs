mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::json;
#[tokio::test]
async fn deleting_supply_or_proxy_preserves_consumers_and_usage_history() {
    let f = Fixture::new().await;
    let a = f.state.accounts.create_pending().await.unwrap().account;
    let p = f
        .storage
        .create_outbound_proxy("bound", "http://127.0.0.1:1080")
        .await
        .unwrap();
    f.storage
        .set_account_proxy(&a.id, Some(&p.id))
        .await
        .unwrap();
    let c = f.consumer("history-owner").await;
    let id = c["id"].as_str().unwrap();
    f.storage
        .insert_usage(&codex2api_storage::UsageRecord {
            id: "history".into(),
            account_id: a.id.clone(),
            account_name: "supply history".into(),
            subject_id: id.into(),
            subject_name: "consumer history".into(),
            status: "completed".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    f.storage
        .store_account_quota(
            &a.id,
            &codex2api_storage::QuotaSnapshot {
                value: json!({}),
                observed_at: chrono::Utc::now(),
            },
        )
        .await
        .unwrap();
    f.storage
        .insert_oauth_pending(
            "pending-delete",
            "verifier",
            "http://localhost/callback",
            Some(&a.id),
            std::time::Duration::from_secs(60),
        )
        .await
        .unwrap();
    let path = format!("/admin/api/proxies/{}", p.id);
    let r = f
        .request("DELETE", &path, json!({"confirm_unbind":false}))
        .await;
    assert_eq!(r.status(), StatusCode::CONFLICT);
    assert_eq!(body(r).await["account_count"], 1);
    assert_eq!(
        f.request("DELETE", &path, json!({"confirm_unbind":true}))
            .await
            .status(),
        StatusCode::OK
    );
    assert!(
        f.storage
            .require_account(&a.id)
            .await
            .unwrap()
            .proxy_id
            .is_none()
    );
    assert_eq!(
        f.request(
            "DELETE",
            &format!("/admin/api/suppliers/{}", a.id),
            json!({})
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert!(f.storage.get_account(&a.id).await.unwrap().is_none());
    assert!(f.storage.get_account_quota(&a.id).await.unwrap().is_none());
    assert!(
        f.storage
            .get_oauth_pending("pending-delete")
            .await
            .unwrap()
            .is_none()
    );
    assert!(f.storage.virtual_account(id).await.unwrap().is_some());
    let history = f.storage.query_usage(&Default::default()).await.unwrap();
    assert_eq!(history.total, 1);
    assert_eq!(history.records[0].account_name, "supply history");
    let reopened = codex2api_storage::Storage::open(f.dir.path().join("test.sqlite"))
        .await
        .unwrap();
    assert_eq!(
        reopened
            .query_usage(&Default::default())
            .await
            .unwrap()
            .total,
        1
    );
}
