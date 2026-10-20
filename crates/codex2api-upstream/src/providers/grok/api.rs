//! Grok channel request context: affinity seeds and the request-scoped headers
//! Grok Build forwards. Request shapes are built by the conversion matrix.
use http::HeaderMap;
use serde_json::Value;

pub(crate) fn scheduling(headers: &HeaderMap, body: &Value) -> crate::scheduler::ScheduleRequest {
    use crate::scheduler::{body_seed, hash, header_seed, text};
    let seed = header_seed(headers)
        .or_else(|| {
            headers
                .get("x-grok-conv-id")
                .and_then(|v| v.to_str().ok())
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        })
        .or_else(|| text(body, "prompt_cache_key"))
        .or_else(|| text(body, "previous_response_id"))
        .or_else(|| body_seed(body));
    crate::scheduler::ScheduleRequest {
        model: text(body, "model"),
        session_hash: seed.map(|seed| hash(&seed)),
        previous_response: text(body, "previous_response_id"),
        previous_can_move: false,
        ..Default::default()
    }
}

pub(crate) fn outbound_headers(inbound: &HeaderMap) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for name in ["x-grok-req-id", "x-grok-session-id", "x-grok-conv-id"] {
        if let Some(value) = inbound.get(name) {
            headers.insert(name, value.clone());
        }
    }
    headers.entry("x-grok-req-id").or_insert_with(|| {
        uuid::Uuid::new_v4()
            .to_string()
            .parse()
            .expect("UUID header")
    });
    headers
}
