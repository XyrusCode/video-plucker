//! Portable dump of everything that would not survive a reinstall, in the same shape the
//! Android app writes.
//!
//! Schema (identical to `SettingsDump.kt` on Android):
//! ```json
//! {
//!   "dumpVersion": 3,
//!   "appVersion": "4.8.0",
//!   "exportedAt": 1792341101993,
//!   "prefs": { "auto_update": true, ... },
//!   "cookieFiles": { "x.txt": "# Netscape HTTP Cookie File\n..." },
//!   "queue": [ { "itemId": "...", "url": "...", "quality": "1080", "title": "...",
//!                "thumbnailUrl": "", "addedAt": 0 } ]
//! }
//! ```
//!
//! The dump contains live session cookies. Treat the file like a password.
//!
//! Two things about the contract are easy to get wrong and are pinned by tests below:
//!
//! - `cookieFiles` is keyed by **filename**, not by platform. Android stores X as `x.txt`
//!   under the platform key `twitter`, so a reader cannot infer the platform from the name
//!   without the reverse of that table. Fortunately the desktop matcher already serves `x`
//!   for both x.com and twitter.com, so writing the filename straight through is correct.
//! - A dump with no `queue` key does **not** clear the parked list here. Android replaces its
//!   queue on import because it owns the whole list; this merges instead, so picking a file
//!   can never silently delete downloads the user still has.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::AppHandle;

use crate::ledger::{self, LedgerEntry};

pub const DUMP_VERSION: u32 = 3;

/// Pref keys the desktop app owns and can therefore round-trip.
///
/// `terms_accepted` is deliberately absent. The app keeps it in localStorage, not in the
/// settings store, so there is nothing to export and importing it would write a key that
/// nothing ever reads.
///
/// `cookies_*` and `cookie_file_*` are absent for a different reason: both are per-device, and
/// `cookie_file_*` carries an absolute path from the exporting machine. Android's
/// `CookieResolver` would read it, find no file, and fall through, while it looked configured.
/// Cookie files travel as `cookieFiles` content, and each side derives its own path on arrival.
const EXPORTED_PREFS: &[(&str, &str)] = &[("auto_update", "autoUpdate")];

