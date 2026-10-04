use chrono::{Duration, Utc};
use codex2api_storage::{
    CheckoutInput, CouponInput, OrderFilter, OrderRequest, Storage, SubscriptionChange,
    SubscriptionOrder, User, VirtualAccount,
};
use serde_json::json;

async fn setup() -> (tempfile::TempDir, Storage, User) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("orders.sqlite"))
        .await
        .unwrap();
    let user = User {
        id: "buyer".into(),
        username: "buyer".into(),
        password_hash: codex2api_storage::hash_password("password").unwrap(),
        name: "Buyer".into(),
        email: "buyer@example.test".into(),
        enabled: true,
        wallet_cents: 0,
        revision: 1,
        created_at: Utc::now().to_rfc3339(),
    };
    storage.save_user(&user, None).await.unwrap();
    storage
        .save_supplier_tag("private-pool", "chatgpt", "Supplier private detail")
        .await
        .unwrap();
    for (id, price) in [("plus", "30"), ("pro", "60")] {
        let mut plan = storage.virtual_plan(id).await.unwrap().unwrap();
        plan.config["sale_price_usd"] = json!(price);
        plan.config["duration_days"] = json!(30);
        plan.config["supplier_tag_id"] = json!("private-pool");
        storage
            .save_virtual_plan(&plan, Some(plan.revision))
            .await
            .unwrap();
    }
    sqlx::query("UPDATE users SET wallet_cents=100000 WHERE id=?")
        .bind(&user.id)
        .execute(storage.pool())
        .await
        .unwrap();
    (dir, storage, user)
}
async fn preview(
    storage: &Storage,
    user: &User,
    plan_id: &str,
    coupon: &str,
) -> codex2api_storage::Result<serde_json::Value> {
    let plan = storage.virtual_plan(plan_id).await?.unwrap();
    let subscription = storage
        .user_subscriptions(Some(&user.id), true)
        .await?
        .into_iter()
        .find(|s| s.provider_id == "chatgpt")
        .unwrap();
    storage
        .checkout_preview(
            &user.id,
            CheckoutInput {
                plan_id: plan_id.into(),
                plan_revision: plan.revision,
                subscription_revision: Some(subscription.revision),
                payment_method: "wallet".into(),
                coupon_code: coupon.into(),
            },
        )
        .await
}
async fn quote(
    storage: &Storage,
    user: &User,
    plan_id: &str,
) -> codex2api_storage::Result<SubscriptionOrder> {
    let preview = preview(storage, user, plan_id, "").await?;
    storage
        .create_subscription_order(OrderRequest {
            user_id: &user.id,
            preview_token: preview["preview_token"].as_str().unwrap(),
        })
        .await
}
async fn grant(storage: &Storage, user: &User, plan: &str, expiry: &str) {
    let subscription = storage
        .user_subscriptions(Some(&user.id), true)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.provider_id == "chatgpt")
        .unwrap();
    storage
        .save_user_subscription(SubscriptionChange {
            reissue: false,
            user_id: &user.id,
            plan_id: plan,
            expires_at: Some(expiry),
            enabled: true,
            revision: Some(subscription.revision),
        })
        .await
        .unwrap();
}
#[tokio::test]
async fn order_filters_and_plan_options_preserve_user_ownership() {
    let (_dir, storage, user) = setup().await;
    let mut other = user.clone();
    other.id = "other-buyer".into();
    other.username = "other-buyer".into();
    storage.save_user(&other, None).await.unwrap();
    let order = quote(&storage, &user, "plus").await.unwrap();
    quote(&storage, &other, "pro").await.unwrap();
    let filter = OrderFilter {
        plan_id: "plus".into(),
        ..Default::default()
    };
    let admin = storage
        .admin_subscription_orders(Some(&user.id), &filter)
        .await
        .unwrap();
    assert_eq!(admin["total"], 1);
    assert_eq!(admin["items"][0]["username"], user.username);
    assert_eq!(
        storage
            .user_subscription_orders(&other.id, &filter)
            .await
            .unwrap()["total"],
        0
    );
    assert_eq!(
        storage.order_plan_options(Some(&user.id)).await.unwrap(),
        vec![json!({"id":"plus","provider_id":"chatgpt","name":order.plan_name})]
    );
    assert_eq!(storage.order_plan_options(None).await.unwrap().len(), 2);
    storage.close().await;
}

