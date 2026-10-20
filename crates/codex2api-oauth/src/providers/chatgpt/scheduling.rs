use crate::pool_scheduler::{ScheduleRequest, body_seed, hash, header_seed, text};
pub(crate) fn request(headers: &http::HeaderMap, body: &serde_json::Value) -> ScheduleRequest {
    let seed = header_seed(headers)
        .or_else(|| text(body, "prompt_cache_key"))
        .or_else(|| body_seed(body));
    ScheduleRequest {
        model: text(body, "model"),
        session_hash: seed.map(|v| hash(&v)),
        previous_response: text(body, "previous_response_id"),
        previous_can_move: false,
        admission: None,
        allowed_providers: None,
    }
}
