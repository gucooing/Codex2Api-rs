#[cfg(test)]
mod account_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../codex2api-storage/test-support/accounts.rs"
    ));
}
#[cfg(test)]
use account_fixture::AccountFixture;
use chrono::{Duration, Utc};
use codex2api_storage::{
    CheckoutInput, CouponInput, OrderFilter, OrderRequest, PlatformAccount, Storage,
    SubscriptionChange, SubscriptionOrder, User,
};
use serde_json::json;

async fn setup() -> (tempfile::TempDir, Storage, User) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("orders.sqlite"))
        .await
        .unwrap();
    let user = User {
        kind: codex2api_storage::UserKind::Regular,
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
    subscription_id: Option<&str>,
    plan_id: &str,
    coupon: &str,
) -> codex2api_storage::Result<serde_json::Value> {
    let plan = storage.virtual_plan(plan_id).await?.unwrap();
    let revision = match subscription_id {
        Some(id) => Some(storage.platform_revision(id).await?),
        None => None,
    };
    storage
        .checkout_preview(
            &user.id,
            CheckoutInput {
                subscription_id: subscription_id.map(str::to_owned),
                plan_id: plan_id.into(),
                plan_revision: plan.revision,
                subscription_revision: revision,
                payment_method: "wallet".into(),
                coupon_code: coupon.into(),
            },
        )
        .await
}
async fn quote(
    storage: &Storage,
    user: &User,
    subscription_id: Option<&str>,
    plan_id: &str,
) -> codex2api_storage::Result<SubscriptionOrder> {
    let preview = preview(storage, user, subscription_id, plan_id, "").await?;
    storage
        .create_subscription_order(OrderRequest {
            user_id: &user.id,
            preview_token: preview["preview_token"].as_str().unwrap(),
        })
        .await
}
async fn grant(
    storage: &Storage,
    user: &User,
    subscription_id: Option<&str>,
    plan: &str,
    expiry: &str,
) -> String {
    let revision = match subscription_id {
        Some(id) => Some(storage.platform_revision(id).await.unwrap()),
        None => None,
    };
    storage
        .save_user_subscription(SubscriptionChange {
            subscription_id,
            reissue: false,
            user_id: &user.id,
            plan_id: plan,
            expires_at: Some(expiry),
            enabled: true,
            revision,
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn order_payment_does_not_stale_relative_wallet_adjustments() {
    let (_dir, storage, user) = setup().await;
    let snapshot = storage.user(&user.id).await.unwrap().unwrap();
    let order = quote(&storage, &user, None, "plus").await.unwrap();
    let paid = storage
        .pay_subscription_order(&user.id, &order.id)
        .await
        .unwrap();
    assert_eq!(paid.status, "paid");
    let admin = storage.require_admin_user().await.unwrap();
    let entry = storage
        .adjust_user_wallet(
            &user.id,
            admin.id,
            codex2api_storage::WalletAdjustment {
                request_id: "after-order-payment".into(),
                amount_cents: 100,
                expected_revision: snapshot.revision,
                reason: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        entry.balance_before_cents,
        snapshot.wallet_cents - paid.amount_cents
    );
    assert_eq!(
        entry.balance_cents,
        snapshot.wallet_cents - paid.amount_cents + 100
    );
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().revision,
        snapshot.revision
    );
    storage.close().await;
}

#[tokio::test]
async fn purchased_subscriptions_follow_current_plan_pools_without_changing_history() {
    use codex2api_storage::{NewSupplierAccount, SupplierTokens};
    let (dir, storage, user) = setup().await;
    for tag in ["replacement", "free-pool"] {
        storage
            .save_supplier_tag(tag, "chatgpt", tag)
            .await
            .unwrap();
    }
    for (id, tag) in [
        ("s1", "private-pool"),
        ("s2", "replacement"),
        ("s3", "free-pool"),
    ] {
        let mut supplier = NewSupplierAccount::pending_identity(
            id,
            "codex_cli_rs",
            "fixture",
            "Linux",
            "6",
            "x86_64",
            "",
            "{}",
        );
        supplier.id = Some(id.into());
        supplier.chatgpt_account_id = Some(id.into());
        supplier.chatgpt_user_id = Some(format!("user-{id}"));
        storage
            .save_authorized_account(
                supplier,
                SupplierTokens {
                    access_token: Some("fixture".into()),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        storage
            .replace_supplier_tags(&[id.into()], &[tag.into()])
            .await
            .unwrap();
    }
    assert!(
        storage
            .user_subscriptions(Some(&user.id), true)
            .await
            .unwrap()
            .is_empty()
    );
    let order = quote(&storage, &user, None, "plus").await.unwrap();
    let paid = storage
        .pay_subscription_order(&user.id, &order.id)
        .await
        .unwrap();
    let subscription = storage
        .platform_account(paid.subscription_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        paid.subscription_id.as_deref(),
        Some(subscription.id.as_str())
    );
    assert_eq!(
        storage
            .select_pool_supplier(&subscription.id, "chatgpt", &[])
            .await
            .unwrap()
            .as_deref(),
        Some("s1")
    );
    let anchor: String =
        sqlx::query_scalar("SELECT subscription_started_at FROM platform_accounts WHERE id=?")
            .bind(&subscription.id)
            .fetch_one(storage.pool())
            .await
            .unwrap();
    let pricing: Vec<(String, i64, i64)> = sqlx::query_as("SELECT id,starts_at_ms,ends_at_ms FROM subscription_pricing_periods WHERE subscription_id=?")
        .bind(&subscription.id).fetch_all(storage.pool()).await.unwrap();

    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["supplier_tag_id"] = json!("replacement");
    assert!(
        storage
            .save_virtual_plan(&plan, Some(plan.revision))
            .await
            .unwrap()
    );
    let route = storage
        .execution_route(&subscription.id, "chatgpt")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(route.tag_id.as_deref(), Some("replacement"));
    let persisted: Option<String> =
        sqlx::query_scalar("SELECT tag_id FROM execution_routes WHERE virtual_account_id=?")
            .bind(&subscription.id)
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert!(
        persisted.is_none(),
        "a subscription must never store its own pool"
    );
    assert_eq!(
        storage
            .select_pool_supplier(&subscription.id, "chatgpt", &[])
            .await
            .unwrap()
            .as_deref(),
        Some("s2")
    );
    assert_eq!(
        storage
            .supplier_tags()
            .await
            .unwrap()
            .into_iter()
            .find(|t| t.id == "replacement")
            .unwrap()
            .binding_count,
        1
    );
    assert!(
        storage
            .save_pool_route(
                &subscription.id,
                "chatgpt",
                Some("private-pool"),
                Some(route.revision)
            )
            .await
            .unwrap()
    );
    assert_eq!(
        storage
            .execution_route(&subscription.id, "chatgpt")
            .await
            .unwrap()
            .unwrap()
            .tag_id
            .as_deref(),
        Some("private-pool")
    );
    let revision = storage
        .execution_route(&subscription.id, "chatgpt")
        .await
        .unwrap()
        .unwrap()
        .revision;
    assert!(
        storage
            .save_pool_route(&subscription.id, "chatgpt", None, Some(revision))
            .await
            .unwrap()
    );
    // Removing membership and recovering supply both use the inherited pool.
    storage
        .replace_supplier_tags(&["s2".into()], &[])
        .await
        .unwrap();
    assert!(
        storage
            .select_pool_supplier(&subscription.id, "chatgpt", &[])
            .await
            .unwrap()
            .is_none()
    );
    storage
        .replace_supplier_tags(&["s2".into()], &["replacement".into()])
        .await
        .unwrap();
    storage.invalidate_runtime_cache();
    assert_eq!(
        storage
            .select_pool_supplier(&subscription.id, "chatgpt", &[])
            .await
            .unwrap()
            .as_deref(),
        Some("s2")
    );
    // Unrelated plan edits preserve the current assignment and its revision.
    let route = storage
        .execution_route(&subscription.id, "chatgpt")
        .await
        .unwrap()
        .unwrap();
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.name = "Renamed plan".into();
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    assert_eq!(
        storage
            .execution_route(&subscription.id, "chatgpt")
            .await
            .unwrap()
            .unwrap()
            .revision,
        route.revision
    );
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["supplier_tag_id"] = serde_json::Value::Null;
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    assert!(
        storage
            .select_pool_supplier(&subscription.id, "chatgpt", &[])
            .await
            .unwrap()
            .is_none()
    );

    let unchanged = storage
        .platform_account(&subscription.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(unchanged.id, subscription.id);
    assert_eq!(
        unchanged.subscription_expires_at,
        subscription.subscription_expires_at
    );
    let current_anchor: String =
        sqlx::query_scalar("SELECT subscription_started_at FROM platform_accounts WHERE id=?")
            .bind(&subscription.id)
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(current_anchor, anchor);
    let current_pricing: Vec<(String, i64, i64)> = sqlx::query_as("SELECT id,starts_at_ms,ends_at_ms FROM subscription_pricing_periods WHERE subscription_id=?")
        .bind(&subscription.id).fetch_all(storage.pool()).await.unwrap();
    assert_eq!(current_pricing, pricing);
    assert_eq!(storage.wallet_entries(&user.id).await.unwrap().len(), 1);
    let replay = storage
        .pay_subscription_order(&user.id, &order.id)
        .await
        .unwrap();
    assert_eq!(replay.amount_cents, paid.amount_cents);
    assert_eq!(replay.balance_cents, paid.balance_cents);

    let expires = (Utc::now() - Duration::seconds(1)).to_rfc3339();
    sqlx::query("UPDATE platform_accounts SET subscription_expires_at=? WHERE id=?")
        .bind(&expires)
        .bind(&subscription.id)
        .execute(storage.pool())
        .await
        .unwrap();
    storage.invalidate_runtime_cache();
    assert!(
        !storage
            .effective_entitlements(&subscription.id)
            .await
            .unwrap()
            .execution_enabled
    );
    assert_eq!(
        storage
            .platform_account(&subscription.id)
            .await
            .unwrap()
            .unwrap()
            .plan_id,
        "plus"
    );
    assert!(storage.virtual_plan("free").await.unwrap().is_none());
    storage.close().await;
    let reopened = Storage::open(dir.path().join("orders.sqlite"))
        .await
        .unwrap();
    assert!(
        !reopened
            .effective_entitlements(&subscription.id)
            .await
            .unwrap()
            .execution_enabled
    );
    reopened.close().await;
}

#[tokio::test]
async fn order_filters_and_plan_options_preserve_user_ownership() {
    let (_dir, storage, user) = setup().await;
    let mut other = user.clone();
    other.id = "other-buyer".into();
    other.username = "other-buyer".into();
    storage.save_user(&other, None).await.unwrap();
    let order = quote(&storage, &user, None, "plus").await.unwrap();
    quote(&storage, &other, None, "pro").await.unwrap();
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
    let preview = preview(&storage, &user, None, "plus", "").await.unwrap();
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
    let second = quote(&storage, &user, None, "plus").await.unwrap();
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
    let second_paid = storage
        .pay_subscription_order(&user.id, &second.id)
        .await
        .unwrap();
    assert_eq!(second_paid.status, "paid");
    let first_paid = storage
        .subscription_order(Some(&user.id), &first.id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(first_paid.subscription_id, second_paid.subscription_id);
    assert_eq!(
        storage
            .user_subscriptions(Some(&user.id), true)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        94000
    );
    assert_eq!(storage.wallet_entries(&user.id).await.unwrap().len(), 2);
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
    let account_id = grant(&storage, &user, None, "plus", &expiry).await;
    let account = storage
        .platform_account(&account_id)
        .await
        .unwrap()
        .unwrap();
    let anchor: String =
        sqlx::query_scalar("SELECT subscription_started_at FROM platform_accounts WHERE id=?")
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
    let order = quote(&storage, &user, Some(&account.id), "pro")
        .await
        .unwrap();
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
        .platform_account(&account.id)
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
        sqlx::query_scalar("SELECT subscription_started_at FROM platform_accounts WHERE id=?")
            .bind(&account.id)
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(anchor, after_anchor);
    assert!(
        quote(&storage, &user, Some(&account.id), "pro")
            .await
            .is_ok()
    );
    old_plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    old_plan.config["sale_price_usd"] = json!("30");
    storage
        .save_virtual_plan(&old_plan, Some(old_plan.revision))
        .await
        .unwrap();
    assert!(
        quote(&storage, &user, Some(&account.id), "plus")
            .await
            .is_err()
    );
    grant(
        &storage,
        &user,
        Some(&account.id),
        "pro",
        &(Utc::now() - Duration::seconds(1)).to_rfc3339(),
    )
    .await;
    assert_eq!(
        quote(&storage, &user, Some(&account.id), "plus")
            .await
            .unwrap()
            .kind,
        "purchase"
    );
    storage.close().await;
}

#[tokio::test]
async fn prepaid_renewal_prices_are_independent_snapshots_in_upgrade_credit() {
    let (_dir, storage, user) = setup().await;
    let subscription_id = grant(
        &storage,
        &user,
        None,
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
    let renewal = quote(&storage, &user, Some(&subscription_id), "plus")
        .await
        .unwrap();
    assert_eq!(renewal.kind, "renew");
    assert_eq!(renewal.amount_cents, 2000);
    storage
        .pay_subscription_order(&user.id, &renewal.id)
        .await
        .unwrap();
    let upgrade = quote(&storage, &user, Some(&subscription_id), "pro")
        .await
        .unwrap();
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
    let order = quote(&storage, &user, None, "plus").await.unwrap();
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
    assert!(quote(&storage, &user, None, "plus").await.is_err());
    let subscription_id = grant(
        &storage,
        &user,
        None,
        "plus",
        &(Utc::now() + Duration::days(15)).to_rfc3339(),
    )
    .await;
    let standalone = PlatformAccount {
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
    storage.save_account_fixture(&standalone).await.unwrap();
    let expiring = quote(&storage, &user, Some(&subscription_id), "pro")
        .await
        .unwrap();
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
    let cancel = quote(&storage, &user, Some(&subscription_id), "pro")
        .await
        .unwrap();
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
async fn catalog_preview_confirmation_and_payment_require_published_priced_plans() {
    let (_dir, storage, user) = setup().await;
    for (price, published) in [(None, true), (Some("30"), false), (None, false)] {
        let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
        plan.config["sale_price_usd"] = json!("30");
        plan.config["supplier_tag_id"] = json!(null);
        plan.allow_purchase = true;
        storage
            .save_virtual_plan(&plan, Some(plan.revision))
            .await
            .unwrap();
        let stale_preview = preview(&storage, &user, None, "plus", "").await.unwrap();
        let pending = quote(&storage, &user, None, "plus").await.unwrap();

        plan = storage.virtual_plan("plus").await.unwrap().unwrap();
        plan.config["sale_price_usd"] = json!(price);
        plan.allow_purchase = published;
        storage
            .save_virtual_plan(&plan, Some(plan.revision))
            .await
            .unwrap();
        let page = storage
            .user_store()
            .plans(&codex2api_storage::ListQuery {
                search: plan.name.clone(),
                page_size: Some(10),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page.total, 0);
        assert!(page.items.is_empty());
        assert!(preview(&storage, &user, None, "plus", "").await.is_err());
        assert!(
            storage
                .create_subscription_order(OrderRequest {
                    user_id: &user.id,
                    preview_token: stale_preview["preview_token"].as_str().unwrap(),
                })
                .await
                .is_err()
        );
        assert!(
            storage
                .pay_subscription_order(&user.id, &pending.id)
                .await
                .is_err()
        );
        assert_eq!(
            storage
                .subscription_order(Some(&user.id), &pending.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            "cancelled"
        );
        assert_eq!(
            storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
            100000
        );
        assert!(storage.wallet_entries(&user.id).await.unwrap().is_empty());
        assert!(
            storage
                .user_subscriptions(Some(&user.id), true)
                .await
                .unwrap()
                .is_empty()
        );
    }
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["sale_price_usd"] = json!("0");
    plan.allow_purchase = true;
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    let listed = storage
        .user_store()
        .plans(&codex2api_storage::ListQuery {
            search: plan.name.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(listed.total, 1);
    assert_eq!(listed.items[0]["id"], "plus");
    assert!(listed.items[0]["sale_price_usd"].is_string());
    assert!(listed.items[0].get("supplier_tag_id").is_none());
    assert_eq!(
        quote(&storage, &user, None, "plus")
            .await
            .unwrap()
            .amount_cents,
        0
    );
    storage.close().await;
}

#[tokio::test]
async fn administrator_reissue_establishes_an_explicit_new_price_snapshot() {
    let (_dir, storage, user) = setup().await;
    let expiry = (Utc::now() + Duration::days(15)).to_rfc3339();
    let subscription_id = grant(&storage, &user, None, "plus", &expiry).await;
    let subscription = storage
        .user_subscriptions(Some(&user.id), true)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.virtual_account_id == subscription_id)
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
        .find(|s| s.virtual_account_id == subscription_id)
        .unwrap();
    assert!(refreshed.revision > subscription.revision);
    storage
        .save_user_subscription(SubscriptionChange {
            subscription_id: Some(&refreshed.virtual_account_id),
            reissue: true,
            user_id: &user.id,
            plan_id: "plus",
            expires_at: Some(&expiry),
            enabled: true,
            revision: Some(refreshed.revision),
        })
        .await
        .unwrap();
    let order = quote(&storage, &user, Some(&subscription_id), "pro")
        .await
        .unwrap();
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
    let one = preview(&storage, &user, None, "plus", "save-five")
        .await
        .unwrap();
    let two = preview(&storage, &user, None, "plus", "SAVE-FIVE")
        .await
        .unwrap();
    assert_eq!(one["amount_cents"], 2500);
    assert_eq!(
        serde_json::to_value(storage.coupon_page(&Default::default()).await.unwrap()).unwrap()["items"]
            [0]["reserved_count"],
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
        serde_json::to_value(storage.coupon_page(&Default::default()).await.unwrap()).unwrap()["items"]
            [0]["reserved_count"],
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
    let stats =
        serde_json::to_value(storage.coupon_page(&Default::default()).await.unwrap()).unwrap();
    assert_eq!(stats["items"][0]["used_count"], 1);
    assert_eq!(stats["items"][0]["reserved_count"], 0);
    let upgrade = preview(&storage, &user, paid.subscription_id.as_deref(), "pro", "")
        .await
        .unwrap();
    assert_eq!(upgrade["credit_cents"], 2500);
    assert_eq!(upgrade["amount_cents"], 3500);
    storage.close().await;
}

#[tokio::test]
async fn invalid_coupon_proofs_and_stale_previews_never_create_orders() {
    let (_dir, storage, user) = setup().await;
    assert!(
        preview(&storage, &user, None, "plus", "unknown")
            .await
            .is_err()
    );
    let value = preview(&storage, &user, None, "plus", "").await.unwrap();
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
