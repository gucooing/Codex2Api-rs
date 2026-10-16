//! The pinned Codex ImagesClient sends JSON image references and receives base64 data.
use crate::{ApiError, PreparedRequest, Result, images};
use serde_json::Value;

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
            "background",
            "quality",
            "size",
            "image",
            "images",
            "response_format",
            "stream",
        ],
    )?;
    images::choice(&body, "background", &["auto", "opaque", "transparent"])?;
    images::choice(&body, "quality", &["auto", "low", "medium", "high"])?;
    images::choice(&body, "response_format", &["b64_json"])?;
    if body.get("size").is_some_and(|v| {
        !v.is_null()
            && !v
                .as_str()
                .is_some_and(|s| s == "auto" || codex2api_storage::image_resolution(s).is_some())
    }) {
        return Err(ApiError::bad_request("size must be auto or WIDTHxHEIGHT"));
    }
    let refs = images::references(&mut body, edit, 16)?;
    if edit {
        body["images"] = refs.into();
    }
    body.as_object_mut()
        .expect("validated image body")
        .remove("response_format");
    Ok(images::prepared(body, edit, super::image_headers(inbound)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn edit_uses_pinned_codex_image_references() {
        let request = prepare(json!({"model":"custom-image","prompt":"edit","image":{"url":"https://example.test/image.png"},"n":2,"response_format":"b64_json"}),true, &http::HeaderMap::new()).unwrap();
        assert_eq!(
            request.body["images"],
            json!([{"image_url":"https://example.test/image.png"}])
        );
        assert!(request.body.get("image").is_none());
        assert!(request.body.get("response_format").is_none());
        assert_eq!(request.upstream_path, "images/edits");
        assert!(prepare(json!({"model":"custom-image","prompt":"edit","image":"https://example.test/image.png","mask":"https://example.test/mask.png"}),true, &http::HeaderMap::new()).is_err());
        assert!(
            prepare(
                json!({"model":"custom-image","prompt":"draw","response_format":"url"}),
                false,
                &http::HeaderMap::new()
            )
            .is_err()
        );
    }
}
