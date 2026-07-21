use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::sidecar;

/// Directory where platform-specific cookie files are stored.
pub fn app_cookies_dir(app: &AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("yt-plucker"))
        .join("cookies");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Per-pluck yt-dlp download archive, kept in the app data dir and keyed by
/// job id. yt-dlp records finished items here so a resumed pluck skips them.
pub fn archive_path(app: &AppHandle, job_id: u64) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("archives");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join(format!("pluck-{job_id}.txt")))
}

/// Machine-readable progress line. `NA` appears for unknown fields.
const PROGRESS_TEMPLATE: &str = "PROG|%(info.playlist_index)s|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPayload {
    pub job_id: u64,
    pub item_index: Option<u64>,
    pub downloaded_bytes: Option<f64>,
    pub total_bytes: Option<f64>,
    pub percent: Option<f64>,
    pub speed: Option<f64>,
    pub eta: Option<f64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemStartPayload {
    pub job_id: u64,
    pub item_index: u64,
    pub item_count: u64,
    pub title: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDonePayload {
    pub job_id: u64,
    pub item_index: u64,
    pub filepath: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPayload {
    pub job_id: u64,
    pub message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DonePayload {
    pub job_id: u64,
    pub ok: bool,
    pub cancelled: bool,
}

pub fn build_args(
    url: &str,
    quality: &str,
    dest_dir: &str,
    playlist_mode: bool,
    archive: &str,
    referer: Option<&str>,
    headers: &[(String, String)],
    // When set, forces the output filename (used for resolved streams, whose
    // m3u8 URLs carry no usable title/id metadata for the default template).
    out_name: Option<&str>,
    // When set, reads YouTube (etc.) login cookies from this browser to get
    // past "Sign in to confirm you're not a bot". e.g. "chrome", "firefox".
    cookies_from_browser: Option<&str>,
    // Path to a Netscape-format cookies.txt file for platforms that require
    // login (Twitter/X, YouTube, TikTok).
    cookies_file: Option<&str>,
) -> Result<Vec<String>, String> {
    let ffmpeg = sidecar::ffmpeg_path()?;

    let mut args: Vec<String> = [
        url,
        "-P",
        dest_dir,
        "--ffmpeg-location",
        &ffmpeg.to_string_lossy(),
        "--windows-filenames",
        "--trim-filenames",
        "180",
        // resume support: keep partial files and skip already-finished items
        "--continue",
        "--download-archive",
        archive,
        "--retries",
        "10",
        "--fragment-retries",
        "10",
        "--concurrent-fragments",
        "4",
        "--no-mtime",
        "--no-warnings",
        "--newline",
        "--progress",
        "--progress-template",
        PROGRESS_TEMPLATE,
        // --print implies --simulate, so --no-simulate is mandatory
        "--no-simulate",
        "--print",
        "before_dl:ITEM|%(playlist_index)s|%(playlist_count)s|%(title)s",
        "--print",
        "after_move:DONE|%(playlist_index)s|%(filepath)s",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    if let Some(name) = out_name {
        // Resolved-stream job: one URL per yt-dlp run, named by the caller.
        args.extend(
            ["--no-playlist", "-o"]
                .iter()
                .map(|s| s.to_string())
                .chain(std::iter::once(format!("{name}.%(ext)s"))),
        );
    } else if playlist_mode {
        args.extend(
            [
                "--yes-playlist",
                "--ignore-errors",
                "-o",
                "%(playlist_title)s/%(playlist_index)03d - %(title)s [%(id)s].%(ext)s",
            ]
            .iter()
            .map(|s| s.to_string()),
        );
    } else {
        args.extend(
            ["--no-playlist", "-o", "%(title)s [%(id)s].%(ext)s"]
                .iter()
                .map(|s| s.to_string()),
        );
    }

    // The trailing `/b` is a fallback to the best available format when no
    // stream sits at or below the requested height (e.g. an X clip whose
    // lowest resolution is already above the cap), so a pluck never hard-fails.
    let quality_args: Vec<&str> = match quality {
        "best" => vec!["-f", "bv*+ba/b", "--merge-output-format", "mp4"],
        "2160" => vec!["-f", "bv*[height<=2160]+ba/b[height<=2160]/b", "--merge-output-format", "mp4"],
        "1440" => vec!["-f", "bv*[height<=1440]+ba/b[height<=1440]/b", "--merge-output-format", "mp4"],
        "1080" => vec!["-f", "bv*[height<=1080]+ba/b[height<=1080]/b", "--merge-output-format", "mp4"],
        "720" => vec!["-f", "bv*[height<=720]+ba/b[height<=720]/b", "--merge-output-format", "mp4"],
        "480" => vec!["-f", "bv*[height<=480]+ba/b[height<=480]/b", "--merge-output-format", "mp4"],
        "mp3" => vec!["-f", "ba/b", "-x", "--audio-format", "mp3", "--audio-quality", "0"],
        "m4a" => vec!["-f", "ba[ext=m4a]/ba/b", "-x", "--audio-format", "m4a"],
        other => return Err(format!("unknown quality option: {other}")),
    };
    args.extend(quality_args.iter().map(|s| s.to_string()));

    // Streaming-site hosts gate their CDN on Referer/Origin and expect the same
    // User-Agent that resolved the token; without these the m3u8 fetch 403s.
    // YouTube/X plucks pass none of these and are unaffected.
    if let Some(r) = referer {
        args.push("--referer".into());
        args.push(r.into());
    }
    for (k, v) in headers {
        args.push("--add-header".into());
        args.push(format!("{k}: {v}"));
    }

    if let Some(browser) = cookies_from_browser {
        if !browser.is_empty() && browser != "none" {
            args.push("--cookies-from-browser".into());
            args.push(browser.into());
        }
    }

    // Platform-specific cookies.txt file (imported by the user).
    if let Some(cf) = cookies_file {
        if !cf.is_empty() {
            args.push("--cookies".into());
            args.push(cf.into());
        }
    }

    Ok(args)
}

/// Rate limiter for progress events so the webview isn't flooded
/// (yt-dlp emits many progress lines per second per fragment).
pub struct Throttle {
    last: Instant,
    interval: Duration,
}

impl Throttle {
    pub fn new(interval: Duration) -> Self {
        Self {
            last: Instant::now() - interval,
            interval,
        }
    }

    fn ready(&mut self) -> bool {
        if self.last.elapsed() >= self.interval {
            self.last = Instant::now();
            true
        } else {
            false
        }
    }
}

/// yt-dlp prints the literal string "NA" (or "None") for unknown fields.
fn num(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() || t == "NA" || t == "None" {
        None
    } else {
        t.parse().ok()
    }
}

fn index(s: &str) -> Option<u64> {
    num(s).map(|f| f as u64)
}

pub fn handle_line(app: &AppHandle, job_id: u64, raw: &str, throttle: &mut Throttle) {
    let line = raw.trim_end_matches(['\r', '\n']).trim();

    if let Some(rest) = line.strip_prefix("PROG|") {
        let p: Vec<&str> = rest.split('|').collect();
        if p.len() < 6 {
            return;
        }
        let downloaded = num(p[1]);
        // total_bytes is often NA; fall back to the estimate
        let total = num(p[2]).or_else(|| num(p[3]));
        let percent = match (downloaded, total) {
            (Some(d), Some(t)) if t > 0.0 => Some((d / t * 100.0).min(100.0)),
            _ => None,
        };
        let finished = percent.map(|pc| pc >= 100.0).unwrap_or(false);
        if !finished && !throttle.ready() {
            return;
        }
        let _ = app.emit(
            "pluck://progress",
            ProgressPayload {
                job_id,
                item_index: index(p[0]),
                downloaded_bytes: downloaded,
                total_bytes: total,
                percent,
                speed: num(p[4]),
                eta: num(p[5]),
            },
        );
    } else if let Some(rest) = line.strip_prefix("ITEM|") {
        let p: Vec<&str> = rest.splitn(3, '|').collect();
        if p.len() < 3 {
            return;
        }
        let _ = app.emit(
            "pluck://item-start",
            ItemStartPayload {
                job_id,
                // single videos report NA for index/count -> treat as 1/1
                item_index: index(p[0]).unwrap_or(1),
                item_count: index(p[1]).unwrap_or(1),
                title: p[2].to_string(),
            },
        );
    } else if let Some(rest) = line.strip_prefix("DONE|") {
        let p: Vec<&str> = rest.splitn(2, '|').collect();
        if p.len() < 2 {
            return;
        }
        let _ = app.emit(
            "pluck://item-done",
            ItemDonePayload {
                job_id,
                item_index: index(p[0]).unwrap_or(1),
                filepath: p[1].to_string(),
            },
        );
    } else if line.starts_with("ERROR") {
        let _ = app.emit(
            "pluck://error",
            ErrorPayload {
                job_id,
                message: line.to_string(),
            },
        );
    }
}

/// Line handler for resolved-stream plucks. Unlike [`handle_line`], the item
/// index comes from the batch ordinal we pass in (each episode is its own
/// single-video yt-dlp run, so yt-dlp's own playlist index is always NA).
/// yt-dlp's `ITEM|` lines are ignored — the caller emits `item-start` itself
/// with the real episode title. Returns the output path when a `DONE|` line is
/// seen, so the caller can remember the last file for "Open folder".
pub fn handle_stream_line(
    app: &AppHandle,
    job_id: u64,
    item_index: u64,
    raw: &str,
    throttle: &mut Throttle,
) -> Option<String> {
    let line = raw.trim_end_matches(['\r', '\n']).trim();

    if let Some(rest) = line.strip_prefix("PROG|") {
        let p: Vec<&str> = rest.split('|').collect();
        if p.len() < 6 {
            return None;
        }
        let downloaded = num(p[1]);
        let total = num(p[2]).or_else(|| num(p[3]));
        let percent = match (downloaded, total) {
            (Some(d), Some(t)) if t > 0.0 => Some((d / t * 100.0).min(100.0)),
            _ => None,
        };
        let finished = percent.map(|pc| pc >= 100.0).unwrap_or(false);
        if !finished && !throttle.ready() {
            return None;
        }
        let _ = app.emit(
            "pluck://progress",
            ProgressPayload {
                job_id,
                item_index: Some(item_index),
                downloaded_bytes: downloaded,
                total_bytes: total,
                percent,
                speed: num(p[4]),
                eta: num(p[5]),
            },
        );
    } else if let Some(rest) = line.strip_prefix("DONE|") {
        let p: Vec<&str> = rest.splitn(2, '|').collect();
        if p.len() < 2 {
            return None;
        }
        let filepath = p[1].to_string();
        let _ = app.emit(
            "pluck://item-done",
            ItemDonePayload {
                job_id,
                item_index,
                filepath: filepath.clone(),
            },
        );
        return Some(filepath);
    } else if line.starts_with("ERROR") {
        let _ = app.emit(
            "pluck://error",
            ErrorPayload {
                job_id,
                message: line.to_string(),
            },
        );
    }
    None
}

/// Kill yt-dlp AND its child ffmpeg: CommandChild::kill() only terminates
/// yt-dlp itself, leaving ffmpeg orphaned with the output file locked.
pub fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status();
    }
}