#[tokio::test]
async fn orders_quote_before_debit_and_pay_idempotently_under_contention() {
    let (_dir, storage, user) = setup().await;
    let preview = preview(&storage, &user, "plus", "").await.unwrap();
    assert_eq!(
        storage
            .user_subscription_orders(&user.id, &OrderFilter::default())
            .await
            .unwrap()["total"],
        0
    );
    let proof = preview["preview_token"].as_str().unwrap();
    let first = storage
        .create_subscription_order(OrderRequest {
            user_id: &user.id,
            preview_token: proof,
        })
        .await
        .unwrap();
    assert_eq!(first.kind, "purchase");
    assert_eq!(first.status, "pending");
    assert_eq!(first.amount_cents, 3000);
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        100000
    );
    assert!(storage.wallet_entries(&user.id).await.unwrap().is_empty());
    sqlx::query("UPDATE users SET wallet_cents=0 WHERE id=?")
        .bind(&user.id)
        .execute(storage.pool())
        .await
        .unwrap();
    assert!(
        storage
            .pay_subscription_order(&user.id, &first.id)
            .await
            .is_err()
    );
    assert_eq!(
        storage
            .subscription_order(Some(&user.id), &first.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "pending"
    );
    assert!(storage.wallet_entries(&user.id).await.unwrap().is_empty());
    sqlx::query("UPDATE users SET wallet_cents=100000 WHERE id=?")
        .bind(&user.id)
        .execute(storage.pool())
        .await
        .unwrap();
    assert_eq!(
        storage
            .create_subscription_order(OrderRequest {
                user_id: &user.id,
                preview_token: proof
            })
            .await
            .unwrap()
            .id,
        first.id
    );
    let second = quote(&storage, &user, "plus").await.unwrap();
    let (one, two) = tokio::join!(
        storage.pay_subscription_order(&user.id, &first.id),
        storage.pay_subscription_order(&user.id, &first.id)
    );
    assert_eq!(one.unwrap().status, "paid");
    assert_eq!(two.unwrap().status, "paid");
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        97000
    );
    assert_eq!(storage.wallet_entries(&user.id).await.unwrap().len(), 1);
    assert!(
        storage
            .pay_subscription_order(&user.id, &second.id)
            .await
            .is_err()
    );
    assert_eq!(
        storage
            .subscription_order(Some(&user.id), &second.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "cancelled"
    );
    assert!(
        storage
            .cancel_subscription_order(Some(&user.id), &first.id)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE subscription_orders SET amount_cents=1 WHERE id=?")
            .bind(&first.id)
            .execute(storage.pool())
            .await
            .is_err()
    );
    let visible = first.view(false).unwrap().to_string();
    for secret in [
        "private-pool",
        "supplier_tag_id",
        "request_signature",
        "password",
        "username",
    ] {
        assert!(!visible.contains(secret));
    }
    storage.close().await;
}

