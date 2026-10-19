//! When a failed download is worth running again.
//!
//! Ported from the mobile app's `service/RetryPolicy.kt`. The rules are pure
//! so they can be unit-tested without a download running, which is the only
//! way to test a timing policy honestly.

use std::time::Duration;

use crate::errors::FailureKind;

/// How many times a job may be re-run for the same transport failure before
/// it settles as failed and waits for the user.
pub const MAX_AUTO_ATTEMPTS: u32 = 3;

/// How long to wait before the next attempt.
///
/// The first retry is quick: a dropped Wi-Fi link is usually back within a
/// second or two. The later ones are not, because by then the network is
/// probably gone for good and a fast retry just burns the budget.
pub fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(match attempt {
        1 => 2,
        2 => 5,
        _ => 12,
    })
}

/// Whether a failure is worth running again, and how long to wait first.
///
/// Only a transport failure qualifies. A login gate, a photo post and a
/// stale extractor each need something the app cannot do by itself: cookies,
/// a different link, or a newer engine. The engine self-heal is handled by
/// the caller, because it is not a retry and costs the user nothing.
pub fn retry_after(kind: FailureKind, attempts_used: u32) -> Option<Duration> {
    if kind != FailureKind::Network {
        return None;
    }
    if attempts_used >= MAX_AUTO_ATTEMPTS {
        return None;
    }
    Some(backoff(attempts_used + 1))
}

/// The job's progress, kept between runs so a retry can tell whether the
/// download actually got any further.
///
/// A run that ends where the last one ended is the same failure twice, and
/// the budget must run out. A run that got further is a different failure,
/// and the budget goes back to full.
#[derive(Default)]
pub struct Progress {
    item: u64,
    fraction: f64,
}

impl Progress {
    /// Record a progress tick and report whether the job moved forward.
    ///
    /// `item` is the playlist item index, absent for a single video. A new
    /// item is forward progress even at zero percent, and a restart of the
    /// same item below the last mark is not.
    pub fn advanced(&mut self, item: Option<u64>, fraction: Option<f64>) -> bool {
        let item = item.unwrap_or(0);
        let fraction = fraction.unwrap_or(0.0);
        let moved = if item > self.item {
            true
        } else if item == self.item {
            fraction > self.fraction
        } else {
            false
        };
        if moved {
            self.item = item;
            self.fraction = fraction;
        }
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transport_failure_is_retried_until_the_budget_is_spent() {
        assert_eq!(retry_after(FailureKind::Network, 0), Some(backoff(1)));
        assert_eq!(retry_after(FailureKind::Network, 1), Some(backoff(2)));
        assert_eq!(retry_after(FailureKind::Network, 2), Some(backoff(3)));
        assert_eq!(retry_after(FailureKind::Network, 3), None);
    }

    #[test]
    fn a_failure_that_cannot_fix_itself_is_not_retried() {
        for kind in [
            FailureKind::LoginRequired,
            FailureKind::UnsupportedContent,
            FailureKind::StaleEngine,
            FailureKind::Other,
        ] {
            assert_eq!(retry_after(kind, 0), None, "{kind:?} was retried");
        }
    }

    #[test]
    fn the_wait_grows_between_attempts() {
        assert!(backoff(1) < backoff(2));
        assert!(backoff(2) < backoff(3));
    }

    #[test]
    fn progress_above_the_last_mark_is_forward_progress() {
        let mut p = Progress::default();
        assert!(p.advanced(None, Some(0.5)));
        assert!(!p.advanced(None, Some(0.2)));
        assert!(p.advanced(None, Some(0.7)));
    }

    #[test]
    fn a_new_playlist_item_is_forward_progress() {
        let mut p = Progress::default();
        assert!(p.advanced(Some(1), Some(0.5)));
        assert!(p.advanced(Some(2), Some(0.0)));
    }

    #[test]
    fn a_retry_that_restarts_the_same_item_is_not_forward_progress() {
        // The download died at half way. The retry starts from the beginning
        // of that item, which is where the last one stopped, so the budget
        // must not be handed back.
        let mut p = Progress::default();
        assert!(p.advanced(Some(2), Some(0.5)));
        assert!(!p.advanced(Some(2), Some(0.1)));
    }
}
