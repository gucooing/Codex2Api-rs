mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::{Value, json};

#[tokio::test]
async fn paged_selection_exclusions_atomic_failure_and_retry_preserve_exact_targets() {
    let f = Fixture::new().await;
    let account = f.consumer("seed").await;
    let seed = f
        .storage
        .virtual_account(account["id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    for index in 0..205 {
        let mut row = seed.clone();
        row.id = format!("batch-{index:03}");
        row.username = format!("match-{index:03}");
        row.name = "跨页账户".into();
        f.storage.save_virtual_account(&row).await.unwrap();
    }
    let page=f.get("/admin/api/consumers?page=2&page_size=10&search=match-&status=enabled&subscription=active").await;
    assert_eq!(page["total"], 205);
    assert_eq!(page["items"].as_array().unwrap().len(), 10);
    assert!(page["items"][0]["quota"]["windows"].is_array());
    let future = (chrono::Utc::now() + chrono::Duration::days(2)).to_rfc3339();
    let batch = json!({"request_id":"cross-page","operation":"grant_reset","all_matching":true,
        "excluded_ids":["batch-001"],"filters":{"search":"match-","status":"enabled","subscription":"active"},
        "quantity":1,"activate_at":future,"duration_days":30});
    // A storage error after the first account must roll back the entire operation.
    sqlx::query("CREATE TRIGGER fail_batch BEFORE INSERT ON virtual_reset_grants WHEN NEW.virtual_account_id='batch-002' BEGIN SELECT RAISE(ABORT,'fixture failure'); END")
        .execute(f.storage.pool()).await.unwrap();
    assert_eq!(
        f.request("POST", "/admin/api/consumers/batch", batch.clone())
            .await
            .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        f.storage
            .virtual_reset_credit_records("batch-000")
            .await
            .unwrap()["items"],
        json!([])
    );
    sqlx::query("DROP TRIGGER fail_batch")
        .execute(f.storage.pool())
        .await
        .unwrap();
    for _ in 0..2 {
        let r = f
            .request("POST", "/admin/api/consumers/batch", batch.clone())
            .await;
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(
            body(r).await,
            json!({"ok":true,"matched":204,"affected":204,"skipped":0})
        );
    }
    let record = f
        .storage
        .virtual_reset_credit_records("batch-204")
        .await
        .unwrap();
    assert_eq!(record["items"].as_array().unwrap().len(), 1);
    assert_eq!(record["items"][0]["status"], "pending");
    let start =
        chrono::DateTime::parse_from_rfc3339(record["items"][0]["available_at"].as_str().unwrap())
            .unwrap();
    let end =
        chrono::DateTime::parse_from_rfc3339(record["items"][0]["expires_at"].as_str().unwrap())
            .unwrap();
    assert_eq!((end - start).num_days(), 30);
    assert_eq!(
        f.storage
            .virtual_reset_credit_records("batch-001")
            .await
            .unwrap()["items"],
        json!([])
    );
    assert_eq!(
        f.storage.virtual_reset_credits("batch-204").await.unwrap()["credits"],
        json!([])
    );
    let mut changed = batch.clone();
    changed["duration_days"] = 7.into();
    assert_eq!(
        f.request("POST", "/admin/api/consumers/batch", changed)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let no_op = json!({"request_id":"noop","operation":"reset","ids":["batch-000","batch-000"]});
    assert_eq!(
        body(f.request("POST", "/admin/api/consumers/batch", no_op).await).await,
        json!({"ok":true,"matched":1,"affected":0,"skipped":1})
    );
    assert_eq!(
        f.storage
            .virtual_reset_credit_records("batch-000")
            .await
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for invalid in [
        json!({"request_id":"unknown","operation":"invalid","all_matching":true,"filters":{"search":"no-match"}}),
        json!({"request_id":"mixed","operation":"delete","all_matching":true,"ids":["batch-000"]}),
        json!({"request_id":"missing","operation":"delete","ids":["batch-000","missing"]}),
        json!({"request_id":"invalid-note","operation":"grant_reset","ids":["batch-000"],"quantity":1,"note":"x".repeat(257)}),
    ] {
        assert_eq!(
            f.request("POST", "/admin/api/consumers/batch", invalid)
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert!(
        f.storage
            .virtual_account("batch-000")
            .await
            .unwrap()
            .is_some()
    );
    let delete = json!({"request_id":"delete-selected","operation":"delete","ids":["batch-204"]});
    for _ in 0..2 {
        assert_eq!(
            body(
                f.request("POST", "/admin/api/consumers/batch", delete.clone())
                    .await
            )
            .await["affected"],
            1
        );
    }
    assert_eq!(
        f.with_auth(
            "POST",
            "/admin/api/consumers/batch",
            batch,
            Some(&f.cookie),
            None
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn single_grant_retries_validate_the_schedule_and_duration() {
    let f = Fixture::new().await;
    let account = f.consumer("grant-retry").await;
    let id = account["id"].as_str().unwrap();
    let path = format!("/admin/api/consumers/{id}/reset-credits");
    let input = json!({"request_id":"retry","quantity":1});
    for _ in 0..2 {
        assert_eq!(
            f.request("POST", &path, input.clone()).await.status(),
            StatusCode::OK
        );
    }
    let records = f.get(&path).await;
    let row = &records["items"][0];
    assert_eq!(records["items"].as_array().unwrap().len(), 1);
    let start =
        chrono::DateTime::parse_from_rfc3339(row["available_at"].as_str().unwrap()).unwrap();
    let end = chrono::DateTime::parse_from_rfc3339(row["expires_at"].as_str().unwrap()).unwrap();
    assert_eq!((end - start).num_days(), 30);
    for patch in [
        json!({"duration_days":7}),
        json!({"activate_at":"2030-01-01T00:00:00Z"}),
    ] {
        let mut changed = input.clone();
        changed
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert_eq!(
            f.request("POST", &path, changed).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn administrators_issue_and_inspect_account_owned_reset_cards_with_csrf() {
    let f = Fixture::new().await;
    let account = f.consumer("reset-owner").await;
    let other = f.consumer("reset-other").await;
    let id = account["id"].as_str().unwrap();
    let path = format!("/admin/api/consumers/{id}/reset-credits");
    let input = json!({"request_id":"grant-once","quantity":2,"note":"运营发放"});
    assert_eq!(
        f.with_auth("POST", &path, input.clone(), None, None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.with_auth("POST", &path, input.clone(), Some(&f.cookie), None)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    for _ in 0..2 {
        assert_eq!(
            f.request("POST", &path, input.clone()).await.status(),
            StatusCode::OK
        );
    }
    let records = f.get(&path).await;
    assert_eq!(records["items"].as_array().unwrap().len(), 2);
    assert_eq!(records["available_count"], 2);
    assert_eq!(records["items"][0]["note"], "运营发放");
    let foreign = f
        .get(&format!(
            "/admin/api/consumers/{}/reset-credits",
            other["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(foreign["items"], json!([]));
    let card = records["items"][0]["id"].as_str().unwrap();
    let result = f
        .request(
            "POST",
            &format!("{path}/consume"),
            json!({"redeem_request_id":"empty","credit_id":card}),
        )
        .await;
    assert_eq!(result.status(), StatusCode::OK);
    assert_eq!(body(result).await["code"], "nothing_to_reset");
    assert_eq!(f.get(&path).await["available_count"], 2);
    assert_eq!(
        f.get(&format!("/admin/api/consumers/{id}")).await["subscription_expires_at"],
        account["subscription_expires_at"]
    );
    assert_eq!(
        f.request("POST", &path, json!({"request_id":"bad","quantity":0}))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        f.request("POST", "/admin/api/consumers/missing/reset-credits", input)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.with_auth("GET", &path, Value::Null, None, None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn batch_operations_resolve_all_matching_accounts_in_sqlite_and_use_same_card_policy() {
    let f = Fixture::new().await;
    let first = f.consumer("batch-owner-a").await;
    let second = f.consumer("batch-owner-b").await;
    let response = f.request("POST", "/admin/api/consumers/batch", json!({
        "request_id":"grant-batch","operation":"grant_reset","all_matching":true,"filters":{"search":"batch-owner","status":"enabled","subscription":"active"},
        "quantity":2,"note":"批量发放","activate_at":null,"duration_days":30
    })).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body(response).await["affected"], 2);
    for account in [first, second] {
        let cards = f
            .get(&format!(
                "/admin/api/consumers/{}/reset-credits",
                account["id"].as_str().unwrap()
            ))
            .await;
        assert_eq!(cards["available_count"], 2);
    }
    let response = f.request("POST", "/admin/api/consumers/batch", json!({
        "request_id":"delete-batch","operation":"delete","all_matching":true,"filters":{"search":"batch-owner","status":"enabled","subscription":"active"}
    })).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body(response).await["affected"], 2);
    assert_eq!(
        f.get("/admin/api/consumers").await["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["username"]
                .as_str()
                .unwrap()
                .starts_with("batch-owner"))
            .count(),
        0
    );
}
