//! Deobfuscation helpers shared by the extractors.

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
