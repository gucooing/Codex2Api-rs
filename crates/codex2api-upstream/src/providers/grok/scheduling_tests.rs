use super::*;
use crate::{SupplierAuthRevision, UpstreamError, scheduler::inspect_response};
use serde_json::json;

const FREE: &str = "You've used all the included free usage for model grok-4.7 for now. Usage resets over a rolling 24-hour window — tokens (actual/limit): 650951/600000. Upgrade to a Grok subscription for higher limits: https://grok.com/supergrok\u{a0}";

#[test]
fn free_usage_exhaustion_is_recognized_across_grok_wire_envelopes() {
    for value in [
        json!({"code":errors::FREE_USAGE_EXHAUSTED,"error":FREE}),
        json!({"error":{"code":errors::FREE_USAGE_EXHAUSTED,"message":FREE}}),
        json!({"error":{"type":errors::FREE_USAGE_EXHAUSTED,"message":FREE}}),
        json!({"code":429,"error":FREE}),
        json!({"code":"429","error":FREE}),
        json!({"error":{"type":"rate_limit_error","code":"rate_limit_exceeded","message":FREE}}),
        json!({"error":format!("{FREE} [WKE= {} ]", errors::FREE_USAGE_EXHAUSTED)}),
        json!({"error":json!({"code":errors::FREE_USAGE_EXHAUSTED,"error":FREE}).to_string()}),
        json!({"error":{"error_code":errors::FREE_USAGE_EXHAUSTED,"description":FREE}}),
        json!([{"code":429,"error":FREE}]),
        json!(FREE),
        errors::decode(FREE.as_bytes()),
        json!({"type":"response.failed","response":{"error":{"status_code":429,"message":FREE}}}),
    ] {
        let normalized = errors::normalize_reply(&value, Some(429));
        assert_eq!(
            errors::normalize(&normalized),
            normalized,
            "normalization must be idempotent"
        );
        let failure = crate::ResponseFailure::from_error(Some(429), &json!({"error":normalized}));
        assert_eq!(failure.kind, crate::FailureKind::QuotaExhausted, "{value}");
        assert_eq!(
            crate::public_output::PublicFailure::new(&failure).status,
            503
        );
        for value in [value.clone(), json!({"error":errors::normalize(&value)})] {
            assert_eq!(
                classify_failure(Some(429), &value, &HeaderMap::new(), 100),
                Some(SupplierFailure::QuotaExhausted {
                    until: 160,
                    code: errors::FREE_USAGE_EXHAUSTED.into(),
                    model: None,
                }),
                "{value}"
            );
        }
    }
}

#[test]
fn quota_codes_and_nested_stream_status_use_the_same_recovery_policy() {
    for code in [
        errors::FREE_USAGE_EXHAUSTED,
        "personal-team-blocked:spending-limit",
        "insufficient_quota",
        "insufficient_credits",
        "billing_error",
    ] {
        for value in [
            json!({"type":"error","code":code,"message":"rejected"}),
            json!({"type":"error","error":{"type":code,"message":"rejected"}}),
            json!({"type":"response.failed","response":{"error":{"code":code,"reset_at":200}}}),
            json!({"type":"error","code":429,"error":format!("rejected [WKE={code}]")}),
        ] {
            let failure = classify_failure(None, &value, &HeaderMap::new(), 100);
            assert!(
                matches!(failure, Some(SupplierFailure::QuotaExhausted { .. })),
                "{value}: {failure:?}"
            );
        }
    }
    let stream =
        json!({"type":"response.failed","response":{"error":{"status":429,"message":FREE}}});
    assert!(matches!(
        classify_failure(None, &stream, &HeaderMap::new(), 100),
        Some(SupplierFailure::QuotaExhausted { .. })
    ));
    for value in [
        json!({"type":"error","error":{"status":401,"message":"expired"}}),
        json!({"type":"response.failed","response":{"error":{"status_code":401}}}),
        json!({"type":"error","code":"401","error":"expired"}),
    ] {
        assert_eq!(
            classify_failure(None, &value, &HeaderMap::new(), 100),
            Some(SupplierFailure::Authentication)
        );
    }
}

