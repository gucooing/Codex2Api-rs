//! xAI image JSON is sent through the existing isolated Grok subscription transport.
use crate::{ApiError, PreparedRequest, Result, images};
use serde_json::{Value, json};

pub(crate) fn prepare(
    mut body: Value,
    edit: bool,
    inbound: &http::HeaderMap,
) -> Result<PreparedRequest> {
    images::validate(
        &body,
        &[
            "model",
            "prompt",
            "n",
            "quality",
            "resolution",
            "aspect_ratio",
            "size",
            "image",
            "images",
            "response_format",
            "stream",
            "deferred",
        ],
    )?;
    images::choice(&body, "quality", &["auto", "low", "medium"])?;
    images::choice(&body, "response_format", &["url", "b64_json"])?;
    if let Some(size) = body
        .as_object_mut()
        .unwrap()
        .remove("size")
        .filter(|v| !v.is_null())
    {
        let size = size
            .as_str()
            .ok_or_else(|| ApiError::bad_request("size must be a string"))?;
        if size != "auto" {
            if body.get("resolution").is_some_and(|v| !v.is_null())
                || body.get("aspect_ratio").is_some_and(|v| !v.is_null())
            {
                return Err(ApiError::bad_request(
                    "Use size or resolution/aspect_ratio, not both",
                ));
            }
            let normalized = codex2api_storage::image_resolution(size)
                .ok_or_else(|| ApiError::bad_request("size must be auto or WIDTHxHEIGHT"))?;
            let (w, h) = normalized.split_once('x').unwrap();
            let (w, h): (u32, u32) = (w.parse().unwrap(), h.parse().unwrap());
            let (mut a, mut b) = (w, h);
            while b != 0 {
                (a, b) = (b, a % b);
            }
            body["aspect_ratio"] = format!("{}:{}", w / a, h / a).into();
            body["resolution"] = match w.max(h) {
                1..=1024 => "1k",
                1025..=2048 => "2k",
                _ => {
                    return Err(ApiError::bad_request(
                        "Grok supports 1k or 2k image resolution",
                    ));
                }
            }
            .into();
        }
    }
    // Pin the documented default so admission and billed size use the same request.
    if body.get("resolution").is_none_or(Value::is_null) {
        body["resolution"] = "1k".into();
    }
    images::choice(&body, "resolution", &["1k", "2k"])?;
    images::choice(
        &body,
        "aspect_ratio",
        &[
            "auto", "1:1", "16:9", "9:16", "4:3", "3:4", "3:2", "2:3", "2:1", "1:2", "19.5:9",
            "9:19.5", "20:9", "9:20", "21:9", "5:2",
        ],
    )?;
    let refs = images::references(&mut body, edit, 5)?;
    if refs.iter().any(|image| image.get("file_id").is_some()) {
        return Err(ApiError::bad_request(
            "Grok image edits require image URLs or base64 data, not OpenAI file IDs",
        ));
    }
    if refs.len() == 1 {
        body["image"] = json!({"url":refs[0]["image_url"],"type":"image_url"});
    } else if edit {
        body["images"] = Value::Array(
            refs.into_iter()
                .map(|image| json!({"url":image["image_url"],"type":"image_url"}))
                .collect(),
        );
    }
    Ok(images::prepared(
        body,
        edit,
        super::outbound_headers(inbound),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grok_image_json_and_size_are_provider_owned() {
        let request = prepare(json!({"model":"custom-image","prompt":"edit","images":[{"image_url":"https://example.test/a.png"},{"image_url":"https://example.test/b.png"}],"size":"1024x1024"}),true, &http::HeaderMap::new()).unwrap();
        assert_eq!(request.body["resolution"], "1k");
        assert_eq!(request.body["aspect_ratio"], "1:1");
        assert_eq!(
            request.body["images"][0],
            json!({"url":"https://example.test/a.png","type":"image_url"})
        );
        assert!(request.body.get("size").is_none());
        assert!(
            prepare(
                json!({"model":"custom-image","prompt":"draw","deferred":true}),
                false,
                &http::HeaderMap::new()
            )
            .is_err()
        );
        assert!(
            prepare(
                json!({"model":"custom-image","prompt":"draw","background":"transparent"}),
                false,
                &http::HeaderMap::new()
            )
            .is_err()
        );
    }
}
