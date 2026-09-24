mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::json;
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
    assert_eq!(item["codex_metadata_status"], "unavailable");
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
    grok["provider_id"] = "grok".into();
    assert_eq!(
        f.request("POST", "/admin/api/models", grok).await.status(),
        StatusCode::BAD_REQUEST
    );
}
