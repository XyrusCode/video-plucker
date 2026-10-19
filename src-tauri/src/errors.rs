//! Download-failure classification.
//!
//! Ported from the mobile app's failure-UX overhaul: raw yt-dlp output is
//! mapped to plain-language guidance, and known "the bundled yt-dlp got too
//! old for this site" signatures are flagged so a failed download can
//! self-heal by updating the engine. The raw text always survives for
//! issue reports — users just never see it in the UI.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// The site demands a logged-in session; cookies fix it.
    LoginRequired,
    /// A TikTok photo/slideshow post, which yt-dlp cannot download.
    UnsupportedContent,
    /// The signature matches a known stale-engine breakage.
    StaleEngine,
    /// The transport failed: the network went away mid-download. Another
    /// attempt can work, so this is the one kind that is retried.
    Network,
    /// Anything else.
    Other,
}

impl FailureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FailureKind::LoginRequired => "login_required",
            FailureKind::UnsupportedContent => "unsupported_content",
            FailureKind::StaleEngine => "stale_engine",
            FailureKind::Network => "network",
            FailureKind::Other => "other",
        }
    }

    fn rank(self) -> u8 {
        match self {
            FailureKind::LoginRequired => 4,
            FailureKind::UnsupportedContent => 3,
            FailureKind::StaleEngine => 2,
            FailureKind::Network => 1,
            FailureKind::Other => 0,
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "login_required" => FailureKind::LoginRequired,
            "unsupported_content" => FailureKind::UnsupportedContent,
            "stale_engine" => FailureKind::StaleEngine,
            "network" => FailureKind::Network,
            _ => FailureKind::Other,
        }
    }
}

/// What gets attached to `pluck://error` / `pluck://done` payloads.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifiedFailure {
    pub kind: &'static str,
    pub friendly: String,
}

/// Keep whichever explanation best describes why the job died: the most
/// specific kind wins; ties go to the latest error.
pub fn worse(
    a: Option<(FailureKind, String)>,
    b: Option<(FailureKind, String)>,
) -> Option<(FailureKind, String)> {
    match (a, b) {
        (None, x) => x,
        (x, None) => x,
        (Some(x), Some(y)) => Some(if y.0.rank() >= x.0.rank() { y } else { x }),
    }
}

/// Turn a stored kind string back into an enum (for resume paths).
pub fn kind_of(s: &str) -> FailureKind {
    FailureKind::from_str(s)
}

const TIKTOK_PHOTO_MSG: &str =
    "TikTok photo posts aren't supported yet — try an audio-only quality or a different link.";

const STALE_ENGINE_MSG: &str =
    "The bundled downloader looks out of date for this site. Updating it and retrying…";

const GENERIC_MSG: &str = "Download failed.";

const NETWORK_MSG: &str = "The network dropped while this was downloading.";

/// Signatures that mean "this needs cookies / a logged-in browser session".
const LOGIN_SIGNATURES: &[&str] = &[
    "sign in to confirm",
    "confirm you're not a bot",
    "only available to registered users",
    "http error 401",
    "--cookies-from-browser or --cookies",
    "login required",
    "log in required",
    "you need to log in",
    "please log in",
    "requires authentication",
    "members-only",
    "join this channel",
    "confirm your age",
    "age-restricted",
    // Chrome holds the cookie DB while running (yt-dlp #7271); the bundled
    // ChromeCookieUnlock plugin usually clears this, otherwise cookies.txt does.
    "could not copy",
    "cookie database",
];

/// Signatures that historically mean the bundled yt-dlp rotted against a
/// changed site — an engine update usually fixes them.
const STALE_SIGNATURES: &[&str] = &[
    "unable to extract",
    "failed to extract",
    "nsig extraction failed",
    "signature extraction failed",
    "http error 400",
    "http error 403",
    "http error 429",
    "too many requests",
    "rate limit",
    "captcha",
    "impersonat", // impersonate / impersonator
    "update yt-dlp",
];

