mod common;
use axum::http::StatusCode;
use codex2api_storage::SupplierStatus;
use common::*;
use serde_json::{Value, json};

fn account(platform: &str, user: &str) -> Value {
    let credentials = match platform {
        "openai" => {
            json!({"access_token":format!("access-{user}"),"refresh_token":format!("refresh-{user}"),
            "chatgpt_account_id":"shared-workspace","chatgpt_user_id":user,"email":format!("{user}@example.test"),
            "model_mapping":{"alias":"original-model"}})
        }
        _ => {
            json!({"access_token":format!("access-{user}"),"refresh_token":format!("refresh-{user}"),
            "sub":user,"team_id":"team-1","expires_at":2000000000,"subscription_tier":"SuperGrok"})
        }
    };
    json!({"name":user,"platform":platform,"type":"oauth","credentials":credentials,"concurrency":3,"priority":2})
}
async fn import(f: &Fixture, data: Value, format: &str, proxies: bool) -> Value {
    let response = f
        .request(
            "POST",
            "/admin/api/suppliers/import",
            json!({"format":format,"include_proxies":proxies,"data":data}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    body(response).await
}
async fn seed(f: &Fixture) -> Vec<String> {
    let result = import(f, json!({"accounts":[account("openai","alice"),account("openai","bob"),account("grok","grok-user")],"proxies":[]}), "sub2api", false).await;
    assert_eq!(result["imported"], 3);
    let rows = f.storage.list_accounts().await.unwrap();
    ["alice", "bob", "grok-user"]
        .into_iter()
        .map(|name| {
            rows.iter()
                .find(|a| a.display_name.as_deref() == Some(name))
                .unwrap()
                .id
                .clone()
        })
        .collect()
}

#[tokio::test]
async fn batch_models_replace_both_sources_atomically_and_allow_names_outside_catalog() {
    let f = Fixture::new().await;
    let ids = seed(&f).await;
    let revision = f
        .storage
        .supplier_auth_revision(&ids[0])
        .await
        .unwrap()
        .unwrap();
    f.storage
        .sync_supplier_models(&ids[0], revision, &[json!({"model":"upstream-model"})])
        .await
        .unwrap();
    f.storage.supplier_model_snapshot(&ids[0]).await.unwrap();
    let apply = json!({"account_ids":ids[..2],"models":["free-input-model","other-model","free-input-model"]});
    let result = f.request("PUT", "/admin/api/suppliers/models", apply).await;
    assert_eq!(result.status(), StatusCode::OK);
    for id in &ids[..2] {
        let models = f.storage.supplier_models(id).await.unwrap();
        assert_eq!(
            models.iter().map(|m| m.model.as_str()).collect::<Vec<_>>(),
            ["free-input-model", "other-model"]
        );
        assert!(
            models
                .iter()
                .all(|m| m.source == "custom" && m.observed_at.is_none())
        );
        assert!(
            f.storage
                .supplier_supports_model(id, "free-input-model")
                .await
                .unwrap()
        );
        assert!(
            !f.storage
                .supplier_supports_model(id, "original-model")
                .await
                .unwrap()
        );
    }
    assert!(
        !f.storage
            .model_configs()
            .await
            .unwrap()
            .iter()
            .any(|m| m.model == "free-input-model")
    );
    for invalid in [
        json!({"account_ids":ids,"models":["cross-platform"]}),
        json!({"account_ids":[ids[0],"missing"],"models":[]}),
        json!({"account_ids":ids[..2],"models":["bad model"]}),
    ] {
        assert_ne!(
            f.request("PUT", "/admin/api/suppliers/models", invalid)
                .await
                .status(),
            StatusCode::OK
        );
        assert!(
            f.storage
                .supplier_supports_model(&ids[0], "free-input-model")
                .await
                .unwrap()
        );
    }
    assert_eq!(
        f.request(
            "POST",
            &format!("/admin/api/suppliers/{}/models", ids[0]),
            json!({"model":"typed-model"})
        )
        .await
        .status(),
        StatusCode::OK
    );
    let options = f
        .get(&format!(
            "/admin/api/suppliers/{}/models/options?search=typed",
            ids[0]
        ))
        .await;
    assert_eq!(options["items"][0]["model"], "typed-model");
    assert_eq!(options["selected"].as_array().unwrap().len(), 3);
    assert_eq!(
        f.request(
            "PUT",
            "/admin/api/suppliers/models",
            json!({"account_ids":ids[..2],"models":[]})
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert!(f.storage.supplier_models(&ids[0]).await.unwrap().is_empty());
    f.storage.close().await;
}

#[tokio::test]
async fn sub2api_import_skips_unsupported_types_preserves_identity_and_never_echoes_secrets() {
    let f = Fixture::new().await;
    let mut unsupported = account("openai", "api-key");
    unsupported["type"] = "apikey".into();
    let result = import(&f, json!({"type":"sub2api-data","version":1,"accounts":[
        account("openai","alice"),account("openai","bob"),account("grok","grok-user"),
        {"platform":"anthropic","credentials":{"email":"skipped@example.test","secret":"not-for-output"}}, unsupported,
        {"platform":"openai","type":"oauth","name":"invalid","credentials":{"email":"failed@example.test"},"concurrency":3,"priority":2}
    ],"proxies":[]}), "sub2api", true).await;
    assert_eq!(result["imported"], 3);
    assert_eq!(result["skipped"], 2);
    assert_eq!(result["failed"], 1);
    assert_eq!(result["results"][0]["name"], "alice@example.test");
    assert_eq!(result["results"][3]["name"], "skipped@example.test");
    assert_eq!(result["results"][5]["name"], "failed@example.test");
    assert!(!result.to_string().contains("refresh-alice"));
    assert!(!result.to_string().contains("access-alice"));
    let rows = f.storage.list_accounts().await.unwrap();
    assert_eq!(rows.len(), 3);
    let alice = rows
        .iter()
        .find(|a| a.display_name.as_deref() == Some("alice"))
        .unwrap();
    let bob = rows
        .iter()
        .find(|a| a.display_name.as_deref() == Some("bob"))
        .unwrap();
    assert_ne!(alice.installation_id, bob.installation_id);
    let grok = rows.iter().find(|a| a.provider_id == "grok").unwrap();
    assert_eq!(grok.chatgpt_account_id.as_deref(), Some("team:team-1"));
    let tokens = f
        .storage
        .load_supplier_tokens(&grok.id)
        .await
        .unwrap()
        .unwrap();
    let auth = codex2api_suppliers::providers::grok::GrokCredentials::from_supplier_tokens(&tokens)
        .unwrap();
    assert_eq!(auth.expires_at, Some(2000000000));
    assert_eq!(auth.profile["userId"], "grok-user");
    f.storage
        .set_account_status(&alice.id, SupplierStatus::Disabled)
        .await
        .unwrap();
    let result = import(
        &f,
        json!({"accounts":[account("openai","alice")]}),
        "sub2api",
        false,
    )
    .await;
    assert_eq!(result["skipped"], 1);
    let unchanged = f.storage.require_account(&alice.id).await.unwrap();
    assert_eq!(unchanged.status, SupplierStatus::Disabled);
    assert_eq!(unchanged.installation_id, alice.installation_id);
    f.storage.close().await;
}

#[tokio::test]
async fn native_json_roundtrip_preserves_models_credentials_fingerprints_and_optional_proxy() {
    let f = Fixture::new().await;
    let mut source = account("openai", "alice");
    source["proxy_key"] = "proxy-1".into();
    let data = json!({"accounts":[source,account("grok","grok-user")],"proxies":[{
        "proxy_key":"proxy-1","name":"test proxy","protocol":"socks5h","host":"127.0.0.1","port":1099,
        "username":"user@name","password":"proxy-password:@%?#"}]});
    assert_eq!(import(&f, data, "sub2api", true).await["imported"], 2);
    for source in f.storage.list_accounts().await.unwrap() {
        let tag = format!("tag-{}", source.provider_id);
        f.storage
            .save_supplier_tag(&tag, &source.provider_id, "portable tag")
            .await
            .unwrap();
        f.storage
            .replace_supplier_tags(std::slice::from_ref(&source.id), &[tag])
            .await
            .unwrap();
        let mut scheduling = f.storage.supplier_scheduling(&source.id).await.unwrap();
        scheduling.load_factor = Some(7);
        assert!(
            f.storage
                .save_supplier_scheduling(&source.id, &scheduling)
                .await
                .unwrap()
        );
        f.storage
            .set_account_status(&source.id, SupplierStatus::Disabled)
            .await
            .unwrap();
    }
    let rows = f.storage.list_accounts().await.unwrap();
    let ids = rows.iter().map(|a| a.id.clone()).collect::<Vec<_>>();
    let with_proxy = body(
        f.request(
            "POST",
            "/admin/api/suppliers/export",
            json!({"account_ids":ids,"include_proxies":true}),
        )
        .await,
    )
    .await;
    let without = body(
        f.request(
            "POST",
            "/admin/api/suppliers/export",
            json!({"account_ids":ids,"include_proxies":false}),
        )
        .await,
    )
    .await;
    assert_eq!(with_proxy["type"], "codex2api-suppliers");
    assert!(!without.to_string().contains("proxy-password"));
    assert!(
        !without["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a.get("proxy").is_some())
    );
    let target = Fixture::new().await;
    let result = import(&target, with_proxy.clone(), "native", true).await;
    assert_eq!(result["imported"], 2, "{result}");
    assert!(
        result["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["name"] == "alice@example.test")
    );
    for source in rows {
        let restored = target.storage.require_account(&source.id).await.unwrap();
        assert_eq!(restored.installation_id, source.installation_id);
        assert_eq!(restored.http_fingerprint_json, source.http_fingerprint_json);
        assert_eq!(restored.status, source.status);
        assert_eq!(restored.status, SupplierStatus::Disabled);
        assert_eq!(
            target
                .storage
                .supplier_scheduling(&source.id)
                .await
                .unwrap()
                .load_factor,
            Some(7)
        );
        assert_eq!(
            target
                .storage
                .supplier_tag_labels(&source.id)
                .await
                .unwrap()
                .into_iter()
                .map(|(_, name)| name)
                .collect::<Vec<_>>(),
            ["portable tag"]
        );
        assert_eq!(restored.chatgpt_user_id, source.chatgpt_user_id);
        assert_eq!(
            target
                .storage
                .supplier_scheduling(&source.id)
                .await
                .unwrap()
                .concurrency,
            3
        );
        assert_eq!(
            target
                .storage
                .load_supplier_tokens(&source.id)
                .await
                .unwrap()
                .unwrap()
                .access_token,
            f.storage
                .load_supplier_tokens(&source.id)
                .await
                .unwrap()
                .unwrap()
                .access_token
        );
        if let Some(proxy_id) = source.proxy_id {
            let expected = f.storage.require_outbound_proxy(&proxy_id).await.unwrap();
            let actual = target
                .storage
                .require_outbound_proxy(restored.proxy_id.as_deref().unwrap())
                .await
                .unwrap();
            assert_eq!(actual.url, expected.url);
            let url = url::Url::parse(&actual.url).unwrap();
            assert_eq!(urlencoding::decode(url.username()).unwrap(), "user@name");
            assert_eq!(
                urlencoding::decode(url.password().unwrap()).unwrap(),
                "proxy-password:@%?#"
            );
            assert!(
                target
                    .storage
                    .supplier_supports_model(&source.id, "original-model")
                    .await
                    .unwrap()
            );
        }
    }
    assert_eq!(
        import(&target, with_proxy.clone(), "native", true).await["skipped"],
        2
    );
    let no_proxy = Fixture::new().await;
    assert_eq!(
        import(&no_proxy, with_proxy, "native", false).await["imported"],
        2
    );
    assert!(
        no_proxy
            .storage
            .list_outbound_proxies()
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        no_proxy
            .storage
            .list_accounts()
            .await
            .unwrap()
            .iter()
            .all(|a| a.proxy_id.is_none())
    );
    f.storage.close().await;
    target.storage.close().await;
    no_proxy.storage.close().await;
}

#[tokio::test]
async fn imports_are_admin_only_and_failed_rows_cannot_leave_proxies_or_partial_accounts() {
    let f = Fixture::new().await;
    assert_eq!(
        f.with_auth(
            "POST",
            "/admin/api/suppliers/export",
            json!({"account_ids":["x"],"include_proxies":true}),
            None,
            None
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.with_auth("POST", "/admin/api/suppliers/import", json!({}), None, None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let mut invalid = account("openai", "bad");
    invalid["proxy_key"] = "p".into();
    invalid["priority"] = 1000001.into();
    let result = import(&f, json!({"accounts":[invalid,account("openai","valid")],"proxies":[
        {"proxy_key":"p","name":"should not exist","protocol":"http","host":"127.0.0.1","port":1099}
    ]}), "sub2api", true).await;
    assert_eq!(result["failed"], 1);
    assert_eq!(result["imported"], 1);
    assert!(f.storage.list_outbound_proxies().await.unwrap().is_empty());
    assert_eq!(f.storage.list_accounts().await.unwrap().len(), 1);
    assert_eq!(f.request("POST", "/admin/api/suppliers/import", json!({"format":"native","include_proxies":false,"data":{"type":"sub2api-data","accounts":[]}})).await.status(), StatusCode::BAD_REQUEST);
    f.storage.close().await;
}

#[tokio::test]
async fn sub2api_export_matches_its_account_and_proxy_format_and_can_be_imported_again() {
    let f = Fixture::new().await;
    let mut rows = vec![account("openai", "alice"), account("grok", "grok-user")];
    for row in &mut rows {
        row["proxy_key"] = "p".into();
    }
    assert_eq!(import(&f, json!({"accounts":rows,"proxies":[{"proxy_key":"p","name":"shared proxy",
        "protocol":"https","host":"127.0.0.1","port":8443,"username":"user@name","password":"pw:@%?#"}]}), "sub2api", true).await["imported"], 2);
    let ids = f
        .storage
        .list_accounts()
        .await
        .unwrap()
        .into_iter()
        .map(|a| a.id)
        .collect::<Vec<_>>();
    let response = f
        .request(
            "POST",
            "/admin/api/suppliers/export",
            json!({"format":"sub2api","account_ids":ids,"include_proxies":true}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let data = body(response).await;
    assert_eq!(data["type"], "sub2api-data");
    assert_eq!(data["version"], 1);
    assert_eq!(data["proxies"].as_array().unwrap().len(), 1);
    assert_eq!(data["proxies"][0]["username"], "user@name");
    assert_eq!(data["proxies"][0]["password"], "pw:@%?#");
    for row in data["accounts"].as_array().unwrap() {
        assert_eq!(row["type"], "oauth");
        assert_eq!(row["proxy_key"], data["proxies"][0]["proxy_key"]);
        assert_eq!(row["concurrency"], 3);
        assert!(
            row["credentials"]["access_token"]
                .as_str()
                .unwrap()
                .starts_with("access-")
        );
    }
    let chatgpt = data["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["platform"] == "openai")
        .unwrap();
    assert_eq!(chatgpt["credentials"]["chatgpt_user_id"], "alice");
    assert_eq!(
        chatgpt["credentials"]["model_mapping"]["original-model"],
        "original-model"
    );
    let target = Fixture::new().await;
    assert_eq!(import(&target, data, "sub2api", true).await["imported"], 2);
    assert_eq!(
        target.storage.list_outbound_proxies().await.unwrap().len(),
        1
    );
    let without = body(
        f.request(
            "POST",
            "/admin/api/suppliers/export",
            json!({"format":"sub2api","account_ids":ids,"include_proxies":false}),
        )
        .await,
    )
    .await;
    assert_eq!(without["proxies"], json!([]));
    assert!(!without.to_string().contains("pw:"));
    assert!(
        without["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a.get("proxy_key").is_none())
    );
    f.storage.close().await;
    target.storage.close().await;
}
