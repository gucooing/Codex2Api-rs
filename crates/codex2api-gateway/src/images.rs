//! Image HTTP input and measured image usage, shared by the two provider adapters.
use crate::{ApiCredential, GatewayError, GatewayState, OutputMode, PreparedRequest, Result};
use axum::{
    body::to_bytes,
    extract::{Extension, FromRequest, Multipart, Request, State},
    response::Response,
};
use base64::Engine;
use serde_json::{Map, Value, json};

pub(crate) async fn generations(
    State(state): State<GatewayState>,
    Extension(credential): Extension<ApiCredential>,
    request: Request,
) -> Result<Response> {
    handle(state, credential, request, false).await
}

pub(crate) async fn edits(
    State(state): State<GatewayState>,
    Extension(credential): Extension<ApiCredential>,
    request: Request,
) -> Result<Response> {
    handle(state, credential, request, true).await
}

async fn handle(
    state: GatewayState,
    credential: ApiCredential,
    request: Request,
    edit: bool,
) -> Result<Response> {
    let headers = request.headers().clone();
    let mut body = decode(request, edit).await?;
    codex2api_upstream::RequestFailureLog::metadata(&codex2api_upstream::request_metadata_value(
        &body,
    ));
    let metadata = body
        .as_object_mut()
        .and_then(|body| body.remove("metadata"));
    if metadata.as_ref().is_some_and(|value| !value.is_object()) {
        return Err(GatewayError::bad_request("metadata must be an object"));
    }
    crate::request::strip_client_fields(&mut body);
    if let Some(body) = body.as_object_mut() {
        body.remove("client_metadata");
    }
    let mut prepared = match credential.principal.group.provider_id.as_str() {
        codex2api_core::CHATGPT => {
            crate::providers::chatgpt::images::prepare(body, edit, &headers)?
        }
        codex2api_core::GROK => crate::providers::grok::images::prepare(body, edit, &headers)?,
        _ => return Err(GatewayError::bad_request("Unsupported image provider")),
    };
    prepared.metadata = metadata;
    crate::execute_prepared(
        state,
        credential.principal,
        credential.token,
        headers,
        prepared,
    )
    .await
}

async fn decode(request: Request, edit: bool) -> Result<Value> {
    let content_type = request
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if content_type
        .split(';')
        .next()
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("multipart/form-data"))
    {
        if !edit {
            return Err(GatewayError::bad_request(
                "Image generation requires application/json",
            ));
        }
        let mut multipart = Multipart::from_request(request, &())
            .await
            .map_err(|_| GatewayError::bad_request("Invalid multipart image upload"))?;
        let mut body = Map::new();
        let mut images = vec![];
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|_| GatewayError::bad_request("Invalid multipart image upload"))?
        {
            let name = field.name().unwrap_or("").to_owned();
            if matches!(name.as_str(), "image" | "image[]" | "images" | "images[]") {
                if images.len() == 16 {
                    return Err(GatewayError::bad_request(
                        "At most 16 input images are supported",
                    ));
                }
                let media_type = field.content_type().unwrap_or("").to_owned();
                let has_file = field.file_name().is_some();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|_| GatewayError::bad_request("Invalid image upload"))?;
                if bytes.is_empty() || bytes.len() > 15 * 1024 * 1024 {
                    return Err(GatewayError::bad_request(
                        "Each image upload must be between 1 byte and 15 MiB",
                    ));
                }
                let url = if has_file || media_type.starts_with("image/") {
                    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                        "image/png"
                    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
                        "image/jpeg"
                    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
                        "image/webp"
                    } else {
                        return Err(GatewayError::bad_request(
                            "Input images must be PNG, JPEG or WebP",
                        ));
                    };
                    format!(
                        "data:{mime};base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(bytes)
                    )
                } else {
                    String::from_utf8(bytes.to_vec()).map_err(|_| {
                        GatewayError::bad_request("Image references must be URLs or data URLs")
                    })?
                };
                images.push(json!({"image_url":url}));
            } else {
                if name.is_empty() || body.contains_key(&name) || field.file_name().is_some() {
                    return Err(GatewayError::bad_request(
                        "Unknown or duplicate image form field",
                    ));
                }
                let value = field
                    .text()
                    .await
                    .map_err(|_| GatewayError::bad_request("Invalid image form field"))?;
                if value.len() > 32_000 {
                    return Err(GatewayError::bad_request("Image form field is too long"));
                }
                let value =
                    if name == "n" {
                        Value::from(
                            value
                                .parse::<u64>()
                                .map_err(|_| GatewayError::bad_request("n must be an integer"))?,
                        )
                    } else if name == "stream" || name == "deferred" {
                        Value::from(value.parse::<bool>().map_err(|_| {
                            GatewayError::bad_request("Expected a boolean form field")
                        })?)
                    } else {
                        Value::String(value)
                    };
                body.insert(name, value);
            }
        }
        body.insert("images".into(), Value::Array(images));
        Ok(Value::Object(body))
    } else {
        let headers = request.headers().clone();
        let bytes = to_bytes(request.into_body(), codex2api_upstream::MAX_REQUEST_BYTES)
            .await
            .map_err(|_| {
                GatewayError::bad_request("Image request body is too large or unreadable")
            })?;
        Ok(codex2api_upstream::decode_body(&bytes, &headers)?)
    }
}

