//! Source platform classification.
//!
//! One classifier feeds everything that cares where a link came from:
//! the ledger's `platform` field, the per-source subdirectory names and
//! issue reports. The folder name is deliberately display-safe on every
//! platform (no separators, no punctuation beyond the OS limits).

/// Union of every signal we have: the link itself and, when present,
/// yt-dlp's extractor key (e.g. `TikTok`, `YoutubeTab`, `Twitter`).
pub fn platform_for(url: &str, extractor: Option<&str>) -> String {
    let lower = url.to_lowercase();
    let ext = extractor.unwrap_or("").to_lowercase();
    let has = |needle: &str| lower.contains(needle) || ext.contains(needle);

    if has("youtu") {
        "YouTube".to_string()
    } else if has("twitter") || has("x.com") {
        "X".to_string()
    } else if has("tiktok") {
        "TikTok".to_string()
    } else if has("instagram") {
        "Instagram".to_string()
    } else if has("facebook") || has("fb.watch") {
        "Facebook".to_string()
    } else if has("reddit") {
        "Reddit".to_string()
    } else if has("vk.com") || has("vk.ru") || has("vkvideo") {
        "VK".to_string()
    } else if has("soundcloud") {
        "SoundCloud".to_string()
    } else if has("luciferdonghua") {
        "LuciferDonghua".to_string()
    } else if has("allanime") {
        "AllAnime".to_string()
    } else if has("lulacloud") {
        "LulaCloud".to_string()
    } else if !ext.is_empty() {
        // yt-dlp extractors we have never heard of: keep their own name
        // (sanitised) so the file still lands somewhere stable and legible.
        sanitise_folder(extractor.unwrap_or("Other"))
    } else {
        "Other".to_string()
    }
}

/// Folder-safe rendering of any name: alphanumerics, dash and underscore.
pub fn sanitise_folder(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "Other".to_string()
    } else {
        trimmed.chars().take(64).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_sites_map_to_display_names() {
        assert_eq!(platform_for("https://www.youtube.com/watch?v=a", None), "YouTube");
        assert_eq!(platform_for("https://youtu.be/a", None), "YouTube");
        assert_eq!(platform_for("https://x.com/user/status/1", None), "X");
        assert_eq!(platform_for("https://twitter.com/user/status/1", None), "X");
        assert_eq!(platform_for("https://www.tiktok.com/@u/video/1", None), "TikTok");
        assert_eq!(platform_for("https://www.reddit.com/r/x/comments/1/t", None), "Reddit");
        assert_eq!(platform_for("https://soundcloud.com/a/b", None), "SoundCloud");
    }

    #[test]
    fn extractor_key_wins_when_the_url_is_unhelpful() {
        assert_eq!(platform_for("https://cdn.example.com/x", Some("YoutubeTab")), "YouTube");
        assert_eq!(platform_for("https://cdn.example.com/x", Some("SoundCloudIE")), "SoundCloud");
    }

    #[test]
    fn unknown_sites_land_in_other() {
        assert_eq!(platform_for("https://example.com/a", None), "Other");
        assert_eq!(platform_for("https://example.com/a", Some("SomeWeirdExtractor")), "SomeWeirdExtractor");
    }

    #[test]
    fn folder_names_are_filesystem_safe() {
        assert_eq!(sanitise_folder("My/Site:Name?"), "My_Site_Name");
        assert_eq!(sanitise_folder("///"), "Other");
    }
}