/// Quality ids the desktop engine understands. Android's `Quality` enum has one more
/// (`image`) with no desktop equivalent; it is mapped to `best` rather than dropped so a
/// restored item still downloads as something.
fn desktop_quality(id: &str) -> &str {
    const KNOWN: [&str; 8] = [
        "best", "2160", "1440", "1080", "720", "480", "mp3", "m4a",
    ];
    if KNOWN.contains(&id) {
        id
    } else {
        "best"
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct DumpCounts {
    #[serde(rename = "prefsKeys")]
    pub prefs_keys: usize,
    #[serde(rename = "cookieFiles")]
    pub cookie_files: usize,
    #[serde(rename = "queueItems")]
    pub queue_items: usize,
    /// Parked items left out because they are streaming-site picks, not URLs.
    #[serde(rename = "skippedQueueItems")]
    pub skipped_queue_items: usize,
    #[serde(rename = "ledgerEntries")]
    pub ledger_entries: usize,
    #[serde(rename = "appVersion")]
    pub app_version: String,
}

#[derive(Serialize)]
struct Dump {
    #[serde(rename = "dumpVersion")]
    dump_version: u32,
    #[serde(rename = "appVersion")]
    app_version: String,
    #[serde(rename = "exportedAt")]
    exported_at: i64,
    #[serde(rename = "sourcePlatform")]
    source_platform: &'static str,
    prefs: Map<String, Value>,
    #[serde(rename = "cookieFiles")]
    cookie_files: BTreeMap<String, String>,
    queue: Vec<QueueItem>,
    ledger: Vec<LedgerEntry>,
}

#[derive(Serialize)]
struct QueueItem {
    #[serde(rename = "itemId")]
    item_id: String,
    url: String,
    quality: String,
    title: String,
    #[serde(rename = "thumbnailUrl")]
    thumbnail_url: String,
    #[serde(rename = "addedAt")]
    added_at: i64,
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Reads every `*.txt` cookie profile as `filename -> contents`.
fn read_cookie_files(dir: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if let Ok(contents) = std::fs::read_to_string(&path) {
            out.insert(name.to_string(), contents);
        }
    }
    out
}

/// Only flat, sane filenames are accepted back.
///
/// A whitelist, not a denylist. Rejecting `/`, `\` and `..` is not enough on Windows: a child
/// path that carries a drive prefix but no root, such as `C:evil.txt`, *replaces* the path it
/// is joined onto rather than nesting inside it, so it lands outside the cookies directory.
/// `""` and `.` name the directory itself. A dump is a file the user chose, and choosing one
/// is exactly the moment an attacker-supplied path arrives.
///
/// Dots are allowed inside a name because `sanitize_cookie_name` permits them (a profile named
/// `my.site` is legitimate); they cannot traverse once no separator can appear.
fn is_safe_cookie_file_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".txt") else {
        return false;
    };
    !stem.is_empty()
        && stem.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// Builds a dump from the app's on-disk state.
///
/// `settings` is the frontend's settings store, passed in so this stays a pure function of
/// its arguments and can be tested without a `tauri::AppHandle`.
pub fn build_dump(
    app_version: &str,
    settings: &Map<String, Value>,
    cookies_dir: &Path,
    parked: &[(String, Value)],
    ledger: &[LedgerEntry],
) -> (String, DumpCounts) {
    let mut prefs = Map::new();
    for (dump_key, store_key) in EXPORTED_PREFS {
        if let Some(v) = settings.get(*store_key) {
            prefs.insert((*dump_key).to_string(), v.clone());
        }
    }
    // Android records the last version it told the user about, so a restored phone does not
    // re-announce a release the user has already dismissed.
    let last_notified = settings
        .get("lastNotifiedVersion")
        .and_then(|v| v.as_str())
        .unwrap_or(app_version);
    prefs.insert(
        "last_notified_version".to_string(),
        Value::String(last_notified.to_string()),
    );

    let cookie_files = read_cookie_files(cookies_dir);

    let mut queue = Vec::new();
    let mut skipped = 0usize;
    for (key, item) in parked {
        let Some(params) = item.get("params") else {
            continue;
        };
        // A streaming-site pick has no URL to hand to yt-dlp; it identifies a show and an
        // episode on someone else's site, so it cannot survive the trip to another machine.
        if params.get("kind").and_then(|k| k.as_str()) == Some("stream") {
            skipped += 1;
            continue;
        }
        let Some(url) = params.get("url").and_then(|u| u.as_str()).filter(|u| !u.is_empty())
        else {
            continue;
        };
        queue.push(QueueItem {
            item_id: item
                .get("jobId")
                .and_then(|j| j.as_u64())
                .map(|j| j.to_string())
                .unwrap_or_else(|| key.clone()),
            url: url.to_string(),
            quality: desktop_quality(params.get("quality").and_then(|q| q.as_str()).unwrap_or("best"))
                .to_string(),
            title: params
                .get("title")
                .and_then(|t| t.as_str())
                .filter(|t| !t.is_empty())
                .unwrap_or(url)
                .to_string(),
            thumbnail_url: String::new(),
            added_at: item.get("parkedAt").and_then(|p| p.as_i64()).unwrap_or(0),
        });
    }

    let counts = DumpCounts {
        prefs_keys: prefs.len(),
        cookie_files: cookie_files.len(),
        queue_items: queue.len(),
        skipped_queue_items: skipped,
        ledger_entries: ledger.len(),
        app_version: app_version.to_string(),
    };
    let dump = Dump {
        dump_version: DUMP_VERSION,
        app_version: app_version.to_string(),
        exported_at: now_millis(),
        source_platform: "desktop",
        prefs,
        cookie_files,
        queue,
        ledger: ledger.to_vec(),
    };
    // Serialising this shape cannot fail, and a silent fallback would write an empty file
    // while the caller reports a successful export of everything it counted.
    let json = serde_json::to_string_pretty(&dump).expect("dump is plain owned data");
    (json, counts)
}

/// The parked-list entries a restored dump produces, ready to write into the queue store.
///
/// Returns only the entries; the caller merges them with what is already parked.
pub fn restored_queue_items(dump: &Value) -> Vec<Value> {
    let Some(queue) = dump.get("queue").and_then(|q| q.as_array()) else {
        return Vec::new();
    };
    queue
        .iter()
        .filter_map(|entry| {
            let url = entry.get("url").and_then(|u| u.as_str()).filter(|u| !u.is_empty())?;
            let item_id = entry
                .get("itemId")
                .and_then(|i| i.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("restored")
                .to_string();
            let quality = desktop_quality(
                entry.get("quality").and_then(|q| q.as_str()).unwrap_or("best"),
            );
            let title = entry
                .get("title")
                .and_then(|t| t.as_str())
                .filter(|t| !t.is_empty())
                .unwrap_or(url);
            Some(serde_json::json!({
                "jobId": item_id,
                "params": {
                    "url": url,
                    "quality": quality,
                    "title": title,
                    "destDir": "",
                    "playlistMode": false,
                    "itemCount": 1,
                },
                "titles": [],
                "completed": 0,
                "failureKind": Value::Null,
                "friendly": "Restored from backup",
                "rawErrors": "",
                "parkedAt": entry.get("addedAt").and_then(|a| a.as_i64()).unwrap_or_else(now_millis),
            }))
        })
        .collect()
}

/// Prefs a dump carries that the desktop store can apply, in `dump key -> store key` form.
///
/// `last_update_check` is accepted as a number because Android writes a float when the value
/// has been through a JS layer, and a dump written by serde here writes an integer.
pub fn restored_prefs(dump: &Value) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    let Some(prefs) = dump.get("prefs").and_then(|p| p.as_object()) else {
        return out;
    };
    for (dump_key, store_key) in EXPORTED_PREFS {
        if let Some(v) = prefs.get(*dump_key) {
            out.insert((*store_key).to_string(), v.clone());
        }
    }
    if let Some(v) = prefs.get("last_notified_version").and_then(|v| v.as_str()) {
        out.insert(
            "lastNotifiedVersion".to_string(),
            Value::String(v.to_string()),
        );
    }
    out
}

/// Cookie files a dump carries, with unsafe names removed.
pub fn restored_cookie_files(dump: &Value) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(files) = dump.get("cookieFiles").and_then(|c| c.as_object()) else {
        return out;
    };
    for (name, contents) in files {
        if !is_safe_cookie_file_name(name) {
            continue;
        }
        if let Some(text) = contents.as_str() {
            out.insert(name.clone(), text.to_string());
        }
    }
    out
}

