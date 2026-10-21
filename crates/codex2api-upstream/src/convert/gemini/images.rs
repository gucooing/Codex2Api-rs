//! Images API -> the Antigravity desktop image tool's Gemini wire, and back.
use crate::inbound::{Error, InboundRequest, Result, images};
use crate::supply::{Operation, Wire, WireRequest};
use serde_json::{Value, json};

pub(crate) fn prepare(input: &InboundRequest) -> Result<WireRequest> {
    let mut body = input.body.clone();
    images::validate(
        &body,
        &[
            "model",
            "prompt",
            "n",
            "size",
            "aspect_ratio",
            "image",
            "images",
            "response_format",
            "stream",
            "deferred",
        ],
    )?;
    images::choice(&body, "response_format", &["b64_json"])?;
    images::choice(&body, "size", &["auto"])?;
    images::choice(
        &body,
        "aspect_ratio",
        &[
            "1:1", "1:4", "1:8", "2:3", "3:2", "3:4", "4:1", "4:3", "4:5", "5:4", "8:1", "9:16",
            "16:9", "21:9",
        ],
    )?;
    if body["n"].as_u64().is_some_and(|n| n != 1) {
        return Err(Error::bad_request(
            "Antigravity generates one image per request; n must be 1",
        ));
    }
    let references = images::references(&mut body, input.operation == Operation::ImageEdit, 14)?;
    let mut parts = vec![json!({"text":body["prompt"]})];
    for reference in references {
        let (mime, data) = reference["image_url"]
            .as_str()
            .and_then(|url| url.strip_prefix("data:"))
            .and_then(|url| url.split_once(";base64,"))
            .ok_or_else(|| {
                Error::bad_request(
                    "Antigravity image edits require base64 data URLs or multipart image uploads",
                )
            })?;
        parts.push(json!({"inlineData":{"mimeType":mime,"data":data}}));
    }
    Ok(WireRequest {
        wire: Wire::Gemini,
        operation: input.operation,
        model: input.model().map(str::to_owned),
        headers: http::HeaderMap::new(),
        stream: false,
        body: json!({"model":input.model(),"request":{
            "contents":[{"role":"user","parts":parts}],
            "generationConfig":{"candidateCount":1,"imageConfig":{
                "aspectRatio":body["aspect_ratio"].as_str().unwrap_or("1:1")
            }}
        }}),
    })
}

fn invalid(message: &str) -> Error {
    Error {
        status: http::StatusCode::BAD_GATEWAY,
        body: json!({"error":{"type":"server_error","code":"upstream_protocol_error","message":message}}),
    }
}

pub(crate) fn response(value: Value, model: &str) -> Result<Value> {
    let body = value
        .get("response")
        .filter(|v| v.is_object())
        .unwrap_or(&value);
    let candidates = body["candidates"]
        .as_array()
        .ok_or_else(|| invalid("Antigravity did not return image candidates"))?;
    let mut images = Vec::new();
    for candidate in candidates {
        if candidate["finishReason"] != "STOP" {
            return Err(invalid("Antigravity image generation did not complete"));
        }
        for part in candidate["content"]["parts"]
            .as_array()
            .into_iter()
            .flatten()
        {
            // Thinking images are intermediate work, never delivered or billed as output images.
            if part["thought"] == true {
                continue;
            }
            let Some(inline) = part.get("inlineData") else {
                continue;
            };
            let mime = inline["mimeType"]
                .as_str()
                .filter(|mime| matches!(*mime, "image/png" | "image/jpeg" | "image/webp"))
                .ok_or_else(|| invalid("Antigravity returned an unsupported image format"))?;
            let data = inline["data"]
                .as_str()
                .filter(|data| !data.is_empty())
                .ok_or_else(|| invalid("Antigravity returned empty image data"))?;
            let size = crate::data_url_resolution(data)
                .ok_or_else(|| invalid("Antigravity returned unreadable image data"))?;
            images.push(json!({"b64_json":data,"mime_type":mime,"size":size}));
        }
    }
    if images.is_empty() {
        return Err(invalid("Antigravity did not generate an image"));
    }
    let mut result = json!({"created":chrono::Utc::now().timestamp(),"model":model,"data":images});
    if body["usageMetadata"].is_object() {
        result["usage"] = super::events::usage(&body["usageMetadata"]);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inbound::Protocol;
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aWQAAAABJRU5ErkJggg==";

    #[test]
    fn edits_build_inline_media_and_reject_unsupported_controls() {
        let body = json!({"model":"custom-image","prompt":"edit","aspect_ratio":"16:9",
            "images":[{"image_url":format!("data:image/png;base64,{PNG}")}]});
        let make =
            |body| InboundRequest::new(Protocol::ImageEdit, body, http::HeaderMap::new()).unwrap();
        let request = prepare(&make(body.clone())).unwrap();
        assert_eq!(
            request.body["request"]["contents"][0]["parts"][1]["inlineData"]["data"],
            PNG
        );
        assert_eq!(
            request.body["request"]["generationConfig"],
            json!({"candidateCount":1,"imageConfig":{"aspectRatio":"16:9"}})
        );
        for (key, value) in [
            ("n", json!(2)),
            ("response_format", json!("url")),
            ("size", json!("1024x1024")),
            ("stream", json!(true)),
            ("images", json!([{"image_url":"http://127.0.0.1/private"}])),
        ] {
            let mut invalid = body.clone();
            invalid[key] = value;
            assert!(prepare(&make(invalid)).is_err(), "{key}");
        }
    }

    #[test]
    fn delivers_only_completed_images_and_measured_usage_without_supplier_metadata() {
        let raw = json!({"project":"private-project","response":{
            "candidates":[{"finishReason":"STOP","content":{"parts":[
                {"thought":true,"inlineData":{"mimeType":"image/png","data":PNG}},
                {"text":"result"},{"inlineData":{"mimeType":"image/png","data":PNG}}
            ]}}],"usageMetadata":{"promptTokenCount":100,"cachedContentTokenCount":40,
                "candidatesTokenCount":20,"thoughtsTokenCount":5}}});
        let output = response(raw.clone(), "custom-image").unwrap();
        assert_eq!(output["data"].as_array().unwrap().len(), 1);
        assert_eq!(output["data"][0]["size"], "1x1");
        assert_eq!(output["usage"]["input_tokens_details"]["cached_tokens"], 40);
        assert_eq!(output["usage"]["output_tokens"], 25);
        assert!(output.get("project").is_none());
        let mut partial = raw.clone();
        partial["response"]["candidates"][0]["finishReason"] = "MAX_TOKENS".into();
        assert!(response(partial, "custom-image").is_err());
        let mut missing = raw;
        missing["response"]["candidates"][0]["content"]["parts"] = json!([{"text":"no image"}]);
        assert!(response(missing, "custom-image").is_err());
    }
}