#[test]
fn throttling_policy_capacity_and_unknown_errors_never_evict_supply() {
    for (status, value) in [
        (
            429,
            json!({"error":{"code":"rate_limit_exceeded","message":"Retry later"}}),
        ),
        (429, json!({"error":{"code":"slow_down"}})),
        (
            429,
            json!({"error":{"code":"resource-exhausted","message":"TPM limit exceeded"}}),
        ),
        (
            429,
            json!({"error":"Quota exceeded. Upgrade to a Grok subscription"}),
        ),
        (
            429,
            json!({"error":{"code":"context_length_exceeded","message":FREE}}),
        ),
        (
            429,
            json!({"error":{"type":"invalid_request_error","message":FREE}}),
        ),
        (429, json!({"error":{"code":"unknown"}})),
        (429, Value::Null),
        (400, json!({"error":{"code":"invalid_image"}})),
        (
            400,
            json!({"error":{"code":"invalid_request_error","message":FREE}}),
        ),
        (
            403,
            json!({"error":"This model requires a Grok subscription"}),
        ),
        (403, json!({"error":{"code":"permission_denied"}})),
        (402, json!({"error":"Payment Required"})),
        (404, json!({"error":"model not found"})),
        (413, json!({"error":{"code":"payload_too_large"}})),
        (500, json!({"error":{"type":"overloaded_error"}})),
        (503, json!({"error":{"type":"service_unavailable_error"}})),
        (529, json!({"error":"Overloaded"})),
        (525, errors::decode(b"<html>insufficient_quota</html>")),
    ] {
        assert_eq!(
            classify_failure(Some(status), &value, &HeaderMap::new(), 100),
            None,
            "{status}: {value}"
        );
    }
    for status in [
        400, 403, 408, 413, 415, 422, 429, 500, 502, 503, 504, 520, 521, 522, 523, 524, 525, 526,
        529, 530,
    ] {
        assert_eq!(
            classify_failure(Some(status), &Value::Null, &HeaderMap::new(), 100),
            None
        );
    }
    let success = json!({"type":"message","message":FREE,"code":errors::FREE_USAGE_EXHAUSTED});
    assert_eq!(
        classify_failure(None, &success, &HeaderMap::new(), 100),
        None
    );
    assert_eq!(
        classify_failure(Some(200), &success, &HeaderMap::new(), 100),
        None
    );
}

#[test]
fn cooldown_uses_actual_retry_advice_without_inventing_a_daily_reset() {
    for (advice, retry, until) in [
        (json!({}), None, 160),
        (json!({"reset_at":180}), Some("20"), 180),
        (json!({"resets_at":110}), Some("90"), 190),
        (json!({"retry_after_ms":1501}), None, 102),
        (json!({"retry_after_seconds":9.1}), None, 110),
        (json!({"retry_after":0}), None, 101),
        (json!({"reset_at":99,"retry_after":-1}), Some("NaN"), 160),
        (json!({}), Some("Thu, 01 Jan 1970 00:03:00 GMT"), 180),
        (json!({"headers":{"Retry-After":"120"}}), Some("7"), 220),
        (json!({"headers":{"retry-after":13}}), None, 113),
        (json!({"headers":{"retry-after-ms":"3500"}}), None, 104),
    ] {
        let mut value = json!({"type":"error","code":errors::FREE_USAGE_EXHAUSTED,"message":FREE});
        value
            .as_object_mut()
            .unwrap()
            .extend(advice.as_object().unwrap().clone());
        let mut headers = HeaderMap::new();
        if let Some(retry) = retry {
            headers.insert("retry-after", retry.parse().unwrap());
        }
        assert_eq!(
            classify_failure(None, &value, &headers, 100),
            Some(SupplierFailure::QuotaExhausted {
                until,
                code: errors::FREE_USAGE_EXHAUSTED.into(),
                model: None,
            }),
            "{value}"
        );
    }
}

