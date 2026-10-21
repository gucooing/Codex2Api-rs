use super::*;
use codex2api_common::version::antigravity as wire;

const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aWQAAAABJRU5ErkJggg==";

#[tokio::test]
async fn antigravity_images_use_native_wire_and_settle_only_delivered_images() {
    for edit in [false, true] {
        let fixture = fixture("antigravity").await;
        fixture
            .storage
            .save_model_config(
                &ModelConfig {
                    platform: "google".into(),
                    model: "custom-image".into(),
                    kind: "image".into(),
                    enabled: true,
                    deleted: false,
                    revision: 0,
                },
                &[],
                &[codex2api_storage::ImagePrice {
                    model: "custom-image".into(),
                    resolution: "1K".into(),
                    price_nano_usd: 20_000_000,
                }],
                None,
            )
            .await
            .unwrap();
        fixture
            .storage
            .add_supplier_model("supplier", "custom-image")
            .await
            .unwrap();
        let mut group = fixture.storage.api_group("group").await.unwrap().unwrap();
        group
            .models
            .push(codex2api_common::ModelRule::granted("custom-image"));
        fixture
            .storage
            .save_api_group(&group, Some(group.revision))
            .await
            .unwrap();
        let raw = json!({"project":"private-project","response":{
            "candidates":[{"finishReason":"STOP","content":{"parts":[
                {"thought":true,"inlineData":{"mimeType":"image/png","data":PNG}},
                {"inlineData":{"mimeType":"image/png","data":PNG}}]}}],
            "usageMetadata":{"promptTokenCount":100,"cachedContentTokenCount":40,
                "candidatesTokenCount":20,"thoughtsTokenCount":5}}});
        *fixture.mock.antigravity_reply.lock().unwrap() = Some((200, raw.to_string()));
        let path = if edit {
            "/v1/images/edits"
        } else {
            "/v1/images/generations"
        };
        let mut body = json!({"model":"custom-image","prompt":"Draw a tree","n":1,"response_format":"b64_json"});
        if edit {
            body["image"] = format!("data:image/png;base64,{PNG}").into();
        }
        let reply = fixture
            .router
            .clone()
            .oneshot(request(path, &fixture.token, body.clone()))
            .await
            .unwrap();
        let status = reply.status();
        let result = json_body(reply).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["data"].as_array().unwrap().len(), 1);
        assert_eq!(result["data"][0]["b64_json"], PNG);
        assert!(result.get("project").is_none());
        let row: (Option<i64>,Option<i64>,Option<i64>,Option<i64>,Option<i64>,Option<String>) = sqlx::query_as(
            "SELECT image_count,cost_nano_usd,input_tokens,output_tokens,cached_tokens,upstream_endpoint FROM usage_records"
        ).fetch_one(fixture.storage.pool()).await.unwrap();
        assert_eq!(
            row,
            (
                Some(1),
                Some(30_000_000),
                Some(100),
                Some(25),
                Some(40),
                Some(wire::PATH_GENERATE.into())
            )
        );
        assert_eq!(
            fixture
                .storage
                .user("user")
                .await
                .unwrap()
                .unwrap()
                .wallet_cents,
            97
        );
        {
            let requests = fixture.mock.requests.lock().unwrap();
            let (path, headers, envelope) = &requests[0];
            assert_eq!(path, wire::PATH_GENERATE);
            assert_eq!(headers["authorization"], "Bearer fixture-supplier");
            assert_eq!(envelope["requestType"], "image_gen");
            assert!(
                envelope["requestId"]
                    .as_str()
                    .unwrap()
                    .starts_with("image_gen/")
            );
            assert_eq!(envelope["request"]["generationConfig"]["candidateCount"], 1);
            if edit {
                assert_eq!(
                    envelope["request"]["contents"][0]["parts"][1]["inlineData"]["data"],
                    PNG
                );
            }
        }
        // A valid JSON response without an image is not a successful paid generation.
        *fixture.mock.antigravity_reply.lock().unwrap() = Some((200,
            json!({"response":{"candidates":[{"finishReason":"STOP","content":{"parts":[{"text":"no image"}]}}]}}).to_string()));
        let failed = fixture
            .router
            .clone()
            .oneshot(request(path, &fixture.token, body.clone()))
            .await
            .unwrap();
        assert_eq!(failed.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(
            json_body(failed).await["error"]["code"],
            "upstream_protocol_error"
        );
        assert_eq!(
            fixture
                .storage
                .user("user")
                .await
                .unwrap()
                .unwrap()
                .wallet_cents,
            97
        );
        body["n"] = 2.into();
        let invalid = fixture
            .router
            .clone()
            .oneshot(request(path, &fixture.token, body))
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
        assert_eq!(fixture.mock.requests.lock().unwrap().len(), 2);
    }
}

