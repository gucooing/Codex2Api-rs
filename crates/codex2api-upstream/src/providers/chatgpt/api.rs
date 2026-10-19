//! Public API requests in the pinned Codex subscription request shape.
use crate::protocol::request::{declared_tools, normalize_tools};
use crate::protocol::{
    Error as ApiError, InboundRequest, PreparedRequest, Protocol, Result, convert,
};
use http::HeaderMap;
use serde_json::{Map, Value, json};

/// Public SDK fields the subscription Responses endpoint does not accept.
const UNSUPPORTED: [&str; 18] = [
    "max_output_tokens",
    "max_completion_tokens",
    "max_tokens",
    "temperature",
    "top_p",
    "frequency_penalty",
    "presence_penalty",
    "chat_template_kwargs",
    "user",
    "metadata",
    "prompt_cache_retention",
    "safety_identifier",
    "stream_options",
    "truncation",
    "stop",
    "stop_sequences",
    "seed",
    "logit_bias",
];

pub(crate) fn prepare(input: &InboundRequest) -> Result<PreparedRequest> {
    let upstream = if input.protocol.generation() {
        Protocol::Responses
    } else {
        input.protocol
    };
    if upstream.image() {
        let mut prepared = super::api_images::prepare(
            input.body.clone(),
            upstream == Protocol::ImageEdit,
            &input.headers,
        )?;
        prepared.metadata = input.metadata.clone();
        return Ok(prepared);
    }
    let mut body = if upstream == Protocol::Responses {
        convert(input.protocol, &input.body)?
    } else {
        input.body.clone()
    };
    let headers = request_context(&mut body, &input.headers)?;
    let object = body.as_object_mut().expect("validated body");
    match upstream {
        Protocol::Responses => codex(object)?,
        Protocol::Search => {
            object.remove("client_metadata");
            search(object)?;
        }
        Protocol::Compact | Protocol::InputTokens => {
            // Unary protocols keep their own instructions field.
            object.remove("client_metadata");
            normalize_tools(object)?;
        }
        _ => return Err(ApiError::bad_request("Unsupported ChatGPT operation")),
    }
    let mut prepared = PreparedRequest::new(input, upstream, body);
    prepared.headers = headers;
    Ok(prepared)
}

/// Codex 0.163 sends system-level input with the developer role, always streams,
/// never stores, and replays reasoning only through encrypted content.
fn codex(object: &mut Map<String, Value>) -> Result<()> {
    for name in UNSUPPORTED {
        object.remove(name);
    }
    for item in object
        .get_mut("input")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if item["role"] == "system" {
            item["role"] = "developer".into();
        }
        if item["type"] == "reasoning" {
            // Stateless replay never looks up rs_* IDs.
            let item = item.as_object_mut().expect("validated input item");
            item.remove("id");
            item.remove("call_id");
        }
    }
    tool_choice(object);
    object.insert("stream".into(), true.into());
    object.insert("store".into(), false.into());
    object.entry("tool_choice").or_insert_with(|| "auto".into());
    object
        .entry("parallel_tool_calls")
        .or_insert_with(|| true.into());
    object.entry("reasoning").or_insert(Value::Null);
    let include = object
        .entry("include")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| ApiError::bad_request("include must be an array"))?;
    if !include.iter().any(|v| v == "reasoning.encrypted_content") {
        include.push("reasoning.encrypted_content".into());
    }
    Ok(())
}

