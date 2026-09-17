use rand::RngCore;
use sha2::{Digest, Sha256};

/// Visible prefix stored alongside the hash so keys can be identified in the admin UI.
pub const PROXY_API_KEY_PREFIX_LEN: usize = 12;

/// Token prefix for issued proxy API keys.
pub const PROXY_API_KEY_TOKEN_PREFIX: &str = "c2a_";

pub fn generate_proxy_api_key() -> String {
    let mut bytes = [0u8; 24];
    rand::rng().fill_bytes(&mut bytes);
    format!("{PROXY_API_KEY_TOKEN_PREFIX}{}", to_hex(&bytes))
}

pub fn hash_api_key(token: &str) -> String {
    to_hex(Sha256::digest(token.as_bytes()).as_slice())
}

pub fn proxy_api_key_prefix(token: &str) -> String {
    token.chars().take(PROXY_API_KEY_PREFIX_LEN).collect()
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}
