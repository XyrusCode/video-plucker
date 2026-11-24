//! Local settings and store replacements.
//!
//! Tauri's `store` plugin (settings.json / plucks.json / queue.json) is gone.
//! These are plain JSON files in the same app-data directory, with the same
//! camelCase keys, so dumps exported by 5.x still round-trip.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub dest_dir: String,
    pub quality: String,
    pub cookies_browser: String,
    /// "queue" | "parallel"
    pub download_mode: String,
    pub max_concurrent_downloads: u32,
    pub auto_update: bool,
    pub next_job_id: u64,
    /// "system" | "light" | "dark" (was localStorage `vp.themeMode`)
    pub theme_mode: String,
    /// was localStorage `video-plucker-terms-accepted`
    pub terms_accepted: bool,
    pub last_notified_version: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dest_dir: downloads_dir(),
            quality: "480".into(),
            cookies_browser: "none".into(),
            download_mode: "queue".into(),
            max_concurrent_downloads: 3,
            auto_update: true,
            next_job_id: 1,
            theme_mode: "system".into(),
            terms_accepted: false,
            last_notified_version: String::new(),
        }
    }
}

fn downloads_dir() -> String {
    #[cfg(windows)]
    {
        if let Some(root) = std::env::var_os(concat!("USER", "PROFILE")) {
            return std::path::Path::new(&root).join("Downloads").to_string_lossy().into_owned();
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::Path::new(&home).join("Downloads").to_string_lossy().into_owned();
        }
    }
    std::env::temp_dir().to_string_lossy().into_owned()
}

fn path(name: &str) -> PathBuf {
    crate::compat::backend().data_dir.join(name)
}

/// Read a JSON array store. Missing or corrupt files are empty stores.
pub fn load_records(name: &str) -> Vec<serde_json::Value> {
    let Ok(text) = std::fs::read_to_string(path(name)) else {
        return Vec::new();
    };
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(serde_json::Value::Array(items)) => items,
        // Tauri's store plugin wrote objects keyed by record id; 6.x writes arrays.
        Ok(serde_json::Value::Object(map)) => map.into_values().collect(),
        _ => Vec::new(),
    }
}

pub fn save_records(name: &str, records: &[serde_json::Value]) -> Result<(), String> {
    let target = path(name);
    std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(records).map_err(|e| e.to_string())?;
    let tmp = target.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &target).map_err(|e| e.to_string())
}

pub fn load() -> Settings {
    std::fs::read_to_string(path("settings.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .unwrap_or_default()
}

/// Dumps store a few settings under Android's snake_case names. Accept both
/// those and this app's camelCase names when importing a backup.
pub fn from_dump_prefs(value: &Value) -> Option<Settings> {
    let mut v = value.clone();
    if let Some(map) = v.as_object_mut() {
        for (dump_key, app_key) in [
            ("auto_update", "autoUpdate"),
            ("last_notified_version", "lastNotifiedVersion"),
            ("dest_dir", "destDir"),
            ("cookies_browser", "cookiesBrowser"),
            ("download_mode", "downloadMode"),
            ("max_concurrent_downloads", "maxConcurrentDownloads"),
            ("next_job_id", "nextJobId"),
        ] {
            if let Some( val) = map.remove(dump_key) {
                map.entry(app_key).or_insert(val);
            }
        }
    }
    serde_json::from_value(v).ok()
}

pub fn save(s: &Settings) -> Result<(), String> {
    let target = path("settings.json");
    std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    let tmp = target.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &target).map_err(|e| e.to_string())
}
