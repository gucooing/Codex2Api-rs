mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::json;

#[tokio::test]
async fn presets_create_complete_prices_and_reject_stale_or_ambiguous_writes() {
    let f = Fixture::new().await;
    let catalog = f.get("/admin/api/models/presets").await;
    let items = catalog["items"].as_array().unwrap();
    let preset = items.iter().find(|p| p["model"] == "gpt-6.1-sol").unwrap();
    assert_eq!(preset["token_prices"].as_array().unwrap().len(), 6);
    assert!(
        preset["source_url"]
            .as_str()
            .unwrap()
            .starts_with("https://developers.openai.com/")
    );
    assert!(!items.iter().any(|p| p["model"] == "codex-auto-review"));
    let legacy = items.iter().find(|p| p["model"] == "gpt-5.5").unwrap();
    let fast = legacy["token_prices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["tier"] == "fast")
        .unwrap();
    assert_eq!(fast["input_rate"], "12.5");
    assert_eq!(fast["cache_write_rate"], "12.5");
    assert_eq!(fast["max_input_tokens"], 272_000);
    let input = json!({"provider_id":"chatgpt","model":"gpt-6.1-sol","kind":"text","enabled":true,"revision":null,"pricing_preset":preset["version"]});
    let mut stale = input.clone();
    stale["pricing_preset"] = "old-preset".into();
    assert_eq!(
        f.request("POST", "/admin/api/models", stale).await.status(),
        StatusCode::CONFLICT
    );
    let mut ambiguous = input.clone();
    ambiguous["token_prices"] = model_input("gpt-6.1-sol")["token_prices"].clone();
    assert_eq!(
        f.request("POST", "/admin/api/models", ambiguous)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let mut unavailable = input.clone();
    unavailable["model"] = "unknown-without-preset".into();
    assert_eq!(
        f.request("POST", "/admin/api/models", unavailable)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        f.request("POST", "/admin/api/models", input).await.status(),
        StatusCode::OK
    );
    let saved = f
        .storage
        .model_prices("chatgpt")
        .await
        .unwrap()
        .into_iter()
        .filter(|p| p.model == "gpt-6.1-sol")
        .collect::<Vec<_>>();
    assert_eq!(saved.len(), 6);
    assert!(saved.iter().all(|p| p.source.starts_with("preset:")));
    let rows = f.get("/admin/api/models").await;
    let model = rows["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["model"] == "gpt-6.1-sol")
        .unwrap();
    assert_eq!(model["enabled"], true);
    assert_eq!(model["token_prices"].as_array().unwrap().len(), 6);
}
#[tokio::test]
async fn model_prices_and_lifecycle_share_revision_checked_persisted_configuration() {
    let f = Fixture::new().await;
    let input = model_input("test-model");
    let r = f.request("POST", "/admin/api/models", input.clone()).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(
        f.request("POST", "/admin/api/models", input.clone())
            .await
            .status(),
        StatusCode::CONFLICT
    );
    let models = f.get("/admin/api/models").await;
    let item = models["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["model"] == "test-model")
        .unwrap();
    assert_eq!(item["token_prices"][0]["input_rate"], "2.5");
    let revision = item["revision"].as_i64().unwrap();
    let mut changed = input;
    changed["revision"] = revision.into();
    changed["token_prices"][0]["input_rate"] = "-1".into();
    assert_eq!(
        f.request("POST", "/admin/api/models", changed.clone())
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    changed["token_prices"][0]["input_rate"] = "7.125".into();
    assert_eq!(
        f.request("POST", "/admin/api/models", changed.clone())
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        f.request("POST", "/admin/api/models", changed)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    let price = f
        .storage
        .model_prices("chatgpt")
        .await
        .unwrap()
        .into_iter()
        .find(|v| v.model == "test-model")
        .unwrap();
    assert_eq!(price.input_rate, 7_125_000);
    assert_eq!(f.request("POST","/admin/api/models/status",json!({"provider_id":"chatgpt","model":"test-model","revision":revision+1,"enabled":false})).await.status(),StatusCode::OK);
    assert_eq!(
        f.request(
            "POST",
            "/admin/api/models/delete",
            json!({"provider_id":"chatgpt","model":"test-model","revision":revision+2})
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert!(
        !f.storage
            .model_configs("chatgpt")
            .await
            .unwrap()
            .iter()
            .any(|m| m.model == "test-model")
    );
    let mut grok = model_input("grok");
    grok["provider_id"] = "unsupported-provider".into();
    assert_eq!(
        f.request("POST", "/admin/api/models", grok).await.status(),
        StatusCode::BAD_REQUEST
    );
}
