use std::io::Read;

use bytes::Bytes;
use http::{HeaderMap, HeaderValue};
use serde_json::{Map, Value};

use crate::error::{Result, UpstreamError};
use crate::headers::*;

pub const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;

pub(crate) struct PreparedRequest {
    pub body: Bytes,
    pub headers: HeaderMap,
}

/// Read only non-content metadata for the incoming usage ledger (including zstd bodies).
#[derive(Default)]
pub struct RequestMetadata {
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub service_tier: Option<String>,
    pub image_size: Option<String>,
    pub generate: Option<bool>,
}

pub fn request_metadata(body: &[u8], headers: &HeaderMap) -> Result<RequestMetadata> {
    let value = decode_body(body, headers)?;
    Ok(RequestMetadata {
        model: value
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned),
        reasoning_effort: value
            .pointer("/reasoning/effort")
            .and_then(Value::as_str)
            .map(|s| s.chars().take(64).collect()),
        service_tier: value
            .get("service_tier")
            .and_then(Value::as_str)
            .map(|s| s.chars().take(64).collect()),
        image_size: value
            .get("size")
            .and_then(Value::as_str)
            .map(|s| s.chars().take(64).collect()),
        generate: value.get("generate").and_then(Value::as_bool),
    })
}

pub fn decode_body(body: &[u8], inbound: &HeaderMap) -> Result<Value> {
    if body.len() > MAX_REQUEST_BYTES {
        return Err(UpstreamError::RequestTooLarge);
    }
    let encodings = inbound.get_all(http::header::CONTENT_ENCODING);
    let values: Vec<_> = encodings.iter().collect();
    if values.len() > 1 {
        return Err(UpstreamError::UnsupportedEncoding);
    }
    let encoding = values
        .first()
        .map(|v| v.to_str().unwrap_or("invalid").trim())
        .unwrap_or("identity");
    let decoded;
    let json = if encoding.eq_ignore_ascii_case("zstd") {
        let mut decoder = zstd::stream::read::Decoder::new(body)
            .map_err(|_| UpstreamError::InvalidRequest("Invalid zstd request body.".into()))?;
        decoder
            .window_log_max(26)
            .map_err(|_| UpstreamError::InvalidRequest("Invalid zstd window.".into()))?;
        let mut bytes = Vec::new();
        decoder
            .take((MAX_REQUEST_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| UpstreamError::InvalidRequest("Invalid zstd request body.".into()))?;
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(UpstreamError::RequestTooLarge);
        }
        decoded = bytes;
        decoded.as_slice()
    } else if encoding.eq_ignore_ascii_case("identity") {
        body
    } else {
        return Err(UpstreamError::UnsupportedEncoding);
    };
    let value: Value = serde_json::from_slice(json)
        .map_err(|e| UpstreamError::InvalidRequest(format!("Invalid JSON request body: {e}")))?;
    if !value.is_object() {
        return Err(UpstreamError::InvalidRequest(
            "Request body must be an object.".into(),
        ));
    }
    Ok(value)
}

