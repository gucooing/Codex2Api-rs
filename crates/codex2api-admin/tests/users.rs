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
    let subscription = f
        .storage
        .user_subscriptions(Some(&ids[0]), true)
        .await
        .unwrap()
        .remove(0);
    let preview = f
        .storage
        .checkout_preview(
            &ids[0],
            codex2api_storage::CheckoutInput {
                plan_id: plan.id,
                plan_revision: plan.revision,
                subscription_revision: Some(subscription.revision),
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
async fn overview_counts_standalone_health_and_distinct_users_with_usage_today() {
    let f = Fixture::new().await;
    let healthy = f.consumer("healthy").await;
    let disabled = f.consumer("disabled").await;
    let expired = f.consumer("expired").await;
    sqlx::query("UPDATE virtual_accounts SET enabled=0 WHERE id=?")
        .bind(disabled["id"].as_str().unwrap())
        .execute(f.storage.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE virtual_accounts SET subscription_expires_at=? WHERE id=?")
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
        .user_platform_account(&owners[0], "chatgpt")
        .await
        .unwrap()
        .unwrap();
    let idle = f
        .storage
        .user_platform_account(&owners[1], "chatgpt")
        .await
        .unwrap()
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
    assert_eq!(overview["consumer_count"], 3);
    assert_eq!(overview["normal_consumer_count"], 1);
    assert_eq!(overview["user_count"], 2);
    assert_eq!(overview["active_user_count"], 1);
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
    assert_eq!(detail["wallet_entries"].as_array().unwrap().len(), 1);
    assert_eq!(detail["wallet_entries"][0]["kind"], "system_adjustment");
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
    let subs = f.get("/admin/api/subscriptions").await;
    let sub = &subs["items"][0];
    assert_eq!(f.request("PUT",&format!("/admin/api/subscriptions/{}",sub["virtual_account_id"].as_str().unwrap()),json!({"user_id":user["id"],"plan_id":"plus","expires_at":"2099-01-01T00:00:00Z","reissue":false,"enabled":true,"revision":sub["revision"]})).await.status(),axum::http::StatusCode::OK);
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
async fn users_and_subscription_expiry_share_free_policy_without_migrating_standalone_accounts() {
    let f = Fixture::new().await;
    let standalone = f.consumer("standalone").await;
    let created=f.request("POST","/admin/api/users",json!({"username":"customer","password":"customer-password","name":"Customer","email":"customer@example.test","enabled":true,"revision":null})).await;
    assert_eq!(created.status(), axum::http::StatusCode::OK);
    let user = body(created).await;
    let user_id = user["id"].as_str().unwrap();
    assert_eq!(user["wallet_balance_usd"], "0");
    assert!(user.get("password_hash").is_none());
    let consumers = f.get("/admin/api/consumers").await;
    assert_eq!(consumers["items"].as_array().unwrap().len(), 1);
    assert_eq!(consumers["items"][0]["id"], standalone["id"]);
    let subscriptions = f.get("/admin/api/subscriptions").await;
    let current = &subscriptions["items"][0];
    assert_eq!(current["plan_type"], "free");
    let id = current["virtual_account_id"].as_str().unwrap();
    let expired = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    let response=f.request("PUT",&format!("/admin/api/subscriptions/{id}"),json!({"user_id":user_id,"plan_id":"plus","expires_at":expired,"reissue":false,"enabled":true,"revision":current["revision"]})).await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert!(
        f.get("/admin/api/subscriptions").await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let historical = f.get("/admin/api/subscriptions?include_expired=true").await;
    assert_eq!(historical["items"][0]["expired"], true);
    let effective = f
        .storage
        .effective_virtual_account(id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        effective.plan_id,
        f.storage.platform_free_plan("chatgpt").await.unwrap().id
    );
    assert_eq!(effective.username, "customer");
    assert!(effective.subscription_expires_at.is_none());
    assert!(
        !f.storage
            .effective_entitlements(id)
            .await
            .unwrap()
            .execution_enabled
    );
    let tampered = json!({"username":"customer","password":"","name":"Customer","email":"customer@example.test","enabled":true,"revision":user["revision"],"wallet_balance_usd":"999"});
    assert_eq!(
        f.request("PUT", &format!("/admin/api/users/{user_id}"), tampered)
            .await
            .status(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn free_cannot_be_deleted_or_retyped_and_paid_plans_have_no_expiry_free_fields() {
    let f = Fixture::new().await;
    let plans = f.get("/admin/api/plans").await;
    let free = plans["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "free")
        .unwrap();
    for plan in plans["items"].as_array().unwrap() {
        assert!(plan.get("free_access_enabled").is_none());
        assert!(plan.get("free_models").is_none());
        assert!(plan.get("free_spending_windows").is_none());
    }
    assert_eq!(
        f.request(
            "DELETE",
            "/admin/api/plans/free",
            json!({"revision":free["revision"]})
        )
        .await
        .status(),
        axum::http::StatusCode::BAD_REQUEST
    );
    let mut input = plan_input();
    input["revision"] = free["revision"].clone();
    assert_eq!(
        f.request("PUT", "/admin/api/plans/free", input.clone())
            .await
            .status(),
        axum::http::StatusCode::BAD_REQUEST
    );
    input["revision"] = serde_json::Value::Null;
    input["free_access_enabled"] = true.into();
    assert_eq!(
        f.request("POST", "/admin/api/plans", input).await.status(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert!(
        sqlx::query("DELETE FROM virtual_plans WHERE id='free'")
            .execute(f.storage.pool())
            .await
            .is_err()
    );
}