#[tokio::test]
async fn upgrade_uses_grant_snapshot_remaining_time_and_keeps_expiry_and_budget_anchor() {
    let (_dir, storage, user) = setup().await;
    let expiry = (Utc::now() + Duration::days(15)).to_rfc3339();
    grant(&storage, &user, "plus", &expiry).await;
    let account = storage
        .user_platform_account(&user.id, "chatgpt")
        .await
        .unwrap()
        .unwrap();
    let anchor: String =
        sqlx::query_scalar("SELECT subscription_started_at FROM virtual_accounts WHERE id=?")
            .bind(&account.id)
            .fetch_one(storage.pool())
            .await
            .unwrap();
    let mut old_plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    old_plan.config["sale_price_usd"] = json!("300");
    storage
        .save_virtual_plan(&old_plan, Some(old_plan.revision))
        .await
        .unwrap();
    let order = quote(&storage, &user, "pro").await.unwrap();
    assert_eq!(order.kind, "upgrade");
    assert_eq!(order.gross_cents, 3000);
    assert_eq!(order.credit_cents, 1500);
    assert_eq!(order.amount_cents, 1500);
    let paid = storage
        .pay_subscription_order(&user.id, &order.id)
        .await
        .unwrap();
    assert_eq!(paid.balance_cents, Some(98500));
    let after = storage
        .user_platform_account(&user.id, "chatgpt")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.plan_id, "pro");
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(after.subscription_expires_at.as_ref().unwrap())
            .unwrap()
            .timestamp_millis(),
        chrono::DateTime::parse_from_rfc3339(&expiry)
            .unwrap()
            .timestamp_millis()
    );
    let after_anchor: String =
        sqlx::query_scalar("SELECT subscription_started_at FROM virtual_accounts WHERE id=?")
            .bind(&account.id)
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(anchor, after_anchor);
    assert!(quote(&storage, &user, "pro").await.is_ok());
    old_plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    old_plan.config["sale_price_usd"] = json!("30");
    storage
        .save_virtual_plan(&old_plan, Some(old_plan.revision))
        .await
        .unwrap();
    assert!(quote(&storage, &user, "plus").await.is_err());
    grant(
        &storage,
        &user,
        "pro",
        &(Utc::now() - Duration::seconds(1)).to_rfc3339(),
    )
    .await;
    assert_eq!(
        quote(&storage, &user, "plus").await.unwrap().kind,
        "purchase"
    );
    storage.close().await;
}

