use super::{Error as ApiError, PreparedRequest, Protocol, Result};
use base64::Engine;
use serde_json::{Value, json};
/// Validate public common fields without selecting a provider or inventing a model.
pub fn validate(body: &Value, supported: &[&str]) -> Result<()> {
    let object = body
        .as_object()
        .ok_or_else(|| ApiError::bad_request("Image request must be an object"))?;
    for (name, value) in object {
        if !value.is_null() && !supported.contains(&name.as_str()) {
            return Err(ApiError::bad_request(format!(
                "Image parameter '{name}' is not supported by this platform"
            )));
        }
    }
    for (name, limit) in [("model", 256), ("prompt", 32_000)] {
        if !body[name]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.len() <= limit)
        {
            return Err(ApiError::bad_request(format!(
                "{name} must be a nonempty string (maximum {limit} bytes)"
            )));
        }
    }
    if body
        .get("n")
        .is_some_and(|v| !v.is_null() && !v.as_u64().is_some_and(|n| (1..=10).contains(&n)))
    {
        return Err(ApiError::bad_request("n must be an integer from 1 to 10"));
    }
    for name in ["stream", "deferred"] {
        if body.get(name).is_some_and(|v| !v.is_null() && v != false) {
            return Err(ApiError::bad_request(
                "Image streaming and deferred jobs are not supported",
            ));
        }
    }
    Ok(())
}

pub fn choice(body: &Value, name: &str, choices: &[&str]) -> Result<()> {
    if body.get(name).is_some_and(|value| {
        !value.is_null() && !value.as_str().is_some_and(|v| choices.contains(&v))
    }) {
        return Err(ApiError::bad_request(format!("Unsupported image {name}")));
    }
    Ok(())
}

pub fn references(body: &mut Value, edit: bool, maximum: usize) -> Result<Vec<Value>> {
    let object = body.as_object_mut().expect("validated image object");
    let images = object.remove("images");
    let image = object.remove("image");
    if images.as_ref().is_some_and(|v| !v.is_null()) && image.as_ref().is_some_and(|v| !v.is_null())
    {
        return Err(ApiError::bad_request("Provide image or images, not both"));
    }
    let refs = match images
        .filter(|v| !v.is_null())
        .or_else(|| image.filter(|v| !v.is_null()))
    {
        Some(Value::Array(items)) => items,
        Some(value) => vec![value],
        None => vec![],
    };
    if edit && (refs.is_empty() || refs.len() > maximum) {
        return Err(ApiError::bad_request(format!(
            "Image editing requires 1 to {maximum} input images"
        )));
    }
    if !edit && !refs.is_empty() {
        return Err(ApiError::bad_request(
            "Input images require /v1/images/edits",
        ));
    }
    refs.into_iter()
        .map(|value| {
            if let Some(file_id) = value.get("file_id") {
                let file_id = file_id
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| ApiError::bad_request("file_id must be a nonempty string"))?;
                if value.get("image_url").is_some() || value.get("url").is_some() {
                    return Err(ApiError::bad_request(
                        "Use file_id or image_url for each image",
                    ));
                }
                return Ok(json!({"file_id":file_id}));
            }
            let image = value
                .as_str()
                .or_else(|| value["image_url"].as_str())
                .or_else(|| value["url"].as_str())
                .ok_or_else(|| {
                    ApiError::bad_request("Each input image requires image_url or url")
                })?;
            if image.len() > 20 * 1024 * 1024 || image.chars().any(char::is_control) {
                return Err(ApiError::bad_request("Invalid image reference"));
            }
            if image.starts_with("data:") {
                let (mime, encoded) = image.split_once(";base64,").ok_or_else(|| {
                    ApiError::bad_request("Input images require a base64 data URL")
                })?;
                if !matches!(
                    mime,
                    "data:image/png" | "data:image/jpeg" | "data:image/webp"
                ) || encoded.is_empty()
                    || base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .is_err()
                {
                    return Err(ApiError::bad_request("Invalid base64 image data"));
                }
            } else {
                let url = url::Url::parse(image)
                    .map_err(|_| ApiError::bad_request("Invalid image URL"))?;
                if !matches!(url.scheme(), "https" | "http")
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                {
                    return Err(ApiError::bad_request(
                        "Input image URLs must use HTTP or HTTPS without credentials",
                    ));
                }
            }
            Ok(json!({"image_url":image}))
        })
        .collect()
}

pub fn prepared(mut body: Value, edit: bool, headers: http::HeaderMap) -> PreparedRequest {
    body.as_object_mut()
        .expect("validated image object")
        .retain(|key, value| !value.is_null() && key != "stream" && key != "deferred");
    let protocol = if edit {
        Protocol::ImageEdit
    } else {
        Protocol::ImageGeneration
    };
    PreparedRequest {
        headers,
        body,
        inbound: protocol,
        upstream: protocol,
        upstream_endpoint: None,
        stream: false,
        include_usage: false,
        metadata: None,
    }
}
