mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::{Value, json};

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
        "operation":"grant_reset","all_matching":true,"filters":{"search":"batch-owner","status":"enabled","subscription":"active"},
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
        "operation":"delete","all_matching":true,"filters":{"search":"batch-owner","status":"enabled","subscription":"active"}
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
