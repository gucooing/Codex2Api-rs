mod common;
use axum::http::StatusCode;
use codex2api_storage::{SupplierAccountUpdate, SupplierStatus, SupplierTokens, User, UserKind};
use common::*;
use serde_json::{Value, json};

fn query(path: &str, fields: &[(&str, &str)]) -> String {
    format!(
        "{path}?{}",
        url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields.iter().copied())
            .finish()
    )
}

fn check_page(value: &Value, total: usize, page: usize, size: usize, rows: usize) {
    assert_eq!(value["total"], total);
    assert_eq!(value["page"], page);
    assert_eq!(value["page_size"], size);
    assert_eq!(value["items"].as_array().unwrap().len(), rows);
}

#[tokio::test]
async fn supplier_filters_pages_and_bulk_selection_share_database_membership() {
    let f = Fixture::new().await;
    f.storage
        .save_supplier_tag("pool", "chatgpt", "Pool")
        .await
        .unwrap();
    let mut ids = Vec::new();
    for i in 0..43 {
        let id = f.state.accounts.create_pending().await.unwrap().account.id;
        f.storage
            .update_account(
                &id,
                SupplierAccountUpdate {
                    display_name: Some(format!("list-{i:02}")),
                    email: Some(format!("list-{i:02}@example.test")),
                    status: Some(SupplierStatus::Active),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        f.storage
            .upsert_supplier_tokens(SupplierTokens {
                account_id: id.clone(),
                access_token: Some("test-only".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        sqlx::query("UPDATE supplier_accounts SET created_at='2026-10-01T00:00:00Z' WHERE id=?")
            .bind(&id)
            .execute(f.storage.pool())
            .await
            .unwrap();
        ids.push(id);
    }
    f.storage
        .replace_supplier_tags(&ids[..25], &["pool".into()])
        .await
        .unwrap();
    f.storage
        .set_account_status(&ids[0], SupplierStatus::Disabled)
        .await
        .unwrap();
    let revision = f
        .storage
        .supplier_auth_revision(&ids[1])
        .await
        .unwrap()
        .unwrap();
    f.storage
        .mark_supplier_payment_required(&ids[1], revision, None, None)
        .await
        .unwrap();
    let revision = f
        .storage
        .supplier_auth_revision(&ids[2])
        .await
        .unwrap()
        .unwrap();
    f.storage
        .exhaust_supplier_quota(
            &ids[2],
            revision,
            chrono::Utc::now().timestamp() + 3600,
            "usage_limit_reached",
        )
        .await
        .unwrap();
    let first = f.get("/admin/api/suppliers?page=1&page_size=20").await;
    let second = f.get("/admin/api/suppliers?page=2&page_size=20").await;
    let last = f.get("/admin/api/suppliers?page=999&page_size=20").await;
    check_page(&first, 43, 1, 20, 20);
    check_page(&second, 43, 2, 20, 20);
    check_page(&last, 43, 3, 20, 3);
    let actual: Vec<_> = [&first, &second, &last]
        .into_iter()
        .flat_map(|v| v["items"].as_array().unwrap())
        .map(|v| v["id"].as_str().unwrap().to_owned())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(actual, sorted);
    let filtered = f
        .get("/admin/api/suppliers?tag=pool&provider_id=chatgpt&status=active&page_size=10&page=2")
        .await;
    check_page(&filtered, 22, 2, 10, 10);
    let selected = f
        .get("/admin/api/suppliers/selection?tag=pool&provider_id=chatgpt&status=active")
        .await;
    assert_eq!(selected["items"].as_array().unwrap().len(), 22);
    for item in selected["items"].as_array().unwrap() {
        assert!(ids[3..25].contains(&item["id"].as_str().unwrap().to_owned()));
        assert_eq!(item.as_object().unwrap().len(), 3);
    }
    check_page(
        &f.get("/admin/api/suppliers?tag=__untagged__").await,
        18,
        1,
        20,
        18,
    );
    let injected = "pool' OR 1=1 --";
    check_page(
        &f.get(&query("/admin/api/suppliers", &[("tag", injected)]))
            .await,
        0,
        1,
        20,
        0,
    );
    assert!(
        f.get(&query(
            "/admin/api/suppliers/selection",
            &[("tag", injected)]
        ))
        .await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn management_lists_search_before_paging_and_bind_injection_text_literally() {
    let f = Fixture::new().await;
    for i in 0..23 {
        let user = User {
            id: format!("list-user-{i:02}"),
            kind: UserKind::Regular,
            username: format!("list-user-{i:02}"),
            password_hash: "test-only".into(),
            name: format!("List User {i:02}"),
            email: format!("u{i}@example.test"),
            enabled: true,
            wallet_cents: 0,
            revision: 1,
            created_at: "2026-10-01T00:00:00Z".into(),
        };
        f.storage.save_user(&user, None).await.unwrap();
        let mut plan = plan_input();
        plan["name"] = format!("list-plan-{i:02}").into();
        assert_eq!(
            f.request("POST", "/admin/api/plans", plan).await.status(),
            StatusCode::OK
        );
        assert_eq!(
            f.request(
                "POST",
                "/admin/api/models",
                model_input(&format!("list-model-{i:02}"))
            )
            .await
            .status(),
            StatusCode::OK
        );
        f.storage
            .save_supplier_tag(
                &format!("tag-{i:02}"),
                "chatgpt",
                &format!("list-tag-{i:02}"),
            )
            .await
            .unwrap();
        assert_eq!(f.request("POST","/admin/api/proxies",json!({"name":format!("list-proxy-{i:02}"),"protocol":"http","host":format!("proxy{i}.test"),"port":8080,"username":"private-user","password":"private-secret"})).await.status(),StatusCode::OK);
        assert_eq!(f.request("POST","/admin/api/coupons",json!({"code":format!("LIST-{i:02}"),"name":format!("list-coupon-{i:02}"),"enabled":true,"discount_cents":100,"minimum_cents":0,"plan_id":null,"starts_at_ms":0,"ends_at_ms":2000000000000_i64,"max_uses":null,"per_user_limit":1,"revision":null})).await.status(),StatusCode::OK);
    }
    for resource in [
        "users",
        "plans",
        "models",
        "proxies",
        "supplier-tags",
        "coupons",
    ] {
        let path = format!("/admin/api/{resource}");
        check_page(
            &f.get(&query(
                &path,
                &[("search", "list-"), ("page", "2"), ("page_size", "10")],
            ))
            .await,
            23,
            2,
            10,
            10,
        );
        for text in [
            "' OR 1=1 --",
            "x'; DROP TABLE supplier_accounts;--",
            "%",
            "_",
        ] {
            check_page(
                &f.get(&query(&path, &[("search", text)])).await,
                0,
                1,
                20,
                0,
            );
        }
        assert_eq!(
            f.request(
                "GET",
                &query(&path, &[("page_size", "10; SELECT 1")]),
                Value::Null
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            f.request(
                "GET",
                &query(&path, &[("sort", "id; DROP TABLE accounts")]),
                Value::Null
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
    }
    // Searching URL components must not turn proxy credentials into a search oracle.
    check_page(
        &f.get("/admin/api/proxies?search=private-secret").await,
        0,
        1,
        20,
        0,
    );
    check_page(
        &f.get("/admin/api/proxies?search=proxy2.test&protocol=http&result=unchecked")
            .await,
        1,
        1,
        20,
        1,
    );
    let literal = "' OR 1=1 --";
    let current = f.storage.user("list-user-00").await.unwrap().unwrap();
    assert_eq!(f.request("PUT","/admin/api/users/list-user-00",json!({"username":current.username,"name":literal,"email":current.email,"password":"","enabled":true,"revision":current.revision})).await.status(),StatusCode::OK);
    let found = f
        .get(&query("/admin/api/users", &[("search", literal)]))
        .await;
    check_page(&found, 1, 1, 20, 1);
    assert_eq!(found["items"][0]["id"], "list-user-00");
    check_page(
        &f.get("/admin/api/subscriptions?user_id=list-user-00&include_expired=true")
            .await,
        2,
        1,
        20,
        2,
    );
    check_page(
        &f.get(&query(
            "/admin/api/subscriptions",
            &[("user_id", literal), ("include_expired", "true")],
        ))
        .await,
        0,
        1,
        20,
        0,
    );
    let operator = f.storage.require_admin_user().await.unwrap();
    for i in 0..23 {
        let current = f.storage.user("list-user-00").await.unwrap().unwrap();
        f.storage
            .adjust_user_wallet(
                &current.id,
                operator.id,
                codex2api_storage::WalletAdjustment {
                    request_id: format!("list-wallet-{i}"),
                    amount_cents: 100,
                    expected_revision: current.revision,
                    reason: Some("private operator reason".into()),
                },
            )
            .await
            .unwrap();
    }
    let wallet = f
        .storage
        .user_store()
        .wallet_entries(
            "list-user-00",
            &codex2api_storage::ListQuery {
                user_id: "list-user-01".into(),
                page: 3,
                page_size: Some(10),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let wallet = serde_json::to_value(wallet).unwrap();
    check_page(&wallet, 23, 3, 10, 3);
    for entry in wallet["items"].as_array().unwrap() {
        for private in ["reason", "operator_name", "operator_account_id", "user_id"] {
            assert!(entry.get(private).is_none());
        }
    }
    assert_eq!(
        f.storage
            .user_store()
            .wallet_entries("list-user-01", &Default::default())
            .await
            .unwrap()
            .total,
        0
    );
}

#[tokio::test]
async fn account_record_pages_keep_owner_and_json_paths_bound() {
    let f = Fixture::new().await;
    let first = f.consumer("record-a").await;
    let second = f.consumer("record-b").await;
    let owner = first["id"].as_str().unwrap();
    let other = second["id"].as_str().unwrap();
    for i in 0..27 {
        f.storage
            .save_virtual_resource(
                owner,
                "task",
                &format!("record-{i:02}"),
                None,
                &json!({"title":format!("Local {i}")}),
            )
            .await
            .unwrap();
        f.storage
            .save_virtual_resource(
                other,
                "task",
                &format!("record-{i:02}"),
                None,
                &json!({"title":"Other owner"}),
            )
            .await
            .unwrap();
    }
    let path = format!("/admin/api/consumers/{owner}/records");
    let page = f
        .get(&query(
            &path,
            &[("kind", "task"), ("page", "3"), ("page_size", "10")],
        ))
        .await;
    check_page(&page, 27, 3, 10, 7);
    assert!(
        page["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["owner"] == owner)
    );
    assert_eq!(
        f.request(
            "GET",
            &query(&path, &[("kind", "task' OR 1=1 --")]),
            Value::Null
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    let settings = json!({"preferences":{},"rules":{"origin":(0..27).map(|i|(format!("site{i:02}.test"),json!("allow"))).collect::<serde_json::Map<_,_>>()}});
    f.storage
        .save_virtual_client_state(owner, "browser_settings", &settings, None)
        .await
        .unwrap();
    let path = format!("/admin/api/consumers/{owner}/client-state/browser_settings/rows");
    check_page(
        &f.get(&query(
            &path,
            &[
                ("section", "$.rules.origin"),
                ("entries", "true"),
                ("page", "3"),
                ("page_size", "10"),
            ],
        ))
        .await,
        27,
        3,
        10,
        7,
    );
    check_page(
        &f.get(&query(&path, &[("section", "$.no_such_key' OR 1=1 --")]))
            .await,
        0,
        1,
        20,
        0,
    );
    check_page(
        &f.get(&format!(
            "/admin/api/consumers/{other}/client-state/browser_settings/rows?section=$.rules.origin"
        ))
        .await,
        0,
        1,
        20,
        0,
    );
}

#[tokio::test]
async fn grok_observed_catalog_pages_only_its_persisted_supplier_snapshot() {
    let f = Fixture::new().await;
    let mut input = codex2api_storage::NewSupplierAccount::pending_identity(
        "list-grok-installation",
        "grok-test",
        "grok-test",
        "Windows",
        "test",
        "x64",
        "",
        "{}",
    );
    input.provider_id = "grok".into();
    let account = f.storage.create_account(input).await.unwrap();
    let revision = f
        .storage
        .supplier_auth_revision(&account.id)
        .await
        .unwrap()
        .unwrap();
    let models:Vec<_>=(0..27).map(|i|json!({"id":format!("list-grok-{i:02}"),"model":format!("list-grok-{i:02}"),"api_backend":"responses"})).collect();
    assert!(
        f.storage
            .save_grok_catalog(&account.id, revision, &json!({"models":models}), &models)
            .await
            .unwrap()
    );
    let page = f
        .get(&format!(
            "/admin/api/suppliers/grok/{}/models?page=3&page_size=10",
            account.id
        ))
        .await;
    check_page(&page, 27, 3, 10, 7);
    assert_eq!(page["items"][0]["model"], "list-grok-20");
    assert!(page["observed_at"].is_string());
    assert_eq!(page["stale"], false);
}
