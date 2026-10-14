//! Shared HTTP response framing. Inspect bytes before selecting a JSON or SSE reader.
use http::HeaderMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseFormat {
    Json,
    EventStream,
    Other,
}

impl ResponseFormat {
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let media = headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .unwrap_or("")
            .trim();
        if media.eq_ignore_ascii_case("text/event-stream") {
            Self::EventStream
        } else if media.eq_ignore_ascii_case("application/json") {
            Self::Json
        } else {
            Self::Other
        }
    }

    /// None means the initial framing field or UTF-8 BOM crosses a chunk boundary.
    /// JSON strings containing `data:` are not SSE framing.
    pub fn from_prefix(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() || (bytes.len() < 3 && b"\xef\xbb\xbf".starts_with(bytes)) {
            return None;
        }
        let prefix = bytes
            .strip_prefix(b"\xef\xbb\xbf")
            .unwrap_or(bytes)
            .trim_ascii_start();
        if prefix.is_empty() {
            return None;
        }
        if matches!(prefix[0], b'{' | b'[') {
            return Some(Self::Json);
        }
        let fields: &[&[u8]] = &[b"data:", b"event:", b"id:", b"retry:", b":"];
        if fields.iter().any(|field| prefix.starts_with(field)) {
            Some(Self::EventStream)
        } else if fields.iter().any(|field| field.starts_with(prefix)) {
            None
        } else {
            Some(Self::Other)
        }
    }
}