pub fn app_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
pub fn export_dump(
    app: AppHandle,
    path: String,
    settings: serde_json::Value,
    parked: Vec<(String, serde_json::Value)>,
) -> Result<DumpCounts, String> {
    let settings_map: Map<String, Value> = match settings {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    let (json, counts) = build_dump(
        &app_version(&app),
        &settings_map,
        &crate::pluck::app_cookies_dir(&app),
        &parked,
        &ledger::load(&app),
    );
    std::fs::write(&path, json).map_err(|e| format!("Could not write {path}: {e}"))?;
    Ok(counts)
}

/// What an import produced, in one object.
///
/// Deliberately a named struct rather than a tuple: a Rust tuple serialises to a JSON
/// array, and the frontend reads `result.prefsKeys` off it.
#[derive(Serialize)]
pub struct ImportResult {
    #[serde(flatten)]
    pub counts: DumpCounts,
    /// The `dump key -> store key` values the caller should persist.
    pub prefs: Value,
    /// Parked-queue entries to merge into the caller's queue store.
    pub queue: Value,
}

#[tauri::command]
pub fn import_dump(app: AppHandle, path: String) -> Result<ImportResult, String> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("Could not read {path}: {e}"))?;
    let dump: Value = serde_json::from_str(&text)
        .map_err(|e| format!("{path} is not a Video Plucker dump: {e}"))?;

    let cookies_dir = crate::pluck::app_cookies_dir(&app);
    let cookie_files = restored_cookie_files(&dump);
    for (name, contents) in &cookie_files {
        std::fs::write(cookies_dir.join(name), contents)
            .map_err(|e| format!("Could not write cookie file {name}: {e}"))?;
    }

    let prefs = restored_prefs(&dump);
    let queue = restored_queue_items(&dump);
    // Absent ledger = nothing to add; the local ledger is never cleared by an import.
    let incoming = ledger::from_json(&dump["ledger"]);
    let merged = ledger::merge(&ledger::load(&app), &incoming);
    ledger::save(&app, &merged)?;
    let counts = DumpCounts {
        prefs_keys: prefs.len(),
        cookie_files: cookie_files.len(),
        queue_items: queue.len(),
        skipped_queue_items: 0,
        ledger_entries: incoming.len(),
        app_version: dump
            .get("appVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    };
    Ok(ImportResult {
        counts,
        prefs: serde_json::to_value(prefs).map_err(|e| e.to_string())?,
        queue: serde_json::to_value(queue).map_err(|e| e.to_string())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ytp-dump-test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_cookie(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    // ------------------------------------------------------------------ export

    #[test]
    fn writes_the_schema_the_android_reader_expects() {
        let dir = temp_dir("schema");
        write_cookie(&dir, "x.txt", "# Netscape HTTP Cookie File\n.x.com\tTRUE\t/\tTRUE\t0\tk\tv\n");
        let settings = json!({ "autoUpdate": true, "termsAccepted": true, "destDir": "C:/vids" });
        let (text, _) = build_dump("4.8.0", settings.as_object().unwrap(), &dir, &[], &[]);
        let dump: Value = serde_json::from_str(&text).unwrap();

        assert_eq!(dump["dumpVersion"], 3);
        assert_eq!(dump["appVersion"], "4.8.0");
        assert!(dump["exportedAt"].as_i64().unwrap() > 0);
        assert!(dump["prefs"].is_object());
        assert!(dump["cookieFiles"].is_object());
        assert!(dump["queue"].is_array());
    }

    #[test]
    fn maps_prefs_to_the_keys_android_reads() {
        let dir = temp_dir("prefs");
        let settings = json!({ "autoUpdate": false });
        let (text, _) = build_dump("4.8.0", settings.as_object().unwrap(), &dir, &[], &[]);
        let dump: Value = serde_json::from_str(&text).unwrap();

        assert_eq!(dump["prefs"]["auto_update"], false);
        // The destination folder is desktop-specific and points at a path that may not exist
        // on the receiving machine.
        assert!(dump["prefs"].get("destDir").is_none());
    }

    #[test]
    fn does_not_carry_terms_acceptance() {
        // The app keeps it in localStorage, so there is nothing in the settings store to
        // export. Carrying it would mean writing a key on import that nothing reads.
        let dir = temp_dir("terms");
        let settings = json!({ "autoUpdate": true, "termsAccepted": true });
        let (text, _) = build_dump("4.8.0", settings.as_object().unwrap(), &dir, &[], &[]);
        let dump: Value = serde_json::from_str(&text).unwrap();
        assert!(dump["prefs"].get("terms_accepted").is_none());
    }

    #[test]
    fn still_carries_android_terms_acceptance_into_nothing() {
        // Android writes it; desktop has nowhere to put it. Dropping an unknown-to-this-app
        // key is the correct behaviour, not a failure.
        let dump = json!({ "prefs": { "terms_accepted": true } });
        assert!(restored_prefs(&dump).is_empty());
    }

    #[test]
    fn never_writes_a_cookie_file_path_pref() {
        // Android's CookieResolver reads cookie_file_<key> and would find nothing on this
        // machine. A stale path reads as "configured" and then fails at download time.
        let dir = temp_dir("stale-path");
        write_cookie(&dir, "x.txt", "body");
        let (text, _) = build_dump("4.8.0", &Map::new(), &dir, &[], &[]);
        let dump: Value = serde_json::from_str(&text).unwrap();

        let prefs = dump["prefs"].as_object().unwrap();
        assert!(prefs.keys().all(|k| !k.starts_with("cookie_")));
        assert!(prefs.keys().all(|k| !k.starts_with("cookies_")));
    }

    #[test]
    fn keys_cookie_files_by_filename() {
        let dir = temp_dir("names");
        write_cookie(&dir, "x.txt", "x body");
        write_cookie(&dir, "vk_video.txt", "vk body");
        write_cookie(&dir, "notes.md", "ignored");
        let (text, counts) = build_dump("4.8.0", &Map::new(), &dir, &[], &[]);
        let dump: Value = serde_json::from_str(&text).unwrap();

        assert_eq!(counts.cookie_files, 2);
        assert_eq!(dump["cookieFiles"]["x.txt"], "x body");
        assert_eq!(dump["cookieFiles"]["vk_video.txt"], "vk body");
        assert!(dump["cookieFiles"].get("notes.md").is_none());
    }

    #[test]
    fn leaves_streaming_site_items_out_of_the_queue() {
        let dir = temp_dir("stream");
        let parked = vec![
            (
                "1".to_string(),
                json!({ "jobId": 1, "parkedAt": 5, "params": { "url": "https://x.com/a", "quality": "720", "title": "A" } }),
            ),
            (
                "2".to_string(),
                json!({ "jobId": 2, "parkedAt": 6, "params": { "kind": "stream", "title": "A Show", "episodes": [1, 2] } }),
            ),
        ];
        let (_, counts) = build_dump("4.8.0", &Map::new(), &dir, &parked, &[]);

        assert_eq!(counts.queue_items, 1);
        assert_eq!(counts.skipped_queue_items, 1);
    }

    #[test]
    fn writes_queue_items_with_the_fields_android_requires() {
        let dir = temp_dir("queue-fields");
        let parked = vec![(
            "job-42".to_string(),
            json!({ "jobId": 42, "parkedAt": 1700000000, "params": { "url": "https://youtu.be/x", "quality": "1080", "title": "Clip" } }),
        )];
        let (text, _) = build_dump("4.8.0", &Map::new(), &dir, &parked, &[]);
        let dump: Value = serde_json::from_str(&text).unwrap();
        let item = &dump["queue"][0];

        // itemId and url are the only two the Android reader hard-requires.
        assert_eq!(item["itemId"], "42");
        assert_eq!(item["url"], "https://youtu.be/x");
        assert_eq!(item["quality"], "1080");
        assert_eq!(item["title"], "Clip");
        // thumbnailUrl must be a string, never null: the reader takes it with optString and
        // a JSON null becomes the literal "null".
        assert_eq!(item["thumbnailUrl"], "");
        assert_eq!(item["addedAt"], 1700000000);
    }

    // ------------------------------------------------------------------ import

    #[test]
    fn maps_android_image_quality_to_something_desktop_can_do() {
        // Android's Quality enum has `image`; the desktop format builder has no such case
        // and errors out on it, so a literal passthrough would break the restored download.
        let dump = json!({
            "queue": [{ "itemId": "a", "url": "https://youtu.be/x", "quality": "image" }]
        });
        let restored = restored_queue_items(&dump);
        assert_eq!(restored[0]["params"]["quality"], "best");
    }

    #[test]
    fn passes_through_a_quality_the_desktop_understands() {
        let dump = json!({
            "queue": [{ "itemId": "a", "url": "https://youtu.be/x", "quality": "m4a" }]
        });
        assert_eq!(restored_queue_items(&dump)[0]["params"]["quality"], "m4a");
    }

    #[test]
    fn rejects_a_dump_with_no_queue_rather_than_emptying_the_list() {
        // The merge-vs-replace decision is enforced here: with no queue key the reader
        // contributes nothing, so the caller's existing parked items survive.
        assert!(restored_queue_items(&json!({ "prefs": {} })).is_empty());
    }

    #[test]
    fn skips_a_queue_entry_without_a_url() {
        let dump = json!({
            "queue": [
                { "itemId": "a", "url": "https://youtu.be/x", "quality": "best" },
                { "itemId": "b" },
                { "itemId": "c", "url": "" },
            ]
        });
        assert_eq!(restored_queue_items(&dump).len(), 1);
    }

    #[test]
    fn refuses_a_cookie_file_name_that_could_escape_the_directory() {
        let dump = json!({
            "cookieFiles": {
                "x.txt": "good",
                "../../evil.txt": "traversal",
                "sub/dir.txt": "traversal",
                "..\\windows.txt": "traversal"
            }
        });
        let files = restored_cookie_files(&dump);
        assert_eq!(files.len(), 1);
        assert_eq!(files.get("x.txt").map(String::as_str), Some("good"));
    }

    #[test]
    fn refuses_a_windows_drive_prefixed_cookie_file_name() {
        // `C:evil.txt` has no separator, so a `/ \ ..` denylist lets it through — and
        // PathBuf::push treats a path with a prefix but no root as a *replacement*, so it
        // resolves to the drive's current directory rather than a file beside the others.
        let dump = json!({ "cookieFiles": { "C:evil.txt": "escape" } });
        assert!(restored_cookie_files(&dump).is_empty());
    }

    #[test]
    fn refuses_a_cookie_file_name_that_is_not_a_txt_profile() {
        // "" and "." name the directory itself, and a stray file type would be picked up by
        // nothing.
        let dump = json!({
            "cookieFiles": { "": "x", ".": "x", "notes.md": "x", "x.txt": "good" }
        });
        let files = restored_cookie_files(&dump);
        assert_eq!(files.len(), 1);
        assert_eq!(files.get("x.txt").map(String::as_str), Some("good"));
    }

    #[test]
    fn accepts_a_dotted_profile_name() {
        // sanitize_cookie_name permits dots, so `my.site.txt` is a real profile.
        let dump = json!({ "cookieFiles": { "my.site.txt": "body" } });
        assert_eq!(restored_cookie_files(&dump).len(), 1);
    }

    #[test]
    fn reads_a_cookie_file_written_by_the_phone() {
        // The real shape of the attachment: a Netscape file whose name is `x.txt` even
        // though the platform key is `twitter`.
        let body = "# Netscape HTTP Cookie File\n.x.com\tTRUE\t/\tTRUE\t1801\tauth_token\tv\n";
        let dump = json!({ "cookieFiles": { "x.txt": body } });
        assert_eq!(restored_cookie_files(&dump)["x.txt"], body);
    }

    #[test]
    fn maps_android_pref_names_back_onto_the_desktop_store() {
        let dump = json!({
            "prefs": {
                "auto_update": true,
                "terms_accepted": true,
                "last_notified_version": "7.0.3",
                "browser_opt_in": false,
                "cookies_twitter": "auth_token=secret"
            }
        });
        let prefs = restored_prefs(&dump);
        assert_eq!(prefs["autoUpdate"], true);
        assert_eq!(prefs["lastNotifiedVersion"], "7.0.3");
        // Terms acceptance lives in localStorage here, and a browser opt-in and a WebView
        // cookie header describe things this app has no concept of. Importing them would
        // store values nothing ever reads.
        assert!(!prefs.contains_key("termsAccepted"));
        assert!(!prefs.contains_key("browser_opt_in"));
        assert!(!prefs.contains_key("cookies_twitter"));
    }

    #[test]
    fn survives_a_dump_that_is_missing_every_section() {
        // Android reads all three sections with optJSONObject/optJSONArray, so an absent one
        // is empty rather than an error, and the two readers must agree.
        let dump = json!({ "dumpVersion": 1, "appVersion": "7.1.0" });
        assert!(restored_prefs(&dump).is_empty());
        assert!(restored_cookie_files(&dump).is_empty());
        assert!(restored_queue_items(&dump).is_empty());
    }

    #[test]
    fn round_trips_a_dump_written_by_this_app() {
        let dir = temp_dir("roundtrip");
        write_cookie(&dir, "x.txt", "cookie body");
        let settings = json!({ "autoUpdate": true, "termsAccepted": true });
        let parked = vec![(
            "7".to_string(),
            json!({ "jobId": 7, "parkedAt": 99, "params": { "url": "https://vimeo.com/1", "quality": "720", "title": "V" } }),
        )];
        let (text, _) = build_dump("4.8.0", settings.as_object().unwrap(), &dir, &parked, &[]);
        let dump: Value = serde_json::from_str(&text).unwrap();

        assert_eq!(restored_cookie_files(&dump).len(), 1);
        assert_eq!(restored_queue_items(&dump).len(), 1);
        assert_eq!(restored_queue_items(&dump)[0]["params"]["url"], "https://vimeo.com/1");
        assert_eq!(restored_prefs(&dump)["autoUpdate"], true);
    }
}
