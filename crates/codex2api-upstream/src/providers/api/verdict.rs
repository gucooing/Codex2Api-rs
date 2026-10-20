//! The API-key channel's judgement of OpenAI-style and Anthropic-style API failures,
//! following sub2api's switching policy: credential and billing rejections are
//! permanent, permission failures, wrong endpoints and overloads block briefly, quota
//! rejections block until the vendor's retry advice, an unknown model blocks that
//! model, and every server-side failure switches without persisting. Statuses the
//! administrator listed retry the same account first, as many times as the account
//! configures.
use crate::supply::{
    AvoidScope, AvoidTarget, TransportFault, UpstreamFailure, Verdict, error_fields, retry_after,
};
use codex2api_storage::BlockKind;
use std::time::Duration;

const QUOTA_CODES: [&str; 6] = [
    "insufficient_quota",
    "quota_exceeded",
    "usage_limit_reached",
    "credit_balance_exhausted",
    "organization_spend_limit_exceeded",
    "project_spend_limit_exceeded",
];

pub(crate) fn verdict(
    failure: &UpstreamFailure,
    retry_statuses: &[u16],
    retry_count: u32,
) -> Verdict {
    let now = failure.now;
    if let Some(fault) = failure.transport {
        return match fault {
            TransportFault::Connect => Verdict::avoid(
                AvoidScope::Until(now.saturating_add(600)),
                AvoidTarget::Account,
                BlockKind::Transport,
                Some("upstream_connection_failed"),
                None,
            ),
            TransportFault::Timeout => Verdict::avoid_request(),
            TransportFault::Stream => {
                Verdict::retry_same(Duration::from_millis(500), 1, Verdict::avoid_request())
            }
        };
    }
    let parsed = crate::ResponseFailure::from_error(failure.status, &failure.body);
    let status = failure.status.or(parsed.status);
    if failure.output_committed {
        return Verdict::Stop {
            status: status.unwrap_or(502),
            body: failure.body.clone(),
        };
    }
    let (code, kind, message) = error_fields(&failure.body);
    let lower = message.to_ascii_lowercase();
    let code_or_kind = if code.is_empty() {
        kind.as_str()
    } else {
        code.as_str()
    };
    let diagnostic = Some(code_or_kind).filter(|c| !c.is_empty());
    let text = Some(parsed.message.as_deref().unwrap_or_default()).filter(|m| !m.is_empty());
    let quota = QUOTA_CODES.contains(&code.as_str())
        || lower.contains("exceeded your current quota")
        || lower.contains("credit balance");
    let model_missing = code == "model_not_found"
        || (kind == "not_found_error" && lower.contains("model"))
        || (status == Some(404) && lower.contains("model"));
    let model_target = || {
        failure
            .model
            .clone()
            .map_or(AvoidTarget::Account, AvoidTarget::Model)
    };
    let advice = retry_after(&failure.headers, now);
    let base = match status {
        Some(401) => Verdict::avoid(
            AvoidScope::Permanent,
            AvoidTarget::Account,
            BlockKind::Auth,
            diagnostic,
            text,
        ),
        Some(402) => Verdict::avoid(
            AvoidScope::Permanent,
            AvoidTarget::Account,
            BlockKind::Billing,
            diagnostic,
            text,
        ),
        Some(403) => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(600)),
            AvoidTarget::Account,
            BlockKind::Penalty,
            diagnostic,
            text,
        ),
        Some(404) if model_missing => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(3600)),
            model_target(),
            BlockKind::ModelMissing,
            diagnostic,
            text,
        ),
        // The endpoint does not exist under the Base URL: the account is misconfigured.
        Some(404 | 405) => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(600)),
            AvoidTarget::Account,
            BlockKind::Penalty,
            Some("endpoint_not_found"),
            text,
        ),
        Some(408) => Verdict::avoid_request(),
        Some(429) if quota => Verdict::avoid(
            AvoidScope::Until(advice.unwrap_or(now.saturating_add(3600))),
            AvoidTarget::Account,
            BlockKind::Quota,
            diagnostic,
            text,
        ),
        Some(429) => Verdict::avoid(
            AvoidScope::Until(
                advice
                    .unwrap_or(now.saturating_add(60))
                    .max(now.saturating_add(1)),
            ),
            AvoidTarget::Account,
            BlockKind::RateLimit,
            diagnostic,
            text,
        ),
        // Anthropic reports an exhausted balance as a 400 invalid request.
        Some(400) if quota => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(3600)),
            AvoidTarget::Account,
            BlockKind::Billing,
            diagnostic,
            text,
        ),
        Some(400) if model_missing => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(3600)),
            model_target(),
            BlockKind::ModelMissing,
            diagnostic,
            text,
        ),
        Some(529) => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(600)),
            AvoidTarget::Account,
            BlockKind::Overload,
            diagnostic,
            text,
        ),
        Some(status @ 400..=499) => Verdict::ClientError {
            status,
            body: failure.body.clone(),
        },
        Some(503) if kind == "overloaded_error" => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(60)),
            AvoidTarget::Account,
            BlockKind::Overload,
            diagnostic,
            text,
        ),
        Some(_) => Verdict::avoid_request(),
        None if quota => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(3600)),
            AvoidTarget::Account,
            BlockKind::Quota,
            diagnostic,
            text,
        ),
        None if kind == "overloaded_error" => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(600)),
            AvoidTarget::Account,
            BlockKind::Overload,
            diagnostic,
            text,
        ),
        None if matches!(code.as_str(), "rate_limit_exceeded" | "slow_down")
            || kind == "rate_limit_error" =>
        {
            Verdict::avoid(
                AvoidScope::Until(now.saturating_add(60)),
                AvoidTarget::Account,
                BlockKind::RateLimit,
                diagnostic,
                text,
            )
        }
        None if kind == "authentication_error" => Verdict::avoid(
            AvoidScope::Permanent,
            AvoidTarget::Account,
            BlockKind::Auth,
            diagnostic,
            text,
        ),
        None if parsed.kind == crate::FailureKind::InvalidRequest
            || kind == "invalid_request_error" =>
        {
            Verdict::ClientError {
                status: 400,
                body: failure.body.clone(),
            }
        }
        None => Verdict::avoid_request(),
    };
    match (status, &base) {
        (Some(status), Verdict::ClientError { .. }) if retry_statuses.contains(&status) => base,
        (Some(status), _) if retry_count > 0 && retry_statuses.contains(&status) => {
            Verdict::retry_same(Duration::from_secs(1), retry_count, base)
        }
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn failure(status: Option<u16>, body: Value) -> UpstreamFailure {
        UpstreamFailure {
            status,
            headers: http::HeaderMap::new(),
            body,
            transport: None,
            model: Some("gpt-5".into()),
            output_committed: false,
            now: 100,
        }
    }

    #[test]
    fn openai_and_anthropic_rejections_follow_the_switching_policy() {
        for (status, body, kind, scope) in [
            (
                401,
                json!({"error":{"type":"authentication_error","message":"bad key"}}),
                BlockKind::Auth,
                AvoidScope::Permanent,
            ),
            (
                402,
                json!({"error":{"message":"payment"}}),
                BlockKind::Billing,
                AvoidScope::Permanent,
            ),
            (
                403,
                json!({"error":{"type":"permission_error"}}),
                BlockKind::Penalty,
                AvoidScope::Until(700),
            ),
            (
                429,
                json!({"error":{"code":"insufficient_quota","message":"You exceeded your current quota"}}),
                BlockKind::Quota,
                AvoidScope::Until(3700),
            ),
            (
                429,
                json!({"error":{"type":"rate_limit_error","message":"slow"}}),
                BlockKind::RateLimit,
                AvoidScope::Until(160),
            ),
            (
                400,
                json!({"type":"error","error":{"type":"invalid_request_error","message":"Your credit balance is too low"}}),
                BlockKind::Billing,
                AvoidScope::Until(3700),
            ),
            (
                529,
                json!({"type":"error","error":{"type":"overloaded_error"}}),
                BlockKind::Overload,
                AvoidScope::Until(700),
            ),
            (405, Value::Null, BlockKind::Penalty, AvoidScope::Until(700)),
        ] {
            match verdict(&failure(Some(status), body), &[], 2) {
                Verdict::Avoid {
                    kind: observed,
                    scope: observed_scope,
                    ..
                } => {
                    assert_eq!(observed, kind, "{status}");
                    assert_eq!(observed_scope, scope, "{status}");
                }
                other => panic!("{status}: {other:?}"),
            }
        }
        match verdict(
            &failure(
                Some(404),
                json!({"error":{"code":"model_not_found","message":"The model `gpt-5` does not exist"}}),
            ),
            &[],
            2,
        ) {
            Verdict::Avoid {
                target: AvoidTarget::Model(model),
                kind: BlockKind::ModelMissing,
                ..
            } => assert_eq!(model, "gpt-5"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            verdict(
                &failure(Some(400), json!({"error":{"code":"invalid_prompt"}})),
                &[],
                2
            ),
            Verdict::ClientError { status: 400, .. }
        ));
        assert!(matches!(
            verdict(&failure(Some(503), Value::Null), &[], 2),
            Verdict::Avoid {
                scope: AvoidScope::Request,
                ..
            }
        ));
    }

    #[test]
    fn administrator_retry_statuses_retry_the_same_account_before_switching() {
        match verdict(&failure(Some(502), Value::Null), &[502], 4) {
            Verdict::RetrySame { max, fallback, .. } => {
                assert_eq!(max, 4);
                assert!(matches!(
                    *fallback,
                    Verdict::Avoid {
                        scope: AvoidScope::Request,
                        ..
                    }
                ));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            verdict(
                &failure(Some(400), json!({"error":{"message":"bad"}})),
                &[400],
                2
            ),
            Verdict::ClientError { .. }
        ));
        let mut committed = failure(Some(502), Value::Null);
        committed.output_committed = true;
        assert!(matches!(
            verdict(&committed, &[502], 2),
            Verdict::Stop { .. }
        ));
    }
}
