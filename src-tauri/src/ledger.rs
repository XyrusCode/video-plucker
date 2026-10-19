//! Download ledger: every file this app has finished, keyed by the yt-dlp video id.
//!
//! The id is the `[id]` token in the output name (`%(title)s [%(id)s].%(ext)s`), so a duplicate is
//! "a ledger entry or a file in the destination folder carrying this id". File contents are never
//! read. The shape matches `DownloadLedger.kt` on Android so a dump moves between the two apps.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntry {
    pub id: String,
    pub title: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub platform: String,
    pub downloaded_at: i64,
}

/// Why a video was judged already downloaded. `source` is `"ledger"` or `"file"`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateMatch {
    pub title: String,
    pub file_name: String,
    pub source: &'static str,
    pub downloaded_at: Option<i64>,
}

/// The `[id]` token of a file name, read from the last bracket before the extension.
pub fn id_from_file_name(name: &str) -> Option<String> {
    let dot = name.rfind('.')?;
    let ext = &name[dot + 1..];
    if ext.is_empty() || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let stem = &name[..dot];
    let inner = stem.strip_suffix(']')?;
    let open = inner.rfind('[')?;
    let id = &inner[open + 1..];
    if id.is_empty() || id.contains(']') {
        return None;
    }
    Some(id.to_string())
}

/// The file name without its extension and its `[id]` token.
pub fn title_from_file_name(name: &str) -> String {
    let stem = match name.rfind('.') {
        Some(d) if d > 0 => &name[..d],
        _ => name,
    };
    let title = stem
        .strip_suffix(']')
        .and_then(|s| s.rfind('[').map(|o| s[..o].trim_end()))
        .unwrap_or(stem)
        .trim();
    if title.is_empty() {
        name.to_string()
    } else {
        title.to_string()
    }
}

/// The ledger entry for [id], or else the first file in [file_names] that carries it.
pub fn match_duplicate(id: &str, ledger: &[LedgerEntry], file_names: &[String]) -> Option<DuplicateMatch> {
    if let Some(entry) = ledger.iter().find(|e| e.id == id) {
        return Some(DuplicateMatch {
            title: entry.title.clone(),
            file_name: entry.file_name.clone(),
            source: "ledger",
            downloaded_at: Some(entry.downloaded_at),
        });
    }
    let file = file_names
        .iter()
        .find(|name| id_from_file_name(name.as_str()).as_deref() == Some(id))?;
    Some(DuplicateMatch {
        title: title_from_file_name(file),
        file_name: file.clone(),
        source: "file",
        downloaded_at: None,
    })
}

/// Union by id. For an id both sides hold, the later download wins, so an older dump never
/// rolls a record back.
pub fn merge(local: &[LedgerEntry], incoming: &[LedgerEntry]) -> Vec<LedgerEntry> {
    let mut out = local.to_vec();
    for entry in incoming {
        match out.iter().position(|e| e.id == entry.id) {
            Some(i) if entry.downloaded_at > out[i].downloaded_at => out[i] = entry.clone(),
            Some(_) => {}
            None => out.push(entry.clone()),
        }
    }
    out
}

/// Reads ledger rows from a dump or a file, skipping any row it cannot read.
pub fn from_json(value: &Value) -> Vec<LedgerEntry> {
    value
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| serde_json::from_value::<LedgerEntry>(row.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn ledger_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("ledger.json"))
}

/// Every entry on disk. A missing or unreadable file is an empty ledger, not an error.
pub fn load(app: &AppHandle) -> Vec<LedgerEntry> {
    let Ok(path) = ledger_path(app) else {
        return Vec::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .map(|value| from_json(&value))
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, entries: &[LedgerEntry]) -> Result<(), String> {
    let path = ledger_path(app)?;
    let text = serde_json::to_string_pretty(entries).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("Could not write {}: {e}", path.display()))
}

/// The file names in [dir], or none if it cannot be read.
fn file_names_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .collect()
}

#[tauri::command]
pub fn ledger_entries(app: AppHandle) -> Vec<LedgerEntry> {
    load(&app)
}

#[tauri::command]
pub fn ledger_record(app: AppHandle, entry: LedgerEntry) -> Result<(), String> {
    let merged = merge(&load(&app), std::slice::from_ref(&entry));
    save(&app, &merged)
}

#[tauri::command]
pub fn find_duplicate(app: AppHandle, id: String, dest_dir: String) -> Option<DuplicateMatch> {
    let names = file_names_in(Path::new(&dest_dir));
    match_duplicate(&id, &load(&app), &names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(id: &str, at: i64) -> LedgerEntry {
        LedgerEntry {
            id: id.into(),
            title: format!("T {id}"),
            file_name: format!("T {id} [{id}].mp4"),
            size_bytes: 1,
            platform: "YouTube".into(),
            downloaded_at: at,
        }
    }

    #[test]
    fn reads_the_id_before_the_extension() {
        assert_eq!(
            id_from_file_name("Never Gonna Give You Up [dQw4w9WgXcQ].mp4").as_deref(),
            Some("dQw4w9WgXcQ")
        );
    }

    #[test]
    fn takes_the_last_bracket_when_the_title_has_brackets() {
        assert_eq!(id_from_file_name("Live [remix] [abc-123].webm").as_deref(), Some("abc-123"));
    }

    #[test]
    fn returns_none_without_an_id_token() {
        assert_eq!(id_from_file_name("Plain title.mp4"), None);
        assert_eq!(id_from_file_name("notes.txt"), None);
        assert_eq!(id_from_file_name("Two ext [abc].tar.gz"), None);
    }

    #[test]
    fn recovers_the_title() {
        assert_eq!(title_from_file_name("Live [remix] [abc-123].webm"), "Live [remix]");
        assert_eq!(title_from_file_name("Plain title.mp4"), "Plain title");
    }

    #[test]
    fn ledger_match_wins_and_carries_the_date() {
        let m = match_duplicate("abc", &[entry("abc", 9)], &["T abc [abc].mp4".into()]).unwrap();
        assert_eq!(m.source, "ledger");
        assert_eq!(m.downloaded_at, Some(9));
    }

    #[test]
    fn file_match_works_with_an_empty_ledger() {
        let m = match_duplicate("abc", &[], &["Title [abc].mp4".into()]).unwrap();
        assert_eq!(m.source, "file");
        assert_eq!(m.file_name, "Title [abc].mp4");
    }

    #[test]
    fn an_id_that_only_shares_a_prefix_is_not_a_match() {
        assert!(match_duplicate("abc", &[], &["x [abcd].mp4".into()]).is_none());
    }

    #[test]
    fn merge_keeps_the_newer_record_and_unions_the_rest() {
        let merged = merge(&[entry("a", 10)], &[entry("a", 20), entry("b", 5)]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].downloaded_at, 20);
        assert_eq!(merged[1].id, "b");
    }

    #[test]
    fn merge_keeps_the_local_record_when_it_is_newer() {
        let merged = merge(&[entry("a", 30)], &[entry("a", 20)]);
        assert_eq!(merged[0].downloaded_at, 30);
    }

    #[test]
    fn from_json_skips_a_row_it_cannot_read() {
        let value = json!([
            { "id": "a", "title": "A", "fileName": "A [a].mp4", "sizeBytes": 1, "platform": "x", "downloadedAt": 1 },
            { "title": "no id" }
        ]);
        assert_eq!(from_json(&value).len(), 1);
    }
}