/// A named choice must reference a declared tool; otherwise the model chooses.
fn tool_choice(object: &mut Map<String, Value>) {
    let Some(choice) = object.get("tool_choice").filter(|c| c.is_object()) else {
        return;
    };
    let kind = choice["type"].as_str().unwrap_or_default().to_owned();
    if kind == "allowed_tools" {
        return;
    }
    let name = choice["name"].as_str().map(str::to_owned);
    let mut declared = Vec::new();
    declared_tools(object.get("tools").unwrap_or(&Value::Null), &mut declared);
    for item in object["input"].as_array().into_iter().flatten() {
        if item["type"] == "additional_tools" {
            declared_tools(&item["tools"], &mut declared);
        }
    }
    let choice = if kind == "function" {
        match name.filter(|name| declared.iter().any(|(k, n)| k == "function" && n == name)) {
            Some(name) => json!({"type":"function","name":name}),
            None => "auto".into(),
        }
    } else if declared.iter().any(|(k, _)| *k == kind) {
        return;
    } else {
        "auto".into()
    };
    object.insert("tool_choice".into(), choice);
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

/// Preserve caller-scoped protocol context; account identity is supplied by the upstream transport.
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

pub(crate) async fn send(
    pool: &crate::UpstreamPool,
    account: &str,
    headers: &HeaderMap,
    upstream: Protocol,
    body: Value,
) -> crate::Result<reqwest::Response> {
    let endpoint = match upstream {
        Protocol::Responses => crate::Endpoint::Responses,
        Protocol::Compact => crate::Endpoint::Compact,
        Protocol::InputTokens => crate::Endpoint::InputTokens,
        Protocol::Search => crate::Endpoint::Search,
        Protocol::ImageGeneration => crate::Endpoint::ImageGeneration,
        Protocol::ImageEdit => crate::Endpoint::ImageEdit,
        _ => {
            return Err(crate::UpstreamError::Internal(
                "Unsupported ChatGPT API operation".into(),
            ));
        }
    };
    pool.get(account)
        .await?
        .forward_decoded_endpoint(endpoint, None, body, headers.clone())
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn inbound(protocol: Protocol, body: Value, headers: HeaderMap) -> InboundRequest {
        InboundRequest::new(protocol, body, headers).unwrap()
    }
    #[test]
    fn responses_keep_api_semantics_and_only_forward_dynamic_client_context() {
        let headers = HeaderMap::from_iter([(
            "session-id".parse().unwrap(),
            "client-session".parse().unwrap(),
        )]);
        let prepared = prepare(&inbound(Protocol::Responses, json!({"model":"custom-model","input":[
            {"role":"system","content":"system"},
            {"type":"reasoning","id":"rs_old","encrypted_content":"ciphertext"},
            {"type":"custom_tool_call","call_id":"ctc_original","name":"code","namespace":"tools","input":"print(1)"},
            {"type":"custom_tool_call_output","call_id":"ctc_original","output":[{"type":"text","text":"ok"},{"type":"input_image","file_id":"file_1"}]}],
            "instructions":"base","tools":[{"type":"namespace","name":"tools","tools":[{"type":"custom","custom":{"name":"code","format":{"type":"text"}}}]},{"type":"web_search"},{"type":"image_generation"}],
            "include":["web_search_call.action.sources"],"prompt_cache_key":" caller key ",
            "client_metadata":{"thread_id":"client-thread","turn_id":"client-turn","session_id":"body-session","x-codex-installation-id":"attacker","access_programs":"attacker"},
            "metadata":{"order":"local"},"access_programs":{"cyber":true},"temperature":0.5,"truncation":"auto","future_field":1}), headers)).unwrap();
        let body = &prepared.body;
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
        assert_eq!(prepared.headers["session-id"], "client-session");
        assert_eq!(prepared.headers["thread-id"], "client-thread");
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
        assert_eq!(prepared.metadata, Some(json!({"order":"local"})));
        assert_eq!(prepared.upstream, Protocol::Responses);
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
            let prepared = prepare(&inbound(Protocol::Responses, json!({"model":"m","input":"hi",
                "tools":[{"type":"function","name":"lookup"},{"type":"web_search"}],"tool_choice":choice}), HeaderMap::new())).unwrap();
            assert_eq!(prepared.body["tool_choice"], expected);
        }
    }

    #[test]
    fn unary_protocols_keep_their_own_wire_contract_and_client_ids_are_not_invented() {
        let search = prepare(&inbound(Protocol::Search, json!({"model":"custom-model","id":"search-session","commands":{"search_query":[{"q":"query","recency":7}],"image_query":[{"q":"image"}],"open":[{"ref_id":"turn0search0"}],"response_length":"long"},"settings":{"external_web_access":"live"},"max_output_tokens":123}), HeaderMap::new())).unwrap();
        assert_eq!(search.upstream.upstream_path(), "alpha/search");
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
            let request = prepare(&inbound(
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