/// Validate public common fields without selecting a provider or inventing a model.
pub(crate) fn validate(body: &Value, supported: &[&str]) -> Result<()> {
    let object = body
        .as_object()
        .ok_or_else(|| GatewayError::bad_request("Image request must be an object"))?;
    for (name, value) in object {
        if !value.is_null() && !supported.contains(&name.as_str()) {
            return Err(GatewayError::bad_request(format!(
                "Image parameter '{name}' is not supported by this platform"
            )));
        }
    }
    for (name, limit) in [("model", 256), ("prompt", 32_000)] {
        if !body[name]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.len() <= limit)
        {
            return Err(GatewayError::bad_request(format!(
                "{name} must be a nonempty string (maximum {limit} bytes)"
            )));
        }
    }
    if body
        .get("n")
        .is_some_and(|v| !v.is_null() && !v.as_u64().is_some_and(|n| (1..=10).contains(&n)))
    {
        return Err(GatewayError::bad_request(
            "n must be an integer from 1 to 10",
        ));
    }
    for name in ["stream", "deferred"] {
        if body.get(name).is_some_and(|v| !v.is_null() && v != false) {
            return Err(GatewayError::bad_request(
                "Image streaming and deferred jobs are not supported",
            ));
        }
    }
    Ok(())
}

pub(crate) fn choice(body: &Value, name: &str, choices: &[&str]) -> Result<()> {
    if body.get(name).is_some_and(|value| {
        !value.is_null() && !value.as_str().is_some_and(|v| choices.contains(&v))
    }) {
        return Err(GatewayError::bad_request(format!(
            "Unsupported image {name}"
        )));
    }
    Ok(())
}

