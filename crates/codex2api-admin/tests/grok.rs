mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::json;

#[tokio::test]
async fn subscription_descriptions_and_available_models_follow_saved_plan_policy() {
    let f = Fixture::new().await;
    for (provider, model, enabled) in [
        ("grok", "custom-enabled", true),
        ("grok", "custom-disabled", false),
        ("chatgpt", "other-platform", true),
    ] {
        let mut input = model_input(model);
        input["provider_id"] = provider.into();
        input["enabled"] = enabled.into();
        assert_eq!(
            f.request("POST", "/admin/api/models", input).await.status(),
            StatusCode::OK
        );
    }
    let description = "## 本地套餐\n\n**独立描述**\n\n- 自选模型\n- 按需使用";
    let mut first = None;
    for tier in [
        "x_basic",
        "x_premium",
        "x_premium_plus",
        "supergrok_lite",
        "supergrok",
        "supergrok_plus",
        "supergrok_heavy",
    ] {
        let mut input = plan_input();
        input["provider_id"] = "grok".into();
        input["plan_type"] = tier.into();
        input["name"] = format!("本地套餐 {tier}").into();
        input["description"] = description.into();
        input["sale_price_usd"] = "15.00".into();
        let response = f.request("POST", "/admin/api/plans", input).await;
        assert_eq!(response.status(), StatusCode::OK, "{tier}");
        let saved = body(response).await;
        assert_eq!(saved["description"], description);
        if first.is_none() {
            first = Some(saved);
        }
    }
    let saved = first.unwrap();
    let id = saved["id"].as_str().unwrap();
    let catalog = f.storage.user_store().plans().await.unwrap();
    let plan = catalog.iter().find(|p| p["id"] == id).unwrap();
    assert_eq!(plan["name"], "本地套餐 x_basic");
    assert_eq!(plan["description"], description);
    let models = plan["models"].as_array().unwrap();
    assert!(models.iter().any(|m| m["model"] == "custom-enabled"));
    assert!(
        models
            .iter()
            .all(|m| m["provider_id"] == "grok" && m["model"] != "custom-disabled")
    );
    assert!(plan.get("supplier_tag_id").is_none());

    let mut input = plan_input();
    input["provider_id"] = "grok".into();
    input["plan_type"] = "x_basic".into();
    input["revision"] = saved["revision"].clone();
    input["model_access"] = "selected".into();
    input["models"] = json!([{"provider_id":"grok","model":"custom-enabled"}]);
    let updated = f
        .request("PUT", &format!("/admin/api/plans/{id}"), input.clone())
        .await;
    assert_eq!(updated.status(), StatusCode::OK);
    let updated = body(updated).await;
    assert_eq!(
        updated["description"], description,
        "older writes preserve the description"
    );
    let catalog = f.storage.user_store().plans().await.unwrap();
    let plan = catalog.iter().find(|p| p["id"] == id).unwrap();
    assert_eq!(
        plan["models"],
        json!([{"provider_id":"grok","model":"custom-enabled","kind":"text"}])
    );

    input["revision"] = updated["revision"].clone();
    input["description"] = "x".repeat(20_001).into();
    assert_eq!(
        f.request("PUT", &format!("/admin/api/plans/{id}"), input.clone())
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    input["description"] = "".into();
    input["model_access"] = "none".into();
    input["models"] = json!([]);
    assert_eq!(
        f.request("PUT", &format!("/admin/api/plans/{id}"), input)
            .await
            .status(),
        StatusCode::OK
    );
    let catalog = f.storage.user_store().plans().await.unwrap();
    let plan = catalog.iter().find(|p| p["id"] == id).unwrap();
    assert_eq!(plan["description"], "");
    assert_eq!(plan["models"], json!([]));
}

#[tokio::test]
async fn presets_and_custom_models_are_equal_catalog_entries_and_grok_plans_are_assignable() {
    let f = Fixture::new().await;
    let presets = f.get("/admin/api/models/presets").await;
    let grok = presets["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["provider_id"] == "grok" && m["model"] == "grok-4.7")
        .unwrap();
    assert_eq!(grok["source_url"], "https://docs.x.ai/developers/pricing");
    assert_eq!(grok["token_prices"][1]["min_input_tokens"], 200000);
    assert_eq!(f.request("POST","/admin/api/models",json!({"provider_id":"grok","model":"grok-4.7","kind":"text","enabled":true,"revision":null,"pricing_preset":grok["version"]})).await.status(),StatusCode::OK);
    for provider in ["chatgpt", "grok"] {
        let mut input = model_input("operator-model-not-in-any-preset");
        input["provider_id"] = provider.into();
        assert_eq!(
            f.request("POST", "/admin/api/models", input).await.status(),
            StatusCode::OK
        );
    }
    let models = f.get("/admin/api/models").await;
    for item in models["items"].as_array().unwrap() {
        assert!(item.get("client_metadata_status").is_none());
        assert!(item.get("codex_metadata_status").is_none());
    }
    let mut plan = plan_input();
    plan["provider_id"] = "grok".into();
    plan["plan_type"] = "supergrok".into();
    plan["sale_price_usd"] = "15.99".into();
    plan["model_access"] = "selected".into();
    plan["models"] = json!([{"provider_id":"grok","model":"operator-model-not-in-any-preset"}]);
    let saved = f.request("POST", "/admin/api/plans", plan).await;
    assert_eq!(saved.status(), StatusCode::OK);
    let saved = body(saved).await;
    let user=body(f.request("POST","/admin/api/users",json!({"username":"grok-customer","password":"fixture-password","name":"Customer","email":"customer@example.test","enabled":true,"revision":null})).await).await;
    let subs = f.get("/admin/api/subscriptions").await;
    let owned = subs["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["user_id"] == user["id"])
        .collect::<Vec<_>>();
    assert_eq!(owned.len(), 2);
    let grok_sub = owned.iter().find(|s| s["provider_id"] == "grok").unwrap();
    let id = grok_sub["virtual_account_id"].as_str().unwrap();
    assert_eq!(f.request("PUT",&format!("/admin/api/subscriptions/{id}"),json!({"user_id":user["id"],"plan_id":saved["id"],"expires_at":"2099-01-01T00:00:00Z","enabled":true,"reissue":false,"revision":grok_sub["revision"]})).await.status(),StatusCode::OK);
    let effective = f
        .storage
        .effective_platform_account(id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(effective.plan_type, "supergrok");
    assert!(
        f.storage
            .available_virtual_models(id, "grok")
            .await
            .unwrap()
            .iter()
            .any(|m| m.model == "operator-model-not-in-any-preset")
    );
    assert!(
        f.storage
            .available_virtual_models(id, "chatgpt")
            .await
            .unwrap()
            .is_empty()
    );
}