/// Transport-level failures, ported from the mobile app's FailureClassifier.
///
/// yt-dlp's own retry budget does not cover a network *interface* going away:
/// the socket fails immediately with a hard error rather than a retryable
/// HTTP status, so `--retries` burns through in seconds and the job dies.
///
/// The list must not overlap [`STALE_SIGNATURES`]. A misclassified network
/// error costs a pointless engine update, and a misclassified engine error
/// costs one extra retry.
const NETWORK_SIGNATURES: &[&str] = &[
    // Transport-level errno values as Python's socket/urllib surface them.
    "errno 101", // ENETUNREACH
    "errno 104", // ECONNRESET
    "errno 105", // ENETDOWN
    "errno 110", // ETIMEDOUT
    "errno 111", // ECONNREFUSED
    "errno 113", // EHOSTUNREACH
    "errno 32",  // EPIPE
    "errno -2",  // gaierror: name or service not known
    "errno -3",  // gaierror: temporary failure in name resolution
    "errno -4",  // gaierror: unknown error
    "errno -5",  // gaierror: no address associated with hostname
    "network is unreachable",
    "network is down",
    "no route to host",
    "name or service not known",
    "temporary failure in name resolution",
    "no address associated with hostname",
    "getaddrinfo",
    "connection timed out",
    "connection reset by peer",
    "connection refused",
    "connection aborted",
    "broken pipe",
    "timed out",
    // yt-dlp's own wrappers around a failed fetch. "unable to download" covers
    // the webpage, the video data and the extractor api page.
    "unable to download",
    "incomplete data received",
    "incompleteread",
    "unexpected eof",
];

/// True when the message carries an HTTP status of the given class:
/// "http error 404" is a 4xx, "http error 503" a 5xx.
fn has_http_status(lower: &str, class: char) -> bool {
    const NEEDLE: &str = "http error ";
    lower.match_indices(NEEDLE).any(|(i, _)| {
        let rest = lower[i + NEEDLE.len()..].as_bytes();
        rest.first() == Some(&(class as u8)) && rest.get(1).is_some_and(|b| b.is_ascii_digit())
    })
}

/// True when this yt-dlp failure means "the network went away", as opposed
/// to "the content is unreachable" or "the engine is out of date".
///
/// A 4xx is never a transport failure: the site answered, and the answer was
/// that the content is not there. A 5xx is: the origin is down or overloaded,
/// so holding and trying again is the right move.
fn is_network(lower: &str) -> bool {
    if has_http_status(lower, '4') {
        return false;
    }
    if has_http_status(lower, '5') {
        return true;
    }
    NETWORK_SIGNATURES.iter().any(|s| lower.contains(s))
}

/// Pretty platform name for a URL, used in login-required guidance.
fn platform_name(url: &str) -> Option<&'static str> {
    let lower = url.to_lowercase();
    if lower.contains("youtube.com") || lower.contains("youtu.be") {
        Some("YouTube")
    } else if lower.contains("twitter.com") || lower.contains("x.com") {
        Some("X (Twitter)")
    } else if lower.contains("tiktok.com") {
        Some("TikTok")
    } else if lower.contains("instagram.com") {
        Some("Instagram")
    } else if lower.contains("facebook.com") || lower.contains("fb.watch") {
        Some("Facebook")
    } else if lower.contains("reddit.com") {
        Some("Reddit")
    } else if lower.contains("vk.com") || lower.contains("vk.ru") || lower.contains("vkvideo.ru") {
        Some("VK")
    } else {
        None
    }
}

fn login_friendly(url: Option<&str>) -> String {
    match url.and_then(platform_name) {
        Some(name) => format!(
            "{name} requires a logged-in account to download this. Import a cookies.txt \
             for the site in the Cookie Manager below, then try again."
        ),
        None => String::from(
            "This site requires a logged-in account. Import its cookies.txt in the \
             Cookie Manager below, then try again.",
        ),
    }
}

/// Classify one raw yt-dlp error line (or resolver error) against the
/// known signature tables. `url` gives context for content-specific hints.
pub fn classify(raw: &str, url: Option<&str>) -> (FailureKind, String) {
    let lower = raw.to_lowercase();

    if lower.contains("unsupported url") {
        if let Some(u) = url {
            if u.contains("/photo/") {
                return (FailureKind::UnsupportedContent, TIKTOK_PHOTO_MSG.into());
            }
        }
    }

    if LOGIN_SIGNATURES.iter().any(|s| lower.contains(s)) {
        return (FailureKind::LoginRequired, login_friendly(url));
    }

    // A stale extractor is a more specific cause than a transport failure, and
    // refreshing the engine is the cheaper fix, so it wins when a line
    // matches both. The engine self-heal is handled by the caller.
    if STALE_SIGNATURES.iter().any(|s| lower.contains(s)) {
        return (FailureKind::StaleEngine, STALE_ENGINE_MSG.into());
    }

    if is_network(&lower) {
        return (FailureKind::Network, NETWORK_MSG.into());
    }

    (FailureKind::Other, GENERIC_MSG.into())
}

