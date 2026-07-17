//! Deobfuscation helpers shared by the extractors.

use aes::cipher::{KeyIvInit, StreamCipher};
use base64::Engine;
use sha2::{Digest, Sha256};

type Aes256Ctr = ctr::Ctr128BE<aes::Aes256>;

/// AllAnime wraps some episode-source responses in a base64, AES-256-CTR
/// encrypted `tobeparsed` field. Key = SHA256("Xot36i3lK3:v1"); the decoded
/// blob is `[skip 1][12-byte IV][ciphertext][16-byte GCM tag]` and the CTR
/// counter is `IV || 0x00000002` (GCM's second counter block — the tag is
/// dropped, not verified). Returns the decrypted UTF-8 plaintext.
pub fn allanime_tobeparsed(blob_b64: &str) -> Option<String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(blob_b64.trim())
        .ok()?;
    // 1 leading byte + 12-byte IV + >=1 ciphertext byte + 16-byte tag.
    if raw.len() < 1 + 12 + 16 + 1 {
        return None;
    }
    let mut counter = [0u8; 16];
    counter[..12].copy_from_slice(&raw[1..13]);
    counter[12..].copy_from_slice(&[0, 0, 0, 2]);

    let key = Sha256::digest(b"Xot36i3lK3:v1");
    let mut buf = raw[13..raw.len() - 16].to_vec();
    let mut cipher = Aes256Ctr::new_from_slices(&key, &counter).ok()?;
    cipher.apply_keystream(&mut buf);
    String::from_utf8(buf).ok()
}

/// AllAnime hides each `sourceUrl` as a hex string XOR'd byte-for-byte with
/// this key. Decoding yields a provider path like `/apivtwo/clock?id=...`.
/// Kept as a named constant so a future key change is a one-line fix.
const ALLANIME_XOR_KEY: u8 = 0x38;

/// Decode an AllAnime obfuscated source. Input is the raw `sourceUrl`; if it
/// starts with `--` the remainder is hex XOR'd with [`ALLANIME_XOR_KEY`].
/// Non-obfuscated URLs (already `http...`) are returned unchanged.
pub fn allanime_source(source_url: &str) -> Option<String> {
    let hex = match source_url.strip_prefix("--") {
        Some(h) => h,
        None => {
            return if source_url.starts_with("http") {
                Some(source_url.to_string())
            } else {
                None
            };
        }
    };
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut out = String::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let byte = u8::from_str_radix(std::str::from_utf8(&bytes[i..i + 2]).ok()?, 16).ok()?;
        out.push((byte ^ ALLANIME_XOR_KEY) as char);
        i += 2;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a path the way AllAnime does, to prove the round-trip.
    fn encode(path: &str) -> String {
        let mut s = String::from("--");
        for b in path.bytes() {
            s.push_str(&format!("{:02x}", b ^ ALLANIME_XOR_KEY));
        }
        s
    }

    #[test]
    fn round_trips_a_clock_path() {
        let path = "/apivtwo/clock?id=abc123";
        assert_eq!(allanime_source(&encode(path)).as_deref(), Some(path));
    }

    #[test]
    fn passes_through_plain_urls() {
        let url = "https://example.com/x.m3u8";
        assert_eq!(allanime_source(url).as_deref(), Some(url));
    }

    #[test]
    fn rejects_odd_length_hex() {
        assert_eq!(allanime_source("--abc"), None);
    }
}