#[tokio::test]
async fn prepaid_renewal_prices_are_independent_snapshots_in_upgrade_credit() {
    let (_dir, storage, user) = setup().await;
    grant(
        &storage,
        &user,
        "plus",
        &(Utc::now() + Duration::days(15)).to_rfc3339(),
    )
    .await;
    let mut plus = storage.virtual_plan("plus").await.unwrap().unwrap();
    plus.config["sale_price_usd"] = json!("20");
    storage
        .save_virtual_plan(&plus, Some(plus.revision))
        .await
        .unwrap();
    let renewal = quote(&storage, &user, "plus").await.unwrap();
    assert_eq!(renewal.kind, "renew");
    assert_eq!(renewal.amount_cents, 2000);
    storage
        .pay_subscription_order(&user.id, &renewal.id)
        .await
        .unwrap();
    let upgrade = quote(&storage, &user, "pro").await.unwrap();
    assert_eq!(upgrade.gross_cents, 9000);
    assert_eq!(upgrade.credit_cents, 3500);
    assert_eq!(upgrade.amount_cents, 5500);
    assert_eq!(
        upgrade.view(false).unwrap()["pricing"]["credit_periods"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    storage.close().await;
}

#[tokio::test]
async fn order_ownership_expiration_cancellation_and_closed_sales_reject_payment() {
    let (_dir, storage, user) = setup().await;
    let order = quote(&storage, &user, "plus").await.unwrap();
    assert!(
        storage
            .subscription_order(Some("other"), &order.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .cancel_subscription_order(Some("other"), &order.id)
            .await
            .is_err()
    );
    assert!(
        storage
            .pay_subscription_order("other", &order.id)
            .await
            .is_err()
    );
    assert_eq!(
        storage
            .user_subscription_orders("other", &OrderFilter::default())
            .await
            .unwrap()["total"],
        0
    );
    let mut plus = storage.virtual_plan("plus").await.unwrap().unwrap();
    plus.allow_purchase = false;
    storage
        .save_virtual_plan(&plus, Some(plus.revision))
        .await
        .unwrap();
    assert!(
        storage
            .pay_subscription_order(&user.id, &order.id)
            .await
            .is_err()
    );
    assert!(quote(&storage, &user, "plus").await.is_err());
    grant(
        &storage,
        &user,
        "plus",
        &(Utc::now() + Duration::days(15)).to_rfc3339(),
    )
    .await;
    let standalone = VirtualAccount {
        id: "independent".into(),
        provider_id: "chatgpt".into(),
        username: "independent".into(),
        password_hash: "fixture".into(),
        name: "Independent".into(),
        email: "independent@example.test".into(),
        plan_id: "plus".into(),
        plan_type: "plus".into(),
        subscription_expires_at: None,
        enabled: true,
        created_at: Utc::now().to_rfc3339(),
    };
    storage.save_virtual_account(&standalone).await.unwrap();
    let expiring = quote(&storage, &user, "pro").await.unwrap();
    sqlx::query("UPDATE subscription_orders SET quote_expires_at_ms=0 WHERE id=?")
        .bind(&expiring.id)
        .execute(storage.pool())
        .await
        .unwrap();
    assert!(
        storage
            .pay_subscription_order(&user.id, &expiring.id)
            .await
            .is_err()
    );
    assert_eq!(
        storage
            .subscription_order(Some(&user.id), &expiring.id)
            .await
            .unwrap()
            .unwrap()
            .view(false)
            .unwrap()["status"],
        "expired"
    );
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        100000
    );
    let cancel = quote(&storage, &user, "pro").await.unwrap();
    assert_eq!(
        storage
            .cancel_subscription_order(None, &cancel.id)
            .await
            .unwrap()
            .status,
        "cancelled"
    );
    assert!(
        storage
            .pay_subscription_order(&user.id, &cancel.id)
            .await
            .is_err()
    );
    storage.close().await;
}

#[tokio::test]
async fn publication_is_the_only_catalog_gate_and_unknown_prices_never_become_free_orders() {
    let (_dir, storage, user) = setup().await;
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["sale_price_usd"] = serde_json::Value::Null;
    plan.config["supplier_tag_id"] = serde_json::Value::Null;
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    let listed = storage
        .user_store()
        .plans()
        .await
        .unwrap()
        .into_iter()
        .find(|item| item["id"] == "plus")
        .unwrap();
    assert!(listed["sale_price_usd"].is_null());
    assert!(listed.get("supplier_tag_id").is_none());
    assert!(quote(&storage, &user, "plus").await.is_err());
    plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.allow_purchase = false;
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    assert!(
        !storage
            .user_store()
            .plans()
            .await
            .unwrap()
            .iter()
            .any(|item| item["id"] == "plus")
    );
    storage.close().await;
}

#[tokio::test]
async fn administrator_reissue_establishes_an_explicit_new_price_snapshot() {
    let (_dir, storage, user) = setup().await;
    let expiry = (Utc::now() + Duration::days(15)).to_rfc3339();
    grant(&storage, &user, "plus", &expiry).await;
    let subscription = storage
        .user_subscriptions(Some(&user.id), true)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.provider_id == "chatgpt")
        .unwrap();
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["sale_price_usd"] = json!("40");
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    // Updating the plan also revises its subscribed accounts.
    let refreshed = storage
        .user_subscriptions(Some(&user.id), true)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.provider_id == "chatgpt")
        .unwrap();
    assert!(refreshed.revision > subscription.revision);
    storage
        .save_user_subscription(SubscriptionChange {
            reissue: true,
            user_id: &user.id,
            plan_id: "plus",
            expires_at: Some(&expiry),
            enabled: true,
            revision: Some(refreshed.revision),
        })
        .await
        .unwrap();
    let order = quote(&storage, &user, "pro").await.unwrap();
    assert_eq!(order.gross_cents, 3000);
    assert_eq!(order.credit_cents, 2000);
    assert_eq!(order.amount_cents, 1000);
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        100000
    );
    assert!(storage.wallet_entries(&user.id).await.unwrap().is_empty());
    storage.close().await;
}

