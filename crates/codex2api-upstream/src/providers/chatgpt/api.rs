//! ChatGPT channel operations beyond generation, in the pinned Codex request shapes,
//! and the request-scoped context every Codex request carries.
use crate::inbound::request::normalize_tools;
use crate::inbound::{Error as ApiError, InboundRequest, Result};
use crate::supply::{Operation, Wire, WireRequest};
use http::HeaderMap;
use serde_json::{Map, Value, json};

/// Compaction, input counting and search keep their own official bodies.
pub(crate) fn operation(input: &InboundRequest) -> Result<WireRequest> {
    let mut body = input.body.clone();
    let headers = request_context(&mut body, &input.headers)?;
    let object = body.as_object_mut().expect("validated body");
    match input.operation {
        Operation::Search => {
            object.remove("client_metadata");
            search(object)?;
        }
        Operation::Compact | Operation::InputTokens => {
            // Unary protocols keep their own instructions field.
            object.remove("client_metadata");
            normalize_tools(object)?;
        }
        _ => return Err(ApiError::bad_request("Unsupported ChatGPT operation")),
    }
    Ok(WireRequest {
        wire: Wire::Codex,
        operation: input.operation,
        model: body["model"].as_str().map(str::to_owned),
        body,
        headers,
        stream: false,
    })
}