fn response(status: u16, body: String) -> reqwest::Response {
    let mut response: reqwest::Response = http::Response::builder()
        .status(status)
        .header("retry-after", "17")
        .header("x-should-retry", "false")
        .body(reqwest::Body::from(body))
        .unwrap()
        .into();
    response.extensions_mut().insert(SupplierAuthRevision(7));
    response
}

#[tokio::test]
async fn sse_and_json_rejections_keep_auth_revision_and_retry_advice() {
    for body in [
        json!({"error":{"code":errors::FREE_USAGE_EXHAUSTED,"message":FREE}}).to_string(),
        format!(
            "data: {{\"type\":\"response.created\"}}\n\nevent: error\ndata: {}\n\n",
            json!({"code":429,"error":FREE})
        ),
        format!(
            "data: {{\"object\":\"chat.completion.chunk\",\"choices\":[{{\"delta\":{{\"role\":\"assistant\"}},\"finish_reason\":null}}]}}\n\ndata: {}\n\n",
            json!({"error":{"code":errors::FREE_USAGE_EXHAUSTED}})
        ),
        format!(
            "event: ping\ndata: {{\"type\":\"ping\"}}\n\nevent: error\ndata: {}\n\n",
            json!({"error":{"type":"billing_error"}})
        ),
    ] {
        let error = inspect_response("grok", prelude, response(200, body))
            .await
            .unwrap_err();
        assert_eq!(error.auth_revision(), Some(7));
        assert!(matches!(
            error.supplier_failure_for("grok", 100),
            Some(SupplierFailure::QuotaExhausted { until: 117, .. })
        ));
    }
    let error = inspect_response(
        "grok",
        prelude,
        response(429, json!({"error":{"code":"slow_down"}}).to_string()),
    )
    .await
    .unwrap_err();
    assert_eq!(error.auth_revision(), Some(7));
    let UpstreamError::Status {
        status, headers, ..
    } = error
    else {
        panic!()
    };
    assert_eq!(status, 429);
    assert_eq!(headers["retry-after"], "17");
    assert_eq!(headers["x-should-retry"], "false");
}

#[tokio::test]
async fn generated_content_and_usage_commit_every_grok_protocol_before_failure() {
    let failure = format!(
        "data: {}\n\n",
        json!({"type":"error","error":{"code":errors::FREE_USAGE_EXHAUSTED}})
    );
    for generated in [
        json!({"type":"response.output_text.delta","delta":"partial"}),
        json!({"type":"response.reasoning_text.delta","delta":"reasoning"}),
        json!({"type":"response.failed","response":{"error":{"code":errors::FREE_USAGE_EXHAUSTED},"usage":{"input_tokens":12}}}),
        json!({"object":"chat.completion.chunk","choices":[{"delta":{"function_call":{"arguments":"{"}}}]}),
        json!({"object":"chat.completion.chunk","choices":[{"delta":{"reasoning_content":"reasoning"}}]}),
        json!({"type":"message_start","message":{"usage":{"input_tokens":12},"content":[]}}),
        json!({"type":"content_block_start","content_block":{"type":"tool_use","id":"tool"}}),
    ] {
        let body = format!("data: {generated}\n\n{failure}");
        let forwarded = inspect_response("grok", prelude, response(200, body.clone()))
            .await
            .unwrap();
        assert_eq!(forwarded.text().await.unwrap(), body);
    }
}

#[tokio::test]
async fn transient_and_unknown_stream_failures_are_forwarded_unchanged() {
    for error in [
        json!({"type":"error","error":{"type":"overloaded_error","message":"busy"}}),
        json!({"type":"error","status":429,"error":{"code":"slow_down","retry_after_seconds":5}}),
        json!({"type":"error","error":{"type":"invalid_request_error","message":"invalid image"}}),
        json!({"type":"error","error":{"code":"unknown-new-error"}}),
    ] {
        let body = format!("event: error\ndata: {error}\n\n");
        let forwarded = inspect_response("grok", prelude, response(200, body.clone()))
            .await
            .unwrap();
        assert_eq!(forwarded.text().await.unwrap(), body);
    }
}
