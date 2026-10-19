//! Antigravity serves the public generation protocols only; the canonical Responses
//! request is converted once into the desktop client's Gemini request.
use crate::protocol::{
    Error as ApiError, InboundRequest, PreparedRequest, Protocol, Result, convert,
};
use http::HeaderMap;
use serde_json::Value;

pub(crate) fn prepare(input: &InboundRequest) -> Result<PreparedRequest> {
    if !input.protocol.generation() {
        return Err(ApiError::bad_request(
            "This endpoint is not offered by the Antigravity subscription transport",
        ));
    }
    let canonical = convert(input.protocol, &input.body)?;
    let body = super::request::gemini(&canonical, &input.headers)?;
    Ok(PreparedRequest::new(input, Protocol::Responses, body))
}

pub(crate) fn scheduling(headers: &HeaderMap, body: &Value) -> crate::scheduler::ScheduleRequest {
    use crate::scheduler::{body_seed, hash, header_seed, text};
    let seed = header_seed(headers)
        .or_else(|| text(body, "prompt_cache_key"))
        .or_else(|| body_seed(body));
    crate::scheduler::ScheduleRequest {
        model: text(body, "model"),
        session_hash: seed.map(|seed| hash(&seed)),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_generation_protocol_prepares_a_gemini_request_and_others_are_refused() {
        for (protocol, body) in [
            (
                Protocol::ChatCompletions,
                json!({"model":"gemini-3-pro-high","messages":[{"role":"user","content":"hi"}]}),
            ),
            (
                Protocol::Responses,
                json!({"model":"gemini-3-pro-high","input":"hi"}),
            ),
            (
                Protocol::Messages,
                json!({"model":"gemini-3-pro-high","max_tokens":8,"messages":[{"role":"user","content":"hi"}]}),
            ),
        ] {
            let prepared =
                prepare(&InboundRequest::new(protocol, body, HeaderMap::new()).unwrap()).unwrap();
            assert_eq!(prepared.upstream, Protocol::Responses);
            assert_eq!(prepared.body["model"], "gemini-3-pro-high");
            assert_eq!(
                prepared.body["request"]["contents"][0]["parts"][0]["text"],
                "hi"
            );
        }
        assert!(
            prepare(
                &InboundRequest::new(
                    Protocol::Compact,
                    json!({"model":"m","input":"hi"}),
                    HeaderMap::new()
                )
                .unwrap()
            )
            .is_err()
        );
        assert!(
            prepare(
                &InboundRequest::new(
                    Protocol::Responses,
                    json!({"model":"m","input":"hi","previous_response_id":"resp_1"}),
                    HeaderMap::new()
                )
                .unwrap()
            )
            .is_err()
        );
    }
}
