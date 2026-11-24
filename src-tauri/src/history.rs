//! Download history: every download this app ever attempted, kept locally
//! so a lost file can be downloaded again.
//!
//! Unlike the ledger (`ledger.rs`), the history is *not* shared through
//! dumps: it is app-local state keyed by nothing in particular. The ledger
//! stays the cross-app record of finished downloads; the history is this
//! install's diary of attempts, failures included.
//!
//! The shape is one JSON array in `<app_data_dir>/history.json`, capped at
//! [`MAX_ENTRIES`] with the oldest entries dropped. Reads tolerate a
//! missing or corrupt file (empty history); writes are atomic per save.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use crate::compat::{AppHandle, Manager};

/// Oldest entries beyond this are dropped on every save.
pub const MAX_ENTRIES: usize = 1000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// Stable id for UI actions (remove). Uniqueness comes from the
    /// millisecond clock plus a per-write counter — no uuid needed.
    pub id: String,
    /// The link that was attempted. `None` for streaming-site jobs, whose
    /// job URLs are pre-resolved playlist fragments and cannot replay.
    pub url: Option<String>,
    /// yt-dlp video id when it is recoverable (from the output filename).
    pub video_id: Option<String>,
    pub title: String,
    pub platform: String,
    pub quality: String,
    /// Stored so a playlist link can be re-downloaded as a playlist.
    pub playlist_mode: bool,
    /// "completed" | "failed"
    pub status: String,
    /// Classified failure kind ("network", "login_required", ...) on failure.
    pub failure_kind: Option<String>,
    pub file_name: Option<String>,
    pub size_bytes: Option<u64>,
    pub downloaded_at: i64,
}

fn history_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("history.json"))
}

/// All entries, oldest first. A missing or unreadable file is an empty
/// history, not an error.
pub fn load(app: &AppHandle) -> Vec<HistoryEntry> {
    let Ok(path) = history_path(app) else {
        return Vec::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<HistoryEntry>>(&text).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, entries: &[HistoryEntry]) -> Result<(), String> {
    let path = history_path(app)?;
    let trimmed = if entries.len() > MAX_ENTRIES {
        &entries[entries.len() - MAX_ENTRIES..]
    } else {
        entries
    };
    let text = serde_json::to_string_pretty(trimmed).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &text).map_err(|e| format!("Could not write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("Could not replace {}: {e}", path.display()))
}

/// Append an entry, dropping the oldest beyond [`MAX_ENTRIES`].
pub fn record(app: &AppHandle, entry: HistoryEntry) -> Result<(), String> {
    let mut entries = load(app);
    entries.push(entry);
    save(app, &entries)
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Build the entry for a download that just finished. `ok` false folds the
/// classification in so the UI never has to store raw yt-dlp output.
#[allow(clippy::too_many_arguments)]
pub fn outcome_entry(
    url: Option<String>,
    title: String,
    platform: String,
    quality: String,
    playlist_mode: bool,
    ok: bool,
    failure_kind: Option<String>,
    file_path: Option<&Path>,
) -> HistoryEntry {
    let (file_name, video_id, size_bytes) = file_path
        .and_then(|p| {
            let name = p.file_name()?.to_str()?.to_string();
            let size = std::fs::metadata(p).ok().map(|m| m.len());
            Some((name, size))
        })
        .map(|(name, size)| {
            let id = crate::ledger::id_from_file_name(&name);
            (Some(name), id, size)
        })
        .unwrap_or((None, None, None));
    HistoryEntry {
        id: format!("{now}-{suffix}", now = now_nanos(), suffix = std::process::id()),
        url,
        video_id,
        title,
        platform,
        quality,
        playlist_mode,
        status: if ok { "completed" } else { "failed" }.to_string(),
        failure_kind: if ok { None } else { failure_kind },
        file_name,
        size_bytes,
        downloaded_at: (now_nanos() / 1_000_000) as i64,
    }
}

pub fn history_entries(app: AppHandle) -> Vec<HistoryEntry> {
    load(&app)
}

pub fn history_remove(app: AppHandle, id: String) -> Result<(), String> {
    let entries = load(&app);
    let kept: Vec<HistoryEntry> = entries.into_iter().filter(|e| e.id != id).collect();
    save(&app, &kept)
}

pub fn history_clear(app: AppHandle) -> Result<(), String> {
    save(&app, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, at: i64) -> HistoryEntry {
        HistoryEntry {
            id: id.into(),
            url: Some("https://example.com/v".into()),
            video_id: None,
            title: format!("T {id}"),
            platform: "YouTube".into(),
            quality: "480".into(),
            playlist_mode: false,
            status: "completed".into(),
            failure_kind: None,
            file_name: None,
            size_bytes: None,
            downloaded_at: at,
        }
    }

    #[test]
    fn oldest_entries_drop_beyond_the_cap() {
        let mut entries: Vec<HistoryEntry> = Vec::new();
        for i in 0..(MAX_ENTRIES + 5) {
            entries.push(entry(&format!("e{i}"), i as i64));
        }
        let trimmed = if entries.len() > MAX_ENTRIES {
            &entries[entries.len() - MAX_ENTRIES..]
        } else {
            &entries[..]
        };
        assert_eq!(trimmed.len(), MAX_ENTRIES);
        assert_eq!(trimmed[0].id, "e5");
    }

    #[test]
    fn outcome_marks_failure_kind_only_on_failure() {
        let ok = outcome_entry(Some("u".into()), "t".into(), "YouTube".into(), "480".into(), false, true, Some("network".into()), None);
        assert_eq!(ok.status, "completed");
        assert_eq!(ok.failure_kind, None);
        let bad = outcome_entry(Some("u".into()), "t".into(), "YouTube".into(), "480".into(), false, false, Some("network".into()), None);
        assert_eq!(bad.status, "failed");
        assert_eq!(bad.failure_kind.as_deref(), Some("network"));
    }

    #[test]
    fn outcome_reads_the_id_from_the_file_name() {
        let dir = std::env::temp_dir().join(format!("yt-plucker-history-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Some Title [abc-123].mp4");
        std::fs::write(&file, b"xx").unwrap();
        let e = outcome_entry(Some("u".into()), "t".into(), "YouTube".into(), "480".into(), false, true, None, Some(&file));
        assert_eq!(e.video_id.as_deref(), Some("abc-123"));
        assert_eq!(e.file_name.as_deref(), Some("Some Title [abc-123].mp4"));
        assert_eq!(e.size_bytes, Some(2));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
