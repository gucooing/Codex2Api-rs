use crate::pool_scheduler::{ScheduleRequest, body_seed, hash, header_seed, text};
pub(crate) fn request(headers: &http::HeaderMap, body: &serde_json::Value) -> ScheduleRequest {
    let seed = header_seed(headers)
        .or_else(|| text(body, "prompt_cache_key"))
        .or_else(|| body_seed(body));
    ScheduleRequest {
        model: text(body, "model"),
        session_hash: seed.map(|v| hash(&v)),
        previous_response: text(body, "previous_response_id"),
        parent_response: headers
            .get(codex2api_common::version::HEADER_GUARDIAN)
            .is_some_and(|value| value == "reviewer")
            .then(|| {
                body.pointer("/client_metadata/parent_response_id")
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_owned)
            })
            .flatten(),
        previous_can_move: false,
        admission: None,
    }
}
