use sha2::{Digest, Sha256};

/// One-way identifier for OAuth tokens; never log or persist plaintext access tokens.
pub fn hash_token(token: &str) -> String {
    to_hex(Sha256::digest(token.as_bytes()).as_slice())
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
