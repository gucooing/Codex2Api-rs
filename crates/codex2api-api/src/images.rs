//! Image HTTP input and measured image usage, shared by the two provider adapters.
use crate::{ApiCredential, ApiError, ApiState, Result};
use axum::{
    body::to_bytes,
    extract::{Extension, FromRequest, Multipart, Request, State},
    response::Response,
};
use base64::Engine;
use serde_json::{Map, Value, json};

pub(crate) async fn generations(
    State(state): State<ApiState>,
    Extension(credential): Extension<ApiCredential>,
    request: Request,
) -> Result<Response> {
    handle(state, credential, request, false).await
}

pub(crate) async fn edits(
    State(state): State<ApiState>,
    Extension(credential): Extension<ApiCredential>,
    request: Request,
) -> Result<Response> {
    handle(state, credential, request, true).await
}

async fn handle(
    state: ApiState,
    credential: ApiCredential,
    request: Request,
    edit: bool,
) -> Result<Response> {
    let headers = request.headers().clone();
    let body = decode(request, edit).await?;
    let protocol = if edit {
        crate::Protocol::ImageEdit
    } else {
        crate::Protocol::ImageGeneration
    };
    crate::identify(&state, protocol, &body).await?;
    let prepared = crate::InboundRequest::new(protocol, body, headers.clone())?;
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
            return Err(ApiError::bad_request(
                "Image generation requires application/json",
            ));
        }
        let mut multipart = Multipart::from_request(request, &())
            .await
            .map_err(|_| ApiError::bad_request("Invalid multipart image upload"))?;
        let mut body = Map::new();
        let mut images = vec![];
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|_| ApiError::bad_request("Invalid multipart image upload"))?
        {
            let name = field.name().unwrap_or("").to_owned();
            if matches!(name.as_str(), "image" | "image[]" | "images" | "images[]") {
                if images.len() == 16 {
                    return Err(ApiError::bad_request(
                        "At most 16 input images are supported",
                    ));
                }
                let media_type = field.content_type().unwrap_or("").to_owned();
                let has_file = field.file_name().is_some();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|_| ApiError::bad_request("Invalid image upload"))?;
                if bytes.is_empty() || bytes.len() > 15 * 1024 * 1024 {
                    return Err(ApiError::bad_request(
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
                        return Err(ApiError::bad_request(
                            "Input images must be PNG, JPEG or WebP",
                        ));
                    };
                    format!(
                        "data:{mime};base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(bytes)
                    )
                } else {
                    String::from_utf8(bytes.to_vec()).map_err(|_| {
                        ApiError::bad_request("Image references must be URLs or data URLs")
                    })?
                };
                images.push(json!({"image_url":url}));
            } else {
                if name.is_empty() || body.contains_key(&name) || field.file_name().is_some() {
                    return Err(ApiError::bad_request(
                        "Unknown or duplicate image form field",
                    ));
                }
                let value = field
                    .text()
                    .await
                    .map_err(|_| ApiError::bad_request("Invalid image form field"))?;
                if value.len() > 32_000 {
                    return Err(ApiError::bad_request("Image form field is too long"));
                }
                let value = if name == "n" {
                    Value::from(
                        value
                            .parse::<u64>()
                            .map_err(|_| ApiError::bad_request("n must be an integer"))?,
                    )
                } else if name == "stream" || name == "deferred" {
                    Value::from(
                        value
                            .parse::<bool>()
                            .map_err(|_| ApiError::bad_request("Expected a boolean form field"))?,
                    )
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
            .map_err(|_| ApiError::bad_request("Image request body is too large or unreadable"))?;
        Ok(codex2api_upstream::decode_body(&bytes, &headers)?)
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
        let chatgpt = codex2api_upstream::convert::request(
            &crate::InboundRequest::new(
                crate::Protocol::ImageEdit,
                body.clone(),
                http::HeaderMap::new(),
            )
            .unwrap(),
            codex2api_upstream::supply::Wire::Codex,
        )
        .unwrap();
        assert_eq!(
            chatgpt.operation,
            codex2api_upstream::supply::Operation::ImageEdit
        );
        assert!(
            chatgpt.body["images"][0]["image_url"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert!(chatgpt.body.get("stream").is_none());
        let grok = codex2api_upstream::convert::request(
            &crate::InboundRequest::new(crate::Protocol::ImageEdit, body, http::HeaderMap::new())
                .unwrap(),
            codex2api_upstream::supply::Wire::Xai,
        )
        .unwrap();
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
            codex2api_upstream::inbound::images::references(&mut body, true, 5).unwrap(),
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
