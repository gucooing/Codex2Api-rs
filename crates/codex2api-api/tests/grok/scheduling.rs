use super::*;

const FREE: &str = "You've used all the included free usage for model grok-4.7 for now. Usage resets over a rolling 24-hour window — tokens (actual/limit): 650951/600000. Upgrade to a Grok subscription for higher limits: https://grok.com/supergrok";

pub(super) async fn pool() -> Fixture {
    let f = fixture("grok").await;
    add_supplier(&f.storage, "grok", "supplier-two").await;
    f.storage
        .edit_supplier_tags(&["supplier-two".into()], &["tag".into()], false)
        .await
        .unwrap();
    let mut settings = f.storage.gateway_settings().await.unwrap();
    settings.scheduler_mode = "traditional".into();
    f.storage.save_gateway_settings(&settings).await.unwrap();
    f.storage
        .save_supplier_scheduling(
            "supplier-two",
            &codex2api_storage::SupplierScheduling {
                priority: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    f
}

fn attempted(f: &Fixture) -> Vec<String> {
    f.mock
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|(_, headers, _)| headers["x-userid"].to_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn grok_quota_failover_cools_supply_and_settles_only_the_successful_attempt() {
    for (status, body) in [
        (
            429,
            json!({"code":"subscription:free-usage-exhausted","error":FREE}).to_string(),
        ),
        (429, json!({"code":429,"error":FREE}).to_string()),
        (429, FREE.to_owned()),
        (
            200,
            format!(
                "data: {{\"type\":\"response.created\"}}\n\ndata: {}\n\n",
                json!({"type":"error","code":429,"error":FREE})
            ),
        ),
    ] {
        let f = pool().await;
        f.mock
            .grok_rejections
            .lock()
            .unwrap()
            .insert("supplier".into(), (status, body));
        let response = f
            .router
            .clone()
            .oneshot(request(
                "/v1/responses",
                &f.token,
                json!({"model":"custom-model","input":"hi"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json_body(response).await["status"], "completed");
        assert_eq!(attempted(&f), ["supplier", "supplier-two"]);
        let health = f.storage.supplier_health("supplier").await.unwrap();
        assert_eq!(health.cooldown_kind.as_deref(), Some("quota_exhausted"));
        assert_eq!(
            health.cooldown_code.as_deref(),
            Some("subscription:free-usage-exhausted")
        );
        assert!(health.cooldown_until.unwrap() > chrono::Utc::now().timestamp() + 100);
        assert!(!health.authentication_invalid);
        assert!(!health.payment_required);
        let rows: Vec<(String, String, Option<i64>)> =
            sqlx::query_as("SELECT account_id,status,input_tokens FROM usage_records")
                .fetch_all(f.storage.pool())
                .await
                .unwrap();
        assert_eq!(
            rows,
            [("supplier-two".into(), "completed".into(), Some(10000))]
        );
        let charged: i64 =
            sqlx::query_scalar("SELECT count(*) FROM wallet_entries WHERE kind='usage'")
                .fetch_one(f.storage.pool())
                .await
                .unwrap();
        assert_eq!(charged, 1);
    }
}

#[tokio::test]
async fn grok_non_quota_errors_keep_supply_and_retry_instructions() {
    let f = pool().await;
    for (status, code, public_status, public_code) in [
        (400, "invalid_image", 400, "invalid_image"),
        (403, "permission_denied", 503, "service_unavailable"),
        (404, "model_not_found", 404, "model_not_found"),
        (413, "payload_too_large", 413, "payload_too_large"),
        (429, "rate_limit_exceeded", 429, "rate_limit_exceeded"),
        (429, "unknown", 429, "rate_limit_exceeded"),
        (500, "server_error", 502, "upstream_error"),
        (503, "service_unavailable_error", 503, "service_unavailable"),
        (529, "overloaded_error", 502, "upstream_error"),
    ] {
        f.mock.requests.lock().unwrap().clear();
        f.mock.grok_rejections.lock().unwrap().insert(
            "supplier".into(),
            (
                status,
                json!({"error":{"code":code,"message":"rejected"}}).to_string(),
            ),
        );
        let response = f
            .router
            .clone()
            .oneshot(request(
                "/v1/responses",
                &f.token,
                json!({"model":"custom-model","input":"hi"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), public_status);
        if status == 429 {
            assert_eq!(response.headers()["retry-after"], "120");
            assert_eq!(response.headers()["x-should-retry"], "false");
        } else {
            assert!(!response.headers().contains_key("retry-after"));
            assert!(!response.headers().contains_key("x-should-retry"));
        }
        assert_eq!(json_body(response).await["error"]["code"], public_code);
        assert_eq!(attempted(&f), ["supplier"]);
        let health = f.storage.supplier_health("supplier").await.unwrap();
        assert!(!health.authentication_invalid);
        assert!(health.cooldown_until.is_none());
    }
}

#[tokio::test]
async fn grok_measured_failures_never_retry_or_lose_reported_usage() {
    let f = pool().await;
    let body = json!({"error":{"code":"subscription:free-usage-exhausted","message":FREE},"model":"custom-model","usage":{"input_tokens":10000,"output_tokens":5000,"input_tokens_details":{"cached_tokens":2000}}});
    f.mock
        .grok_rejections
        .lock()
        .unwrap()
        .insert("supplier".into(), (429, body.to_string()));
    let response = f
        .router
        .clone()
        .oneshot(request(
            "/v1/chat/completions",
            &f.token,
            json!({"model":"custom-model","messages":[{"role":"user","content":"hi"}]}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let _ = json_body(response).await;
    assert_eq!(attempted(&f), ["supplier"]);
    let rows: Vec<(String, String, Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT account_id,status,input_tokens,output_tokens FROM usage_records")
            .fetch_all(f.storage.pool())
            .await
            .unwrap();
    assert_eq!(
        rows,
        [("supplier".into(), "failed".into(), Some(10000), Some(5000))]
    );
    assert_eq!(
        f.storage
            .supplier_health("supplier")
            .await
            .unwrap()
            .cooldown_kind
            .as_deref(),
        Some("quota_exhausted")
    );
}

#[tokio::test]
async fn grok_pool_exhaustion_tries_each_supplier_once_and_releases_slots() {
    let f = pool().await;
    for id in ["supplier", "supplier-two"] {
        f.mock.grok_rejections.lock().unwrap().insert(
            id.into(),
            (
                429,
                json!({"code":"subscription:free-usage-exhausted","error":FREE}).to_string(),
            ),
        );
    }
    let response = f
        .router
        .clone()
        .oneshot(request(
            "/v1/responses",
            &f.token,
            json!({"model":"custom-model","input":"hi"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "service_unavailable"
    );
    assert_eq!(attempted(&f), ["supplier", "supplier-two"]);
    f.mock.grok_rejections.lock().unwrap().clear();
    f.storage
        .reset_supplier_availability("supplier")
        .await
        .unwrap();
    let response = f
        .router
        .clone()
        .oneshot(request(
            "/v1/responses",
            &f.token,
            json!({"model":"custom-model","input":"retry"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let _ = json_body(response).await;
}
