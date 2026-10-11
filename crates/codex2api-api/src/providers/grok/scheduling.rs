use crate::pool_scheduler::{ScheduleRequest, body_seed, hash, header_seed, text};
pub(crate) fn request(headers: &http::HeaderMap, body: &serde_json::Value) -> ScheduleRequest {
    let seed = header_seed(headers)
        .or_else(|| {
            headers
                .get("x-grok-conv-id")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        })
        .or_else(|| text(body, "prompt_cache_key"))
        .or_else(|| {
            text(body, "previous_response_id")
                .filter(|v| {
                    v.starts_with("resp_")
                        && v.len() > 5
                        && v.chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                })
                .map(|v| format!("grok-prev-resp:{v}"))
        })
        .or_else(|| body_seed(body));
    let model = text(body, "model");
    let session_hash = seed.map(|seed| {
        hash(&format!(
            "grok-affinity:v1:{}:{seed}",
            model.as_deref().unwrap_or("").to_lowercase()
        ))
    });
    ScheduleRequest {
        model,
        session_hash,
        previous_response: None,
        previous_can_move: false,
    }
}
