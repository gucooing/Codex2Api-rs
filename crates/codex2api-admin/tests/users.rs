mod account_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../codex2api-storage/test-support/accounts.rs"
    ));
}
use account_fixture::AccountFixture;
mod common;
use common::*;
use serde_json::json;

#[tokio::test]
async fn wallet_ledger_lists_all_users_and_filters_owner_source_and_time() {
    let f = Fixture::new().await;
    let mut ids = Vec::new();
    for name in ["ledger-alice", "ledger-bob"] {
        let user=body(f.request("POST","/admin/api/users",json!({"username":name,"password":"password","name":name,"email":format!("{name}@example.test"),"enabled":true,"revision":null})).await).await;
        let id = user["id"].as_str().unwrap().to_owned();
        let result=f.request("POST",&format!("/admin/api/users/{id}/wallet-adjustments"),json!({"request_id":format!("credit-{name}"),"amount_cents":2000,"expected_revision":user["revision"]})).await;
        assert_eq!(result.status(), axum::http::StatusCode::OK);
        ids.push(id);
    }
    let mut plan = f.storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["sale_price_usd"] = json!("5");
    f.storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    let plan = f.storage.virtual_plan("plus").await.unwrap().unwrap();
    let preview = f
        .storage
        .checkout_preview(
            &ids[0],
            codex2api_storage::CheckoutInput {
                subscription_id: None,
                plan_id: plan.id,
                plan_revision: plan.revision,
                subscription_revision: None,
                payment_method: "wallet".into(),
                coupon_code: String::new(),
            },
        )
        .await
        .unwrap();
    let order = f
        .storage
        .create_subscription_order(codex2api_storage::OrderRequest {
            user_id: &ids[0],
            preview_token: preview["preview_token"].as_str().unwrap(),
        })
        .await
        .unwrap();
    f.storage
        .pay_subscription_order(&ids[0], &order.id)
        .await
        .unwrap();
    let all = f.get("/admin/api/wallet-entries").await;
    assert_eq!(all["total"], 3);
    assert_eq!(all["items"][0]["order_id"], order.id);
    let filtered = f
        .get(&format!(
            "/admin/api/wallet-entries?user_id={}&kind=system_adjustment",
            ids[0]
        ))
        .await;
    assert_eq!(filtered["total"], 1);
    assert_eq!(filtered["items"][0]["username"], "ledger-alice");
    assert_eq!(filtered["items"][0]["balance_before_cents"], 0);
    assert_eq!(filtered["items"][0]["balance_cents"], 2000);
    assert!(filtered["items"][0]["reason"].is_null());
    assert_eq!(
        f.get("/admin/api/wallet-entries?until_ms=0").await["total"],
        0
    );
    assert_eq!(
        f.get("/admin/api/wallet-entries?limit=10&page=2").await["items"],
        json!([])
    );
    assert_eq!(
        f.with_auth("GET", "/admin/api/wallet-entries", json!(null), None, None)
            .await
            .status(),
        axum::http::StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.request(
            "GET",
            "/admin/api/wallet-entries?from_ms=100&until_ms=99",
            json!(null)
        )
        .await
        .status(),
        axum::http::StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn overview_counts_all_subscription_health_and_distinct_active_users() {
    let f = Fixture::new().await;
    let healthy = f.consumer("healthy").await;
    let disabled = f.consumer("disabled").await;
    let expired = f.consumer("expired").await;
    sqlx::query("UPDATE platform_accounts SET enabled=FALSE WHERE id=$1")
        .bind(disabled["id"].as_str().unwrap())
        .execute(f.storage.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE platform_accounts SET subscription_expires_at=$1 WHERE id=$2")
        .bind((chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339())
        .bind(expired["id"].as_str().unwrap())
        .execute(f.storage.pool())
        .await
        .unwrap();
    let mut owners = Vec::new();
    for name in ["active-user", "idle-user"] {
        let user=body(f.request("POST","/admin/api/users",json!({"username":name,"password":"password","name":name,"email":format!("{name}@example.test"),"enabled":true,"revision":null})).await).await;
        owners.push(user["id"].as_str().unwrap().to_owned());
    }
    let active = f
        .storage
        .grant_subscription_fixture(&owners[0], "plus")
        .await
        .unwrap();
    let idle = f
        .storage
        .grant_subscription_fixture(&owners[1], "plus")
        .await
        .unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    for (id, subject, at) in [
        ("active-a", active.id.as_str(), now),
        ("active-b", active.id.as_str(), now),
        ("yesterday", idle.id.as_str(), now - 86400000),
        ("standalone", healthy["id"].as_str().unwrap(), now),
    ] {
        f.storage
            .insert_usage(&codex2api_storage::UsageRecord {
                id: id.into(),
                subject_id: subject.into(),
                requested_at_ms: at,
                status: "in_progress".into(),
                ..Default::default()
            })
            .await
            .unwrap();
    }
    let overview = f.get("/admin/api/overview?tz_offset=-480").await;
    assert_eq!(overview["consumer_count"], 5);
    assert_eq!(overview["normal_consumer_count"], 3);
    assert_eq!(overview["user_count"], 5);
    assert_eq!(overview["active_user_count"], 2);
    let records = f
        .get(&format!("/admin/api/usage?user_id={}", owners[0]))
        .await;
    assert_eq!(records["total"], 2);
    assert!(
        records["records"]
            .as_array()
            .unwrap()
            .iter()
            .all(|record| record["subject_id"] == active.id)
    );
}

#[tokio::test]
async fn administrator_wallet_route_requires_csrf_and_records_each_change_once() {
    let f = Fixture::new().await;
    let user=body(f.request("POST","/admin/api/users",json!({"username":"wallet-user","password":"password","name":"Wallet User","email":"wallet@example.test","enabled":true,"revision":null})).await).await;
    let path = format!(
        "/admin/api/users/{}/wallet-adjustments",
        user["id"].as_str().unwrap()
    );
    let input = json!({"request_id":"wallet-adjustment","amount_cents":1234,"expected_revision":user["revision"],"reason":"System correction"});
    assert_eq!(
        f.with_auth("POST", &path, input.clone(), Some(&f.cookie), None)
            .await
            .status(),
        axum::http::StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.request("POST", &path, input.clone()).await.status(),
        axum::http::StatusCode::OK
    );
    assert_eq!(
        f.request("POST", &path, input).await.status(),
        axum::http::StatusCode::OK
    );
    let detail = f
        .get(&format!(
            "/admin/api/users/{}",
            user["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(detail["user"]["wallet_balance_usd"], "12.34");
    let entries = f
        .get(&format!(
            "/admin/api/users/{}/wallet-entries",
            user["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(entries["total"], 1);
    assert_eq!(entries["items"].as_array().unwrap().len(), 1);
    assert_eq!(entries["items"][0]["kind"], "system_adjustment");
    let decrease = json!({"request_id":"wallet-decrease","amount_cents":-234,"expected_revision":user["revision"]});
    let response = f.request("POST", &path, decrease.clone()).await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let entry = body(response).await;
    assert_eq!(entry["entry"]["balance_before_cents"], 1234);
    assert_eq!(entry["entry"]["balance_cents"], 1000);
    assert_eq!(body(f.request("POST", &path, decrease).await).await, entry);
    let detail = f
        .get(&format!(
            "/admin/api/users/{}",
            user["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(detail["user"]["wallet_balance_usd"], "10");
    assert_eq!(detail["user"]["revision"], user["revision"]);
    let choices = f.get("/admin/api/users/options?search=WALLET").await;
    assert_eq!(choices["items"][0]["id"], user["id"]);
    assert!(choices["items"][0].get("password_hash").is_none());
    assert_eq!(
        f.get(&format!(
            "/admin/api/subscriptions?user_id={}&plan_id=plus",
            user["id"].as_str().unwrap()
        ))
        .await["items"],
        json!([])
    );
}

#[tokio::test]
async fn closing_purchases_does_not_restrict_administrator_assignments() {
    let f = Fixture::new().await;
    let mut plan = f.storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.allow_purchase = false;
    f.storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    assert_eq!(f.consumer("closed-plan-consumer").await["plan_id"], "plus");
    let user=body(f.request("POST","/admin/api/users",json!({"username":"closed-plan-user","password":"password","name":"User","email":"u@example.test","enabled":true,"revision":null})).await).await;
    assert_eq!(f.request("POST","/admin/api/subscriptions",json!({"user_id":user["id"],"plan_id":"plus","expires_at":"2099-01-01T00:00:00Z","reissue":false,"enabled":true,"revision":null})).await.status(),axum::http::StatusCode::OK);
    let plans = f.get("/admin/api/plans").await;
    let plus = plans["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "plus")
        .unwrap();
    assert_eq!(plus["allow_purchase"], false);
    assert!(plus.get("enabled").is_none());
    assert_eq!(f.get("/admin/api/orders").await["total"], 0);
}

#[tokio::test]
async fn new_users_have_no_subscription_and_expiration_has_no_fallback() {
    let f = Fixture::new().await;
    let created=f.request("POST","/admin/api/users",json!({"username":"customer","password":"customer-password","name":"Customer","email":"customer@example.test","enabled":true,"revision":null})).await;
    assert_eq!(created.status(), axum::http::StatusCode::OK);
    let user = body(created).await;
    let owner = user["id"].as_str().unwrap();
    assert_eq!(
        f.get(&format!("/admin/api/subscriptions?user_id={owner}"))
            .await["total"],
        0
    );
    let response=f.request("POST","/admin/api/subscriptions",json!({"user_id":owner,"plan_id":"plus","expires_at":"2020-01-01T00:00:00Z","enabled":true,"reissue":false,"revision":null})).await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let sub = body(response).await;
    let id = sub["id"].as_str().unwrap();
    assert_eq!(sub["subscription_status"], "expired");
    assert_eq!(sub["plan_id"], "plus");
    assert!(
        !f.storage
            .effective_entitlements(id)
            .await
            .unwrap()
            .execution_enabled
    );
    assert_eq!(
        f.get(&format!(
            "/admin/api/subscriptions?user_id={owner}&subscription=expired"
        ))
        .await["total"],
        1
    );
    assert_eq!(f.request("PUT",&format!("/admin/api/users/{owner}"),json!({"username":"customer","password":"","name":"Customer","email":"customer@example.test","enabled":true,"revision":user["revision"],"wallet_balance_usd":"999"})).await.status(),axum::http::StatusCode::UNPROCESSABLE_ENTITY);
}
#[tokio::test]
async fn no_default_free_catalog_or_removed_free_access_settings_are_accepted() {
    let f = Fixture::new().await;
    assert!(f.storage.virtual_plan("free").await.unwrap().is_none());
    assert!(
        f.get("/admin/api/plans").await["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["plan_type"] != "free")
    );
    let mut input = plan_input();
    input["plan_type"] = "free".into();
    assert_eq!(
        f.request("POST", "/admin/api/plans", input).await.status(),
        axum::http::StatusCode::BAD_REQUEST
    );
}
#[tokio::test]
async fn user_and_subscription_id_scopes_are_distinct_and_the_removed_namespace_is_absent() {
    use axum::http::StatusCode;
    let f = Fixture::new().await;
    let account = f.consumer("scoped-owner").await;
    let user = account["user_id"].as_str().unwrap();
    let subscription = account["id"].as_str().unwrap();
    for suffix in [
        "", "/configs", "/devices", "/routing", "/usage", "/logs", "/records",
    ] {
        assert_eq!(
            f.request(
                "GET",
                &format!("/admin/api/subscriptions/{user}{suffix}"),
                json!(null)
            )
            .await
            .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            f.request(
                "GET",
                &format!("/admin/api/subscriptions/{subscription}{suffix}"),
                json!(null)
            )
            .await
            .status(),
            StatusCode::OK
        );
        assert_eq!(
            f.request(
                "GET",
                &format!("/admin/api/consumers/{subscription}{suffix}"),
                json!(null)
            )
            .await
            .status(),
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        f.request(
            "GET",
            &format!("/admin/api/users/{subscription}"),
            json!(null)
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let other=f.request("POST","/admin/api/subscriptions",json!({"user_id":user,"plan_id":"plus","expires_at":null,"enabled":true,"reissue":false,"revision":null})).await;
    assert_eq!(other.status(), StatusCode::OK);
    assert_ne!(body(other).await["id"], account["id"]);
    assert_eq!(
        f.get(&format!("/admin/api/subscriptions?user_id={user}"))
            .await["total"],
        2
    );
    assert_eq!(
        f.request(
            "PUT",
            &format!("/admin/api/subscriptions/{subscription}/rate-limit"),
            json!({"rpm":12,"concurrency":3})
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        f.get(&format!(
            "/admin/api/subscriptions/{subscription}/rate-limit"
        ))
        .await["effective_rpm"],
        12
    );
}