/// Only installation identity is rewritten. Conversation contents and dynamic IDs survive.
pub fn normalize_response_identity(
    body: &mut Value,
    installation_id: &str,
    inbound: &HeaderMap,
) -> Result<HeaderMap> {
    let object = body
        .as_object_mut()
        .ok_or_else(|| UpstreamError::InvalidRequest("Request body must be an object.".into()))?;
    let metadata = object
        .entry("client_metadata")
        .or_insert_with(|| Value::Object(Map::new()));
    if metadata.is_null() {
        *metadata = Value::Object(Map::new());
    }
    let metadata = metadata.as_object_mut().ok_or_else(|| {
        UpstreamError::InvalidRequest("client_metadata must be an object.".into())
    })?;
    metadata.insert(
        X_CODEX_INSTALLATION_ID_HEADER.into(),
        Value::String(installation_id.into()),
    );
    if metadata.contains_key("installation_id") {
        metadata.insert(
            "installation_id".into(),
            Value::String(installation_id.into()),
        );
    }
    let mut headers = normalize_protocol_headers(inbound, installation_id)?;
    for (key, header) in [
        ("session_id", SESSION_ID_HEADER),
        ("thread_id", THREAD_ID_HEADER),
        (X_CODEX_WINDOW_ID_HEADER, X_CODEX_WINDOW_ID_HEADER),
        (
            X_CODEX_PARENT_THREAD_ID_HEADER,
            X_CODEX_PARENT_THREAD_ID_HEADER,
        ),
        (X_OPENAI_SUBAGENT_HEADER, X_OPENAI_SUBAGENT_HEADER),
    ] {
        if !headers.contains_key(header)
            && let Some(value) = metadata.get(key).and_then(Value::as_str)
        {
            insert_header(&mut headers, header, value);
        }
    }
    if !headers.contains_key(X_CLIENT_REQUEST_ID_HEADER)
        && let Some(thread) = headers.get(THREAD_ID_HEADER).cloned()
    {
        headers.insert(X_CLIENT_REQUEST_ID_HEADER, thread);
    }
    if let Some(value) = metadata.get_mut(X_CODEX_TURN_METADATA_HEADER) {
        let text = value.as_str().ok_or_else(|| {
            UpstreamError::InvalidRequest("Turn metadata must be a JSON string.".into())
        })?;
        let mut turn = normalize_turn(text, installation_id)?;
        *value = Value::String(serde_json::to_string(&turn)?);
        if !headers.contains_key(X_CODEX_TURN_METADATA_HEADER) {
            // Official compatibility header excludes the unbounded tool inventory.
            turn.as_object_mut().unwrap().remove("tool_namespaces_info");
            headers.insert(X_CODEX_TURN_METADATA_HEADER, ascii_json_header(&turn)?);
        }
    }
    if headers
        .get(crate::headers::X_CODEX_GUARDIAN_HEADER)
        .is_some_and(|v| v == "reviewer")
    {
        body.as_object_mut().unwrap().remove("service_tier");
        headers.remove("x-codex-routing-hint");
    } else if !headers.contains_key("x-codex-routing-hint")
        && let Some(model) = body.get("model").and_then(Value::as_str)
    {
        let hint = match body.get("service_tier").and_then(Value::as_str) {
            Some(tier) => format!("model={model};tier={tier}"),
            None => format!("model={model}"),
        };
        insert_header(&mut headers, "x-codex-routing-hint", &hint);
    }
    Ok(headers)
}

/// Search can carry installation identity solely in the turn metadata header.
pub(crate) fn normalize_protocol_headers(
    inbound: &HeaderMap,
    installation_id: &str,
) -> Result<HeaderMap> {
    let mut headers = protocol_headers(inbound);
    if let Some(value) = headers.get(X_CODEX_TURN_METADATA_HEADER) {
        let text = value
            .to_str()
            .map_err(|_| UpstreamError::InvalidRequest("Invalid turn metadata header.".into()))?;
        let turn = normalize_turn(text, installation_id)?;
        headers.insert(X_CODEX_TURN_METADATA_HEADER, ascii_json_header(&turn)?);
    }
    Ok(headers)
}

fn normalize_turn(text: &str, installation_id: &str) -> Result<Value> {
    let mut turn: Value = serde_json::from_str(text)
        .map_err(|_| UpstreamError::InvalidRequest("Invalid turn metadata JSON.".into()))?;
    let object = turn
        .as_object_mut()
        .ok_or_else(|| UpstreamError::InvalidRequest("Turn metadata must be an object.".into()))?;
    for key in ["installation_id", X_CODEX_INSTALLATION_ID_HEADER] {
        if object.contains_key(key) {
            object.insert(key.into(), Value::String(installation_id.into()));
        }
    }
    Ok(turn)
}

fn ascii_json_header(value: &Value) -> Result<HeaderValue> {
    use std::fmt::Write;
    let mut ascii = String::new();
    for unit in serde_json::to_string(value)?.encode_utf16() {
        if unit <= 127 {
            ascii.push(char::from_u32(unit as u32).unwrap());
        } else {
            write!(ascii, "\\u{unit:04x}").unwrap();
        }
    }
    Ok(HeaderValue::from_str(&ascii)?)
}