struct CatalogTransport {
    catalog: Value,
    summary: Mutex<Value>,
    calls: Mutex<Vec<String>>,
}
impl codex2api_upstream::HttpTransport for CatalogTransport {
    fn send(&self, builder: reqwest::RequestBuilder) -> codex2api_upstream::HttpFuture {
        let request = builder.build().unwrap();
        let path = request.url().path().to_owned();
        let reply = match path.as_str() {
            wire::PATH_FETCH_MODELS => self.catalog.clone(),
            wire::PATH_QUOTA_SUMMARY => self.summary.lock().unwrap().clone(),
            _ => panic!("unexpected request {path}"),
        };
        self.calls.lock().unwrap().push(path);
        assert!(
            request.headers()["user-agent"]
                .to_str()
                .unwrap()
                .starts_with("antigravity/hub/2.24.0 (")
        );
        assert_eq!(
            serde_json::from_slice::<Value>(request.body().unwrap().as_bytes().unwrap()).unwrap(),
            json!({"project":"fixture-project"})
        );
        Box::pin(async move {
            Ok(http::Response::builder()
                .header("content-type", "application/json")
                .body(reqwest::Body::from(reply.to_string()))
                .unwrap()
                .into())
        })
    }
}

#[tokio::test]
async fn antigravity_catalog_keeps_argon_images_and_quota_refresh_is_complete_before_recovery() {
    let fixture = fixture("antigravity").await;
    let transport = Arc::new(CatalogTransport {
        catalog: json!({"models":{
            "gemini-4-argon-high":{"displayName":"Gemini 4 Argon (High)","quotaInfo":{"remainingFraction":0.9}},
            "gemini-4-argon-medium":{"displayName":"Gemini 4 Argon (Medium)","quotaInfo":{"remainingFraction":0.9}},
            "gemini-4-argon-low":{"displayName":"Gemini 4 Argon (Low)","quotaInfo":{"remainingFraction":0.9}},
            "gemini-3.1-flash-image":{"displayName":"Gemini 3.1 Flash Image"},
            "custom-supplier-model":{"displayName":"Custom Supplier Model"}
        },"imageGenerationModelIds":["gemini-3.1-flash-image"]}),
        summary: Mutex::new(
            json!({"groups":[{"displayName":"Gemini 4 Argon","buckets":[{
                "bucketId":"gemini-argon-wtu-weekly","window":"weekly","remainingFraction":0.9
            }]}]}),
        ),
        calls: Mutex::default(),
    });
    let upstream = codex2api_upstream::antigravity::AntigravityUpstream::new(
        codex2api_suppliers::antigravity::AntigravityAuthService::new(Some(
            fixture.storage.clone(),
        )),
    )
    .with_transport(transport.clone());
    upstream.discover_supplier_models("supplier").await.unwrap();
    let models = fixture.storage.supplier_models("supplier").await.unwrap();
    assert!(
        models
            .iter()
            .any(|model| model.model == "gemini-3.1-flash-image")
    );
    assert!(
        models
            .iter()
            .any(|model| model.model == "custom-supplier-model")
    );
    assert!(
        fixture
            .storage
            .model_config("custom-supplier-model")
            .await
            .unwrap()
            .is_none()
    );
    let descriptor = fixture
        .storage
        .supplier_model_descriptor("supplier", "gemini-4-argon")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(descriptor["efforts"].as_object().unwrap().len(), 3);
    let revision = fixture
        .storage
        .supplier_auth_revision("supplier")
        .await
        .unwrap()
        .unwrap();
    fixture
        .storage
        .block_supplier(
            "supplier",
            revision,
            &codex2api_storage::BlockRequest {
                model: Some("gemini-4-argon".into()),
                kind: codex2api_storage::BlockKind::Quota,
                until_at: Some(chrono::Utc::now().timestamp() + 3600),
                code: None,
                message: None,
            },
        )
        .await
        .unwrap();
    let summary = transport.summary.lock().unwrap().clone();
    *transport.summary.lock().unwrap() = json!({"groups":[{"buckets":[{"remainingFraction":-1}]}]});
    assert!(upstream.quota("supplier").await.is_err());
    assert_eq!(
        fixture
            .storage
            .supplier_health("supplier")
            .await
            .unwrap()
            .model_blocks()
            .count(),
        1
    );
    *transport.summary.lock().unwrap() = summary;
    let quota = upstream.quota("supplier").await.unwrap();
    assert_eq!(
        codex2api_upstream::antigravity::quota_groups(&quota)[0].name,
        "Gemini 4 Argon"
    );
    assert_eq!(
        fixture
            .storage
            .supplier_health("supplier")
            .await
            .unwrap()
            .model_blocks()
            .count(),
        0
    );
}
