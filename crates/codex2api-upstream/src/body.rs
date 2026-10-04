//! Provider-neutral bounded JSON decoding and ledger metadata.
use crate::{Result, UpstreamError};
use http::HeaderMap;
use serde_json::Value;
use std::io::Read;
pub const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;

/// Read only non-content metadata for the incoming usage ledger (including zstd bodies).
#[derive(Default)]
pub struct RequestMetadata {
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub service_tier: Option<String>,
    pub image_size: Option<String>,
    pub image_input_sizes: Option<Vec<String>>,
    pub generate: Option<bool>,
}

pub fn request_metadata(body: &[u8], headers: &HeaderMap) -> Result<RequestMetadata> {
    let value = decode_body(body, headers)?;
    let image_input_sizes = value
        .get("images")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(image_input_resolution)
                .collect::<Vec<_>>()
        })
        .or_else(|| {
            value
                .get("image")
                .and_then(image_input_resolution)
                .map(|size| vec![size])
        })
        .filter(|items| !items.is_empty());
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
        image_input_sizes,
        generate: value.get("generate").and_then(Value::as_bool),
    })
}

fn image_input_resolution(value: &Value) -> Option<String> {
    if let Some(encoded) = value.as_str() {
        return data_url_resolution(encoded);
    }
    let object = value.as_object()?;
    if let Some(size) = object.get("size").and_then(Value::as_str)
        && let Some(size) = codex2api_storage::image_resolution(size)
    {
        return Some(size);
    }
    if let (Some(width), Some(height)) = (
        object.get("width").and_then(Value::as_u64),
        object.get("height").and_then(Value::as_u64),
    ) {
        return codex2api_storage::image_resolution(&format!("{width}x{height}"));
    }
    let nested_url = object
        .get("image_url")
        .and_then(|value| value.get("url"))
        .and_then(Value::as_str);
    let encoded = ["image_url", "image", "url"]
        .iter()
        .filter_map(|key| object.get(*key).and_then(Value::as_str))
        .chain(nested_url)
        .find_map(|value| {
            value
                .strip_prefix("data:")?
                .split_once(",")
                .map(|(_, data)| data)
        });
    data_url_resolution(encoded?)
}

fn data_url_resolution(encoded: &str) -> Option<String> {
    let encoded = encoded
        .strip_prefix("data:")
        .and_then(|value| value.split_once(",").map(|(_, data)| data))
        .unwrap_or(encoded);
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded).ok()?;
    image_dimensions(&bytes)
        .map(|(width, height)| format!("{width}x{height}"))
        .and_then(|size| codex2api_storage::image_resolution(&size))
}

fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 24 {
        return Some((
            u32::from_be_bytes(bytes[16..20].try_into().ok()?),
            u32::from_be_bytes(bytes[20..24].try_into().ok()?),
        ));
    }
    if (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) && bytes.len() >= 10 {
        return Some((
            u16::from_le_bytes(bytes[6..8].try_into().ok()?) as u32,
            u16::from_le_bytes(bytes[8..10].try_into().ok()?) as u32,
        ));
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        if bytes.get(12..16) == Some(b"VP8X") && bytes.len() >= 30 {
            let width =
                1 + (u32::from_le_bytes([bytes[24], bytes[25], bytes[26], 0]) & 0x00ff_ffff);
            let height =
                1 + (u32::from_le_bytes([bytes[27], bytes[28], bytes[29], 0]) & 0x00ff_ffff);
            return Some((width, height));
        }
    }
    if bytes.first() == Some(&0xff) && bytes.get(1) == Some(&0xd8) {
        let mut index = 2;
        while index + 9 < bytes.len() {
            if bytes[index] != 0xff {
                index += 1;
                continue;
            }
            while index < bytes.len() && bytes[index] == 0xff {
                index += 1;
            }
            let marker = *bytes.get(index)?;
            index += 1;
            if matches!(marker, 0xd8 | 0xd9) {
                continue;
            }
            let length = u16::from_be_bytes(bytes.get(index..index + 2)?.try_into().ok()?) as usize;
            if length < 2 || index + length > bytes.len() {
                return None;
            }
            if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
                let height = u16::from_be_bytes(bytes.get(index + 3..index + 5)?.try_into().ok()?);
                let width = u16::from_be_bytes(bytes.get(index + 5..index + 7)?.try_into().ok()?);
                return Some((width as u32, height as u32));
            }
            index += length;
        }
    }
    None
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