pub(crate) fn prepare_responses(
    body: &[u8],
    inbound: &HeaderMap,
    installation_id: &str,
    timezone: Option<&str>,
) -> Result<PreparedRequest> {
    let mut body = decode_body(body, inbound)?;
    crate::apply_response_timezone(&mut body, timezone)?;
    let mut headers = normalize_response_identity(&mut body, installation_id, inbound)?;
    let json = serde_json::to_vec(&body)?;
    // Logged-in OpenAI backend requests use zstd level 3 by default in pinned Codex.
    let body = Bytes::from(zstd::stream::encode_all(json.as_slice(), 3)?);
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        http::header::CONTENT_ENCODING,
        HeaderValue::from_static("zstd"),
    );
    headers.insert(
        http::header::ACCEPT,
        HeaderValue::from_static(ACCEPT_EVENT_STREAM),
    );
    Ok(PreparedRequest { body, headers })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn usage_metadata_extracts_reasoning_from_compressed_requests() {
        let body=br#"{"model":"gpt-test","reasoning":{"effort":"xhigh"},"service_tier":"priority","size":"1024x1024","input":[{"content":"private"}]}"#;
        let compressed = zstd::stream::encode_all(body.as_slice(), 3).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("content-encoding", HeaderValue::from_static("zstd"));
        let metadata = request_metadata(&compressed, &headers).unwrap();
        assert_eq!(metadata.model.as_deref(), Some("gpt-test"));
        assert_eq!(metadata.reasoning_effort.as_deref(), Some("xhigh"));
        assert_eq!(metadata.service_tier.as_deref(), Some("priority"));
        assert_eq!(metadata.image_size.as_deref(), Some("1024x1024"));
    }

    #[test]
    fn normalizes_only_identity_and_preserves_session_and_tool_contents() {
        let input = json!({"model":"model", "service_tier":"priority", "stream":true,
            "input":[{"role":"user","content":"installation_id=do not edit"}],
            "tools":[{"type":"function","name":"tool"}], "previous_response_id":"previous",
            "client_metadata": {"x-codex-installation-id":"caller", "session_id":"session",
                "thread_id":"thread", "turn_id":"turn", "x-codex-window-id":"window",
                "x-codex-turn-metadata": "{\"installation_id\":\"caller\",\"turn_id\":\"turn\",\"tool_namespaces_info\":[\"tool\"],\"agent_name\":\"测试\"}"}});
        let mut inbound = HeaderMap::new();
        inbound.insert("x-codex-turn-state", HeaderValue::from_static("sticky"));
        inbound.insert("x-forwarded-for", HeaderValue::from_static("private"));
        inbound.insert(
            "authorization",
            HeaderValue::from_static("Bearer proxy-key"),
        );
        for compressed in [false, true] {
            let raw = serde_json::to_vec(&input).unwrap();
            let raw = if compressed {
                inbound.insert("content-encoding", HeaderValue::from_static("zstd"));
                zstd::stream::encode_all(raw.as_slice(), 3).unwrap()
            } else {
                raw
            };
            let prepared = prepare_responses(&raw, &inbound, "account-installation", None).unwrap();
            let out = decode_body(&prepared.body, &prepared.headers).unwrap();
            for key in [
                "input",
                "tools",
                "previous_response_id",
                "model",
                "stream",
                "service_tier",
            ] {
                assert_eq!(out[key], input[key], "{key}");
            }
            for key in ["session_id", "thread_id", "turn_id", "x-codex-window-id"] {
                assert_eq!(out["client_metadata"][key], input["client_metadata"][key]);
            }
            assert_eq!(
                out["client_metadata"][X_CODEX_INSTALLATION_ID_HEADER],
                "account-installation"
            );
            let turn: Value = serde_json::from_str(
                out["client_metadata"][X_CODEX_TURN_METADATA_HEADER]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(turn["installation_id"], "account-installation");
            assert_eq!(turn["tool_namespaces_info"], json!(["tool"]));
            let turn_header: Value = serde_json::from_str(
                prepared.headers[X_CODEX_TURN_METADATA_HEADER]
                    .to_str()
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(turn_header["installation_id"], "account-installation");
            assert_eq!(turn_header["agent_name"], "测试");
            assert!(turn_header.get("tool_namespaces_info").is_none());
            assert_eq!(prepared.headers["x-codex-turn-state"], "sticky");
            assert_eq!(
                prepared.headers["x-codex-routing-hint"],
                "model=model;tier=priority"
            );
            assert!(!prepared.headers.contains_key("x-forwarded-for"));
            assert!(!prepared.headers.contains_key("authorization"));
        }
    }

    #[test]
    fn rejects_invalid_encodings_and_shapes_without_panicking() {
        let mut headers = HeaderMap::new();
        headers.insert("content-encoding", HeaderValue::from_static("zstd"));
        assert!(matches!(
            decode_body(b"invalid", &headers),
            Err(UpstreamError::InvalidRequest(_))
        ));
        headers.insert("content-encoding", HeaderValue::from_static("gzip"));
        assert!(matches!(
            decode_body(b"{}", &headers),
            Err(UpstreamError::UnsupportedEncoding)
        ));
        assert!(decode_body(b"[]", &HeaderMap::new()).is_err());
        assert!(
            prepare_responses(br#"{"client_metadata":[]}"#, &HeaderMap::new(), "id", None).is_err()
        );
    }

    #[test]
    fn same_conversation_has_isolated_account_identity() {
        let raw = br#"{"model":"test","client_metadata":{"thread_id":"thread","session_id":"session","x-codex-installation-id":"caller"},"input":[{"content":"unchanged"}]}"#;
        let a = prepare_responses(raw, &HeaderMap::new(), "account-a", None).unwrap();
        let b = prepare_responses(raw, &HeaderMap::new(), "account-b", None).unwrap();
        let a = decode_body(&a.body, &a.headers).unwrap();
        let mut b = decode_body(&b.body, &b.headers).unwrap();
        assert_eq!(
            a["client_metadata"][X_CODEX_INSTALLATION_ID_HEADER],
            "account-a"
        );
        assert_eq!(
            b["client_metadata"][X_CODEX_INSTALLATION_ID_HEADER],
            "account-b"
        );
        b["client_metadata"][X_CODEX_INSTALLATION_ID_HEADER] = "account-a".into();
        assert_eq!(a, b);
        let configured =
            prepare_responses(raw, &HeaderMap::new(), "account-a", Some("Asia/Taipei")).unwrap();
        assert_eq!(configured.headers["content-encoding"], "zstd");
        let decoded = decode_body(&configured.body, &configured.headers).unwrap();
        assert!(
            decoded["input"]
                .to_string()
                .contains("<timezone>Asia/Taipei</timezone>")
        );
        assert!(!configured.headers.contains_key("timezone"));
    }

    #[test]
    fn existing_turn_headers_keep_all_non_identity_fields() {
        let mut headers = HeaderMap::new();
        headers.insert(
            X_CODEX_TURN_METADATA_HEADER,
            HeaderValue::from_static(
                r#"{"installation_id":"caller","turn_id":"header-turn","extra":"preserve"}"#,
            ),
        );
        let mut body = serde_json::json!({"client_metadata": {
            "x-codex-turn-metadata": "{\"installation_id\":\"caller\",\"turn_id\":\"body-turn\"}"
        }});
        let headers = normalize_response_identity(&mut body, "account", &headers).unwrap();
        let header: Value =
            serde_json::from_str(headers[X_CODEX_TURN_METADATA_HEADER].to_str().unwrap()).unwrap();
        let metadata: Value = serde_json::from_str(
            body["client_metadata"][X_CODEX_TURN_METADATA_HEADER]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            header,
            serde_json::json!({"installation_id":"account", "turn_id":"header-turn", "extra":"preserve"})
        );
        assert_eq!(
            metadata,
            serde_json::json!({"installation_id":"account", "turn_id":"body-turn"})
        );
    }
}