fn search(object: &mut Map<String, Value>) -> Result<()> {
    match object.get("id") {
        Some(id) if id.as_str().is_none_or(str::is_empty) => {
            return Err(ApiError::bad_request("Search id must be a nonempty string"));
        }
        Some(_) => {}
        None => {
            object.insert("id".into(), uuid::Uuid::new_v4().to_string().into());
        }
    }
    if let Some(input) = object.get("input")
        && !input.is_string()
        && !input
            .as_array()
            .is_some_and(|v| v.iter().all(Value::is_object))
    {
        return Err(ApiError::bad_request(
            "Search input must be text or response items",
        ));
    }
    if !object.contains_key("input") && !object.contains_key("commands") {
        return Err(ApiError::bad_request("Search requires input or commands"));
    }
    for (name, value) in object
        .get("commands")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let valid = if name == "response_length" {
            value
                .as_str()
                .is_some_and(|s| ["short", "medium", "long"].contains(&s))
        } else {
            value
                .as_array()
                .is_some_and(|values| values.iter().all(Value::is_object))
        };
        if !valid {
            return Err(ApiError::bad_request(format!(
                "Invalid search commands.{name}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn request_context(body: &mut Value, inbound: &HeaderMap) -> Result<HeaderMap> {
    let supplied = body
        .as_object_mut()
        .expect("validated body")
        .remove("client_metadata")
        .unwrap_or_else(|| json!({}));
    let mut headers = HeaderMap::new();
    let mut metadata = Map::new();
    let mut inbound = inbound.clone();
    crate::strip_hop_by_hop_headers(&mut inbound);
    for (key, header) in [
        ("session_id", "session-id"),
        ("thread_id", "thread-id"),
        ("x-codex-window-id", "x-codex-window-id"),
        ("x-codex-turn-state", "x-codex-turn-state"),
        ("x-codex-parent-thread-id", "x-codex-parent-thread-id"),
        ("x-openai-subagent", "x-openai-subagent"),
        ("traceparent", "traceparent"),
        ("tracestate", "tracestate"),
    ] {
        if supplied.get(key).is_some_and(|v| !v.is_string()) {
            return Err(ApiError::bad_request(format!(
                "client_metadata.{key} must be a string"
            )));
        }
        let value = inbound
            .get(header)
            .map(|v| {
                v.to_str()
                    .map_err(|_| ApiError::bad_request(format!("Invalid {header}")))
            })
            .transpose()?
            .or_else(|| supplied.get(key).and_then(Value::as_str));
        if let Some(value) = value.filter(|v| !v.is_empty()) {
            let value_header = value
                .parse()
                .map_err(|_| ApiError::bad_request(format!("Invalid {header}")))?;
            headers.insert(header, value_header);
            metadata.insert(key.into(), value.into());
        }
    }
    for key in ["turn_id", "parent_turn_id", "root_turn_id"] {
        if let Some(value) = supplied.get(key) {
            let text = value.as_str().ok_or_else(|| {
                ApiError::bad_request(format!("client_metadata.{key} must be a string"))
            })?;
            metadata.insert(key.into(), text.into());
        }
    }
    // A client request ID identifies this request. It is not a synthetic session or cache key.
    if let Some(value) = inbound
        .get("x-client-request-id")
        .or_else(|| headers.get("thread-id"))
    {
        headers.insert("x-client-request-id", value.clone());
    }
    if !metadata.is_empty() {
        body["client_metadata"] = metadata.into();
    }
    Ok(headers)
}

pub(crate) fn image_headers(inbound: &HeaderMap) -> Result<HeaderMap> {
    let mut headers = request_context(&mut json!({}), inbound)?;
    headers.insert(
        "x-codex-image-turn-id",
        inbound
            .get("x-codex-image-turn-id")
            .cloned()
            .unwrap_or_else(|| {
                uuid::Uuid::new_v4()
                    .to_string()
                    .parse()
                    .expect("UUID header")
            }),
    );
    Ok(headers)
}

pub(crate) fn scheduling(headers: &HeaderMap, body: &Value) -> crate::scheduler::ScheduleRequest {
    use crate::scheduler::{body_seed, hash, header_seed, text};
    let seed = header_seed(headers)
        .or_else(|| text(body, "prompt_cache_key"))
        .or_else(|| body_seed(body));
    crate::scheduler::ScheduleRequest {
        model: text(body, "model"),
        session_hash: seed.map(|seed| hash(&seed)),
        previous_response: text(body, "previous_response_id"),
        ..Default::default()
    }
}

/// Send a wire request on the account's official transport. A native client's
/// Responses subpath is forwarded under the Responses endpoint.
pub(crate) async fn send(
    pool: &crate::UpstreamPool,
    account: &str,
    request: WireRequest,
    path: Option<&str>,
) -> crate::Result<reqwest::Response> {
    let (endpoint, subpath) = match path {
        Some(path) => match crate::Endpoint::from_codex_path(path) {
            Some(endpoint) => (endpoint, None),
            None => match path.strip_prefix("responses/") {
                Some(subpath) => (crate::Endpoint::Responses, Some(subpath)),
                None => {
                    return Err(crate::UpstreamError::InvalidRequest(
                        "Unknown Codex endpoint".into(),
                    ));
                }
            },
        },
        None => (
            match request.operation {
                Operation::Generate => crate::Endpoint::Responses,
                Operation::Compact => crate::Endpoint::Compact,
                Operation::InputTokens => crate::Endpoint::InputTokens,
                Operation::Search => crate::Endpoint::Search,
                Operation::ImageGeneration => crate::Endpoint::ImageGeneration,
                Operation::ImageEdit => crate::Endpoint::ImageEdit,
            },
            None,
        ),
    };
    pool.get(account)
        .await?
        .forward_decoded_endpoint(endpoint, subpath, request.body, request.headers)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::responses::{Dialect, build};
    use crate::inbound::Protocol;
    fn inbound(protocol: Protocol, body: Value, headers: HeaderMap) -> InboundRequest {
        InboundRequest::new(protocol, body, headers).unwrap()
    }
    #[test]
    fn responses_keep_api_semantics_and_only_forward_dynamic_client_context() {
        let headers = HeaderMap::from_iter([(
            "session-id".parse().unwrap(),
            "client-session".parse().unwrap(),
        )]);
        let input = inbound(
            Protocol::Responses,
            json!({"model":"custom-model","input":[
            {"role":"system","content":"system"},
            {"type":"reasoning","id":"rs_old","encrypted_content":"ciphertext"},
            {"type":"custom_tool_call","call_id":"ctc_original","name":"code","namespace":"tools","input":"print(1)"},
            {"type":"custom_tool_call_output","call_id":"ctc_original","output":[{"type":"text","text":"ok"},{"type":"input_image","file_id":"file_1"}]}],
            "instructions":"base","tools":[{"type":"namespace","name":"tools","tools":[{"type":"custom","custom":{"name":"code","format":{"type":"text"}}}]},{"type":"web_search"},{"type":"image_generation"}],
            "include":["web_search_call.action.sources"],"prompt_cache_key":" caller key ",
            "client_metadata":{"thread_id":"client-thread","turn_id":"client-turn","session_id":"body-session","x-codex-installation-id":"attacker","access_programs":"attacker"},
            "metadata":{"order":"local"},"access_programs":{"cyber":true},"temperature":0.5,"truncation":"auto","future_field":1}),
            headers,
        );
        let (body, headers) = build(&input, Dialect::Codex).unwrap();
        assert_eq!(body["input"][0]["role"], "developer");
        assert_eq!(body["input"][1]["role"], "developer");
        assert_eq!(body["input"][2]["encrypted_content"], "ciphertext");
        assert_eq!(body["input"][2]["summary"], json!([]));
        assert!(body["input"][2].get("id").is_none());
        assert_eq!(body["input"][4]["output"][0]["type"], "input_text");
        assert_eq!(body["tools"][0]["tools"][0]["name"], "code");
        assert_eq!(
            body["include"],
            json!([
                "web_search_call.action.sources",
                "reasoning.encrypted_content"
            ])
        );
        assert_eq!(body["prompt_cache_key"], " caller key ");
        assert_eq!(
            (body["stream"].clone(), body["store"].clone()),
            (json!(true), json!(false))
        );
        assert_eq!(headers["session-id"], "client-session");
        assert_eq!(headers["thread-id"], "client-thread");
        assert_eq!(body["client_metadata"]["turn_id"], "client-turn");
        assert!(
            body["client_metadata"]
                .get("x-codex-installation-id")
                .is_none()
        );
        for field in [
            "access_programs",
            "instructions",
            "temperature",
            "truncation",
        ] {
            assert!(body.get(field).is_none(), "{field}");
        }
        assert_eq!(input.metadata, Some(json!({"order":"local"})));
    }

    #[test]
    fn tool_choice_follows_declared_tools() {
        for (choice, expected) in [
            (
                json!({"type":"function","function":{"name":"lookup"}}),
                json!({"type":"function","name":"lookup"}),
            ),
            (json!({"type":"function","name":"missing"}), json!("auto")),
            (json!({"type":"web_search"}), json!({"type":"web_search"})),
            (json!({"type":"image_generation"}), json!("auto")),
            (json!("required"), json!("required")),
        ] {
            let (body, _) = build(&inbound(Protocol::Responses, json!({"model":"m","input":"hi",
                "tools":[{"type":"function","name":"lookup"},{"type":"web_search"}],"tool_choice":choice}), HeaderMap::new()), Dialect::Codex).unwrap();
            assert_eq!(body["tool_choice"], expected);
        }
    }

    #[test]
    fn unary_protocols_keep_their_own_wire_contract_and_client_ids_are_not_invented() {
        let search = operation(&inbound(Protocol::Search, json!({"model":"custom-model","id":"search-session","commands":{"search_query":[{"q":"query","recency":7}],"image_query":[{"q":"image"}],"open":[{"ref_id":"turn0search0"}],"response_length":"long"},"settings":{"external_web_access":"live"},"max_output_tokens":123}), HeaderMap::new())).unwrap();
        assert_eq!(search.operation, Operation::Search);
        assert_eq!(search.body["id"], "search-session");
        assert_eq!(search.body["max_output_tokens"], 123);
        for field in [
            "input",
            "tools",
            "stream",
            "store",
            "include",
            "client_metadata",
        ] {
            assert!(search.body.get(field).is_none(), "{field}");
        }
        assert!(!search.headers.contains_key("session-id"));
        for protocol in [Protocol::Compact, Protocol::InputTokens] {
            let request = operation(&inbound(
                protocol,
                json!({"model":"custom","input":"hello","instructions":"count this"}),
                HeaderMap::new(),
            ))
            .unwrap();
            assert_eq!(request.body["instructions"], "count this");
            assert!(!request.stream);
            assert!(request.body.get("store").is_none());
            assert!(request.body.get("stream").is_none());
        }
    }
}
