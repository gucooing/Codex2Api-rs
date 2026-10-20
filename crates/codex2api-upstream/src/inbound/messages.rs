//! Messages request validation, the Messages error envelope and the content helpers
//! every wire's converter reads.
use super::{Error, Result};
use serde_json::{Value, json};

pub(crate) fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| Error::bad_request(format!("{key} must be text")))
}
pub(crate) fn blocks(value: &Value) -> Result<std::borrow::Cow<'_, [Value]>> {
    match value {
        Value::String(v) => Ok(vec![json!({"type":"text","text":v})].into()),
        Value::Array(v) => Ok(v.as_slice().into()),
        _ => Err(Error::bad_request(
            "Message content must be text or content blocks",
        )),
    }
}

pub fn validate(body: &Value) -> Result<()> {
    if body["max_tokens"].as_u64().is_none_or(|n| n == 0) {
        return Err(Error::bad_request("max_tokens must be a positive integer"));
    }
    if let Some(stops) = body.get("stop_sequences")
        && !stops
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
    {
        return Err(Error::bad_request(
            "stop_sequences must be an array of strings",
        ));
    }
    for key in ["temperature", "top_p"] {
        if body
            .get(key)
            .is_some_and(|v| !v.as_f64().is_some_and(|v| (0.0..=1.0).contains(&v)))
        {
            return Err(Error::bad_request(format!("{key} must be between 0 and 1")));
        }
    }
    for field in ["thinking", "output_config", "tool_choice"] {
        if body.get(field).is_some_and(|v| !v.is_object()) {
            return Err(Error::bad_request(format!("{field} must be an object")));
        }
    }
    let messages = body["messages"]
        .as_array()
        .filter(|m| !m.is_empty())
        .ok_or_else(|| Error::bad_request("messages must be a nonempty array"))?;
    for message in messages {
        let assistant = match message["role"].as_str() {
            Some("assistant") => true,
            Some("user") => false,
            _ => {
                return Err(Error::bad_request(
                    "Messages roles must be user or assistant",
                ));
            }
        };
        for block in blocks(&message["content"])?.iter() {
            match block["type"].as_str() {
                Some("text") => {
                    text(block, "text")?;
                }
                Some("tool_use") if assistant => {
                    text(block, "id")?;
                    text(block, "name")?;
                    if !block["input"].is_object() {
                        return Err(Error::bad_request("tool_use input must be an object"));
                    }
                }
                Some("tool_result") if !assistant => {
                    text(block, "tool_use_id")?;
                    if let Some(v) = block.get("content") {
                        blocks(v)?;
                    }
                }
                Some("tool_use" | "tool_result") => {
                    return Err(Error::bad_request(
                        "tool_use belongs to assistant and tool_result to user messages",
                    ));
                }
                Some(_) => {}
                None => return Err(Error::bad_request("Content blocks require a type")),
            }
        }
    }
    if let Some(system) = body.get("system") {
        for block in blocks(system)?.iter() {
            if block["type"] != "text" {
                return Err(Error::bad_request("system accepts text blocks only"));
            }
            text(block, "text")?;
        }
    }
    for tool in body["tools"].as_array().into_iter().flatten() {
        text(tool, "name")?;
        if tool.get("type").is_none_or(|t| t == "custom") && !tool["input_schema"].is_object() {
            return Err(Error::bad_request("input_schema must be an object"));
        }
    }
    if body.get("tools").is_some_and(|v| !v.is_array()) {
        return Err(Error::bad_request("tools must be an array"));
    }
    Ok(())
}

pub fn error(status: http::StatusCode, value: &Value) -> Value {
    let kind = match status.as_u16() {
        401 => "authentication_error",
        403 => "permission_error",
        404 => "not_found_error",
        413 => "request_too_large",
        429 => "rate_limit_error",
        529 => "overloaded_error",
        400..=499 => "invalid_request_error",
        _ => "api_error",
    };
    let mut error = value
        .get("error")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({"message":"The request could not be completed."}));
    error["type"] = kind.into();
    json!({"type":"error","error":error})
}

/// System text without the client attribution block Claude Code injects.
pub(crate) fn system_texts(body: &Value) -> Result<Vec<&str>> {
    let Some(system) = body.get("system") else {
        return Ok(Vec::new());
    };
    let mut texts = Vec::new();
    for block in match system {
        Value::String(text) => {
            return Ok(vec![text.as_str()]
                .into_iter()
                .filter(|t| !t.is_empty())
                .collect());
        }
        Value::Array(blocks) => blocks,
        _ => return Err(Error::bad_request("system must be text or text blocks")),
    } {
        let text = text(block, "text")?;
        if !text.is_empty() && !text.starts_with("x-anthropic-billing-header: ") {
            texts.push(text);
        }
    }
    Ok(texts)
}

/// Messages image source as a URL other protocols accept.
pub(crate) fn image_url(source: &Value) -> Result<String> {
    match source["type"].as_str() {
        Some("url") => Ok(text(source, "url")?.to_owned()),
        Some("base64") => Ok(format!(
            "data:{};base64,{}",
            text(source, "media_type")?,
            text(source, "data")?
        )),
        _ => Err(Error::bad_request("Unsupported Messages image source")),
    }
}
/// Messages input schema as function parameters; an absent schema takes no arguments.
pub(crate) fn schema(value: &Value) -> Value {
    let mut schema = match value {
        Value::Object(_) => value.clone(),
        _ => json!({"type":"object"}),
    };
    if schema["type"] == "object" && !schema["properties"].is_object() {
        schema["properties"] = json!({});
    }
    schema
}