pub(crate) fn references(body: &mut Value, edit: bool, maximum: usize) -> Result<Vec<Value>> {
    let object = body.as_object_mut().expect("validated image object");
    let images = object.remove("images");
    let image = object.remove("image");
    if images.as_ref().is_some_and(|v| !v.is_null()) && image.as_ref().is_some_and(|v| !v.is_null())
    {
        return Err(GatewayError::bad_request(
            "Provide image or images, not both",
        ));
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
        return Err(GatewayError::bad_request(format!(
            "Image editing requires 1 to {maximum} input images"
        )));
    }
    if !edit && !refs.is_empty() {
        return Err(GatewayError::bad_request(
            "Input images require /v1/images/edits",
        ));
    }
    refs.into_iter()
        .map(|value| {
            if let Some(file_id) = value.get("file_id") {
                let file_id = file_id
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| {
                        GatewayError::bad_request("file_id must be a nonempty string")
                    })?;
                if value.get("image_url").is_some() || value.get("url").is_some() {
                    return Err(GatewayError::bad_request(
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
                    GatewayError::bad_request("Each input image requires image_url or url")
                })?;
            if image.len() > 20 * 1024 * 1024 || image.chars().any(char::is_control) {
                return Err(GatewayError::bad_request("Invalid image reference"));
            }
            if image.starts_with("data:") {
                let (mime, encoded) = image.split_once(";base64,").ok_or_else(|| {
                    GatewayError::bad_request("Input images require a base64 data URL")
                })?;
                if !matches!(
                    mime,
                    "data:image/png" | "data:image/jpeg" | "data:image/webp"
                ) || encoded.is_empty()
                    || base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .is_err()
                {
                    return Err(GatewayError::bad_request("Invalid base64 image data"));
                }
            } else {
                let url = url::Url::parse(image)
                    .map_err(|_| GatewayError::bad_request("Invalid image URL"))?;
                if !matches!(url.scheme(), "https" | "http")
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                {
                    return Err(GatewayError::bad_request(
                        "Input image URLs must use HTTP or HTTPS without credentials",
                    ));
                }
            }
            Ok(json!({"image_url":image}))
        })
        .collect()
}

pub(crate) fn prepared(mut body: Value, edit: bool, headers: http::HeaderMap) -> PreparedRequest {
    body.as_object_mut()
        .expect("validated image object")
        .retain(|key, value| !value.is_null() && key != "stream" && key != "deferred");
    PreparedRequest {
        headers,
        body,
        endpoint: if edit {
            "/v1/images/edits"
        } else {
            "/v1/images/generations"
        },
        upstream_path: if edit {
            "images/edits"
        } else {
            "images/generations"
        },
        output: OutputMode::Images,
        stream: false,
        include_usage: false,
        metadata: None,
        legacy_functions: false,
    }
}

/// Count returned images only. An unreadable or truncated response stays unknown.
pub(crate) fn observe_usage(record: &mut codex2api_storage::UsageRecord, value: &Value) {
    let Some(images) = value["data"].as_array() else {
        if !value["error"].is_null() {
            record.image_count = Some(0);
            record.image_usage_json = Some("[]".into());
        }
        return;
    };
    if let Some(size) = value["size"]
        .as_str()
        .or_else(|| value["resolution"].as_str())
    {
        record.image_size = Some(size.chars().take(64).collect());
    }
    let normalize = |size: &str| {
        codex2api_storage::image_resolution(size)
            .or_else(|| codex2api_storage::image_resolution_tier(size))
    };
    let mut counts = std::collections::BTreeMap::<Option<String>, i64>::new();
    for item in images {
        if !item["error"].is_null()
            || !["b64_json", "url"]
                .iter()
                .any(|name| item[name].as_str().is_some_and(|v| !v.is_empty()))
        {
            continue;
        }
        let resolution = item["width"]
            .as_u64()
            .zip(item["height"].as_u64())
            .and_then(|(w, h)| normalize(&format!("{w}x{h}")))
            .or_else(|| item["size"].as_str().and_then(normalize))
            .or_else(|| item["resolution"].as_str().and_then(normalize))
            .or_else(|| {
                item["b64_json"]
                    .as_str()
                    .and_then(codex2api_upstream::data_url_resolution)
            })
            .or_else(|| record.image_size.as_deref().and_then(normalize));
        *counts.entry(resolution).or_default() += 1;
    }
    let usage = counts
        .into_iter()
        .map(|(resolution, count)| codex2api_storage::ImageUsage { resolution, count })
        .collect::<Vec<_>>();
    record.image_count = Some(usage.iter().map(|v| v.count).sum());
    record.image_usage_json = serde_json::to_string(&usage).ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    fn multipart_request(fields: &[(&str, &str)], uploads: &[(&str, &[u8])]) -> Request {
        let mut body = Vec::new();
        for (name, value) in fields {
            body.extend_from_slice(format!("--api-image-boundary\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
        }
        for (name, bytes) in uploads {
            body.extend_from_slice(format!("--api-image-boundary\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"image.png\"\r\nContent-Type: image/png\r\n\r\n").as_bytes());
            body.extend_from_slice(bytes);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(b"--api-image-boundary--\r\n");
        Request::builder()
            .method("POST")
            .uri("/v1/images/edits")
            .header(
                "content-type",
                "multipart/form-data; boundary=api-image-boundary",
            )
            .body(Body::from(body))
            .unwrap()
    }

    #[tokio::test]
    async fn standard_multipart_edits_use_each_providers_official_json_shape() {
        let png = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aWQAAAABJRU5ErkJggg==").unwrap();
        let body = decode(
            multipart_request(
                &[
                    ("model", "custom-image"),
                    ("prompt", "change background"),
                    ("n", "2"),
                    ("stream", "false"),
                ],
                &[("image[]", &png)],
            ),
            true,
        )
        .await
        .unwrap();
        assert_eq!(body["n"], 2);
        let chatgpt =
            crate::providers::chatgpt::images::prepare(body.clone(), true, &http::HeaderMap::new())
                .unwrap();
        assert_eq!(chatgpt.upstream_path, "images/edits");
        assert!(
            chatgpt.body["images"][0]["image_url"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert!(chatgpt.body.get("stream").is_none());
        let grok =
            crate::providers::grok::images::prepare(body, true, &http::HeaderMap::new()).unwrap();
        assert_eq!(grok.body["image"]["type"], "image_url");
        assert_eq!(
            grok.body["image"]["url"],
            chatgpt.body["images"][0]["image_url"]
        );
        assert!(grok.body.get("images").is_none());
    }

    #[tokio::test]
    async fn multipart_rejects_duplicate_fields_non_images_and_generation_uploads() {
        let duplicates = multipart_request(&[("model", "first"), ("model", "second")], &[]);
        assert!(decode(duplicates, true).await.is_err());
        let invalid_image =
            multipart_request(&[("model", "custom")], &[("image", b"not an image")]);
        assert!(decode(invalid_image, true).await.is_err());
        let generation = multipart_request(&[("model", "custom")], &[]);
        assert!(decode(generation, false).await.is_err());
    }

    #[test]
    fn null_image_alias_does_not_hide_the_supplied_image() {
        let mut body = json!({"images":null,"image":"https://example.test/input.png"});
        assert_eq!(
            references(&mut body, true, 5).unwrap(),
            vec![json!({"image_url":"https://example.test/input.png"})]
        );
    }

    #[test]
    fn image_usage_counts_outputs_and_keeps_unknown_sizes() {
        let mut record = codex2api_storage::UsageRecord::default();
        observe_usage(
            &mut record,
            &json!({"data":[{"url":"https://example.test/1.png","width":1024,"height":1024},{"b64_json":"unknown"},{"error":{"code":"failed"},"url":"invalid"}]}),
        );
        assert_eq!(record.image_count, Some(2));
        let rows: Vec<codex2api_storage::ImageUsage> =
            serde_json::from_str(record.image_usage_json.as_deref().unwrap()).unwrap();
        assert!(rows.iter().any(|r| r.resolution.is_none() && r.count == 1));
        assert!(
            rows.iter()
                .any(|r| r.resolution.as_deref() == Some("1024x1024") && r.count == 1)
        );
        let mut missing = codex2api_storage::UsageRecord::default();
        observe_usage(&mut missing, &json!({"status":"pending"}));
        assert_eq!(missing.image_count, None);
    }
}