/// Convenience wrapper producing the serializable payload form.
pub fn classified(raw: &str, url: Option<&str>) -> ClassifiedFailure {
    let (kind, friendly) = classify(raw, url);
    ClassifiedFailure {
        kind: kind.as_str(),
        friendly,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_check_is_login_required() {
        let (kind, _) = classify(
            "ERROR: [youtube] abc: Sign in to confirm you're not a bot.",
            Some("https://youtube.com/watch?v=abc"),
        );
        assert_eq!(kind, FailureKind::LoginRequired);
    }

    #[test]
    fn http_401_is_login_required_with_platform_name() {
        let (_, friendly) = classify(
            "ERROR: unable to download video data: HTTP Error 401: Unauthorized",
            Some("https://www.tiktok.com/@user/video/123"),
        );
        assert!(friendly.starts_with("TikTok requires"));
    }

    #[test]
    fn nsig_failure_is_stale_engine() {
        let (kind, _) = classify(
            "ERROR: nsig extraction failed: some player issue",
            None,
        );
        assert_eq!(kind, FailureKind::StaleEngine);
    }

    #[test]
    fn tiktok_photo_is_unsupported_content() {
        let (kind, friendly) = classify(
            "ERROR: Unsupported URL: https://www.tiktok.com/@u/photo/7123",
            Some("https://www.tiktok.com/@u/photo/7123"),
        );
        assert_eq!(kind, FailureKind::UnsupportedContent);
        assert_eq!(friendly, TIKTOK_PHOTO_MSG);
    }

    #[test]
    fn unsupported_url_without_photo_context_is_other() {
        let (kind, _) = classify(
            "ERROR: Unsupported URL: https://example.com/x",
            Some("https://example.com/x"),
        );
        assert_eq!(kind, FailureKind::Other);
    }

    #[test]
    fn unknown_error_is_other() {
        let (kind, friendly) = classify("ERROR: something exploded", None);
        assert_eq!(kind, FailureKind::Other);
        assert_eq!(friendly, GENERIC_MSG);
    }

    #[test]
    fn a_dropped_connection_is_a_network_failure() {
        let (kind, friendly) = classify(
            "ERROR: unable to download video data: <urlopen error [Errno 101] Network is unreachable>",
            None,
        );
        assert_eq!(kind, FailureKind::Network);
        assert_eq!(friendly, NETWORK_MSG);
    }

    #[test]
    fn a_timeout_is_a_network_failure() {
        let (kind, _) = classify(
            "ERROR: unable to download video data: <urlopen error [Errno 110] Connection timed out>",
            None,
        );
        assert_eq!(kind, FailureKind::Network);
    }

    #[test]
    fn a_server_error_is_a_network_failure() {
        let (kind, _) = classify(
            "ERROR: unable to download webpage: HTTP Error 503: Service Unavailable",
            None,
        );
        assert_eq!(kind, FailureKind::Network);
    }

    #[test]
    fn a_missing_page_is_not_a_network_failure() {
        // The site answered. Running it again gets the same answer.
        let (kind, _) = classify(
            "ERROR: unable to download webpage: HTTP Error 404: Not Found",
            None,
        );
        assert_eq!(kind, FailureKind::Other);
    }

    #[test]
    fn a_401_is_still_login_required() {
        let (kind, _) = classify(
            "ERROR: unable to download webpage: HTTP Error 401: Unauthorized",
            Some("https://www.youtube.com/watch?v=abc"),
        );
        assert_eq!(kind, FailureKind::LoginRequired);
    }

    #[test]
    fn the_network_list_does_not_overlap_the_stale_list() {
        // A misclassified network error costs a pointless engine update, and a
        // misclassified engine error costs one extra retry.
        for network in NETWORK_SIGNATURES {
            for stale in STALE_SIGNATURES {
                assert!(
                    !stale.contains(network),
                    "{network:?} and {stale:?} overlap"
                );
                assert!(
                    !network.contains(stale),
                    "{network:?} and {stale:?} overlap"
                );
            }
        }
    }

    #[test]
    fn worse_prefers_higher_rank_and_latest_on_tie() {
        let login = (FailureKind::LoginRequired, "a".into());
        let stale = (FailureKind::StaleEngine, "b".into());
        let other = (FailureKind::Other, "c".into());
        let merged = worse(Some(stale.clone()), Some(login.clone())).unwrap();
        assert_eq!(merged.0, FailureKind::LoginRequired);
        let merged = worse(Some(other.clone()), Some(stale.clone())).unwrap();
        assert_eq!(merged.0, FailureKind::StaleEngine);
        let merged = worse(None, Some(login)).unwrap();
        assert_eq!(merged.0, FailureKind::LoginRequired);
    }
}