#[tokio::test]
async fn coupons_reserve_only_on_confirmation_and_capture_discount_without_supplier_requirement() {
    let (_dir, storage, user) = setup().await;
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["supplier_tag_id"] = serde_json::Value::Null;
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    let now = Utc::now().timestamp_millis();
    let coupon = storage
        .save_coupon(
            None,
            CouponInput {
                code: "save-five".into(),
                name: "Five dollars".into(),
                enabled: true,
                discount_cents: 500,
                minimum_cents: 1000,
                plan_id: Some("plus".into()),
                starts_at_ms: now - 1000,
                ends_at_ms: now + 86400000,
                max_uses: Some(1),
                per_user_limit: 1,
                revision: None,
            },
        )
        .await
        .unwrap();
    let one = preview(&storage, &user, "plus", "save-five").await.unwrap();
    let two = preview(&storage, &user, "plus", "SAVE-FIVE").await.unwrap();
    assert_eq!(one["amount_cents"], 2500);
    assert_eq!(
        storage.coupons().await.unwrap()["items"][0]["reserved_count"],
        0
    );
    let proof = one["preview_token"].as_str().unwrap();
    for purpose in [
        codex2api_storage::TokenPurpose::UserSession,
        codex2api_storage::TokenPurpose::AdminSession,
    ] {
        assert!(storage.verify_jwt(purpose, proof).await.is_err());
    }
    assert!(
        storage
            .create_subscription_order(OrderRequest {
                user_id: "other",
                preview_token: proof
            })
            .await
            .is_err()
    );
    let order = storage
        .create_subscription_order(OrderRequest {
            user_id: &user.id,
            preview_token: proof,
        })
        .await
        .unwrap();
    assert_eq!(
        storage.coupons().await.unwrap()["items"][0]["reserved_count"],
        1
    );
    assert!(
        storage
            .create_subscription_order(OrderRequest {
                user_id: &user.id,
                preview_token: two["preview_token"].as_str().unwrap()
            })
            .await
            .is_err()
    );
    storage
        .cancel_subscription_order(Some(&user.id), &order.id)
        .await
        .unwrap();
    let reserved = storage
        .create_subscription_order(OrderRequest {
            user_id: &user.id,
            preview_token: two["preview_token"].as_str().unwrap(),
        })
        .await
        .unwrap();
    storage
        .save_coupon(
            Some(&coupon.id),
            CouponInput {
                code: coupon.code,
                name: coupon.name,
                enabled: false,
                discount_cents: 900,
                minimum_cents: 0,
                plan_id: None,
                starts_at_ms: now - 1000,
                ends_at_ms: now + 86400000,
                max_uses: Some(1),
                per_user_limit: 1,
                revision: Some(coupon.revision),
            },
        )
        .await
        .unwrap();
    let paid = storage
        .pay_subscription_order(&user.id, &reserved.id)
        .await
        .unwrap();
    assert_eq!(paid.amount_cents, 2500);
    assert_eq!(paid.discount_cents, 500);
    assert_eq!(paid.balance_cents, Some(97500));
    let stats = storage.coupons().await.unwrap();
    assert_eq!(stats["items"][0]["used_count"], 1);
    assert_eq!(stats["items"][0]["reserved_count"], 0);
    let upgrade = preview(&storage, &user, "pro", "").await.unwrap();
    assert_eq!(upgrade["credit_cents"], 2500);
    assert_eq!(upgrade["amount_cents"], 3500);
    storage.close().await;
}

#[tokio::test]
async fn invalid_coupon_proofs_and_stale_previews_never_create_orders() {
    let (_dir, storage, user) = setup().await;
    assert!(preview(&storage, &user, "plus", "unknown").await.is_err());
    let value = preview(&storage, &user, "plus", "").await.unwrap();
    let proof = value["preview_token"].as_str().unwrap();
    let mut parts: Vec<_> = proof.split('.').map(str::to_owned).collect();
    parts[1].push('A');
    assert!(
        storage
            .create_subscription_order(OrderRequest {
                user_id: &user.id,
                preview_token: &parts.join(".")
            })
            .await
            .is_err()
    );
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["sale_price_usd"] = json!("40");
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    assert!(
        storage
            .create_subscription_order(OrderRequest {
                user_id: &user.id,
                preview_token: proof
            })
            .await
            .is_err()
    );
    assert_eq!(
        storage
            .user_subscription_orders(&user.id, &OrderFilter::default())
            .await
            .unwrap()["total"],
        0
    );
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        100000
    );
    storage.close().await;
}
