use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;
use crate::compat::{AppHandle, Emitter, Manager};

use crate::errors::{self, FailureKind};
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
    /// The raw yt-dlp line, kept for issue reports — the UI never shows it.
    pub message: String,
    pub friendly: String,
    pub kind: String,
}

/// Final job outcome. `failure_kind`/`friendly` are empty unless the job
/// failed (not cancelled) — they carry the plain-language verdict the UI
/// renders instead of a raw error dump.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DonePayload {
    pub job_id: u64,
    pub ok: bool,
    pub cancelled: bool,
    #[serde(default)]
    pub failure_kind: String,
    #[serde(default)]
    pub friendly: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PausedPayload {
    pub job_id: u64,
}

/// Transient status text ("Updating downloader…") shown in place of the
/// regular status line while a self-heal runs.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub job_id: u64,
    pub message: String,
}

impl DonePayload {
    pub fn ok(job_id: u64, ok: bool, cancelled: bool) -> Self {
        Self {
            job_id,
            ok,
            cancelled,
            failure_kind: String::new(),
            friendly: String::new(),
        }
    }

    pub fn cancelled(job_id: u64) -> Self {
        Self::ok(job_id, false, true)
    }

    pub fn failed(job_id: u64, worst: &Option<(FailureKind, String)>) -> Self {
        let (kind, friendly) = match worst {
            Some((k, f)) => (k.as_str().to_string(), f.clone()),
            None => (
                FailureKind::Other.as_str().to_string(),
                errors::classified("download failed", None).friendly,
            ),
        };
        Self {
            job_id,
            ok: false,
            cancelled: false,
            failure_kind: kind,
            friendly,
        }
    }
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
    // When set, yt-dlp downloads and merges here instead of next to the
    // destination; the finished file is moved into the destination after the
    // merge. Keeps all partial files in ONE directory.
    temp_dir: Option<&str>,
    // When set, prefixes the output template with "<subdir>/" so finished
    // files are sorted into per-source directories. Folder-safe.
    subdir: Option<&str>,
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
        // yt-dlp transfers one connection per fragment by default, which
        // leaves most lines nowhere near the line's actual capacity. 16
        // concurrent fragments is what saturates a connection on the hosts
        // that throttle per connection; a single-stream source ignores it.
        "--concurrent-fragments",
        "16",
        // A stalled connection must time out and retry instead of hanging a
        // job forever at 0 bytes/s.
        "--socket-timeout",
        "30",
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

    if let Some(temp) = temp_dir {
        args.push("-P".to_string());
        args.push(format!("temp:{temp}"));
    }
    let dir_prefix = subdir.map(|s| format!("{}/", s)).unwrap_or_default();

    if let Some(name) = out_name {
        // Resolved-stream job: one URL per yt-dlp run, named by the caller.
        args.extend(
            ["--no-playlist", "-o"]
                .iter()
                .map(|s| s.to_string())
                .chain(std::iter::once(format!("{dir_prefix}{name}.%(ext)s"))),
        );
    } else if playlist_mode {
        let template = format!("{dir_prefix}%(playlist_title)s/%(playlist_index)03d - %(title)s [%(id)s].%(ext)s");
        args.extend(
            [
                "--yes-playlist".to_string(),
                "--ignore-errors".to_string(),
                "-o".to_string(),
                template,
            ]
            .into_iter(),
        );
    } else {
        let template = format!("{dir_prefix}%(title)s [%(id)s].%(ext)s");
        args.extend(
            ["--no-playlist".to_string(), "-o".to_string(), template].into_iter(),
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

/// True when the main window exists and is visible.
///
/// Progress ticks are the app's highest-frequency events. While the window
/// sits hidden in the tray — or has been torn down entirely — there is
/// nothing to render them, so they are not emitted at all. Every other
/// pluck event is correctness-relevant and stays unconditional; the webview
/// rebuild reattaches to live jobs from its stores and the backend state.
fn window_listening(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false)
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

/// What one yt-dlp output line meant.
pub enum LineOutcome {
    /// Nothing the app tracks.
    Nothing,
    /// A progress tick. The item is the playlist item index, absent for a
    /// single video; the fraction is how far along that item is.
    Progress {
        item: Option<u64>,
        fraction: Option<f64>,
    },
    /// An error line, with the classified verdict.
    Failure(FailureKind, String),
    /// yt-dlp moved an output into place. Carries the final filepath, so the
    /// backend can record the download instead of relying on the frontend.
    Done(String),
}

/// Parse one yt-dlp output line for a normal pluck. Returns what the line
/// meant, so the caller can accumulate a job-level verdict and see whether
/// the download is still making progress.
pub fn handle_line(
    app: &AppHandle,
    job_id: u64,
    raw: &str,
    throttle: &mut Throttle,
) -> LineOutcome {
    let line = raw.trim_end_matches(['\r', '\n']).trim();

    if let Some(rest) = line.strip_prefix("PROG|") {
        let p: Vec<&str> = rest.split('|').collect();
        if p.len() < 6 {
            return LineOutcome::Nothing;
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
            return LineOutcome::Nothing;
        }
        if window_listening(app) {
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
        }
        return LineOutcome::Progress {
            item: index(p[0]),
            fraction: percent.map(|pc| pc / 100.0),
        };
    } else if let Some(rest) = line.strip_prefix("ITEM|") {
        let p: Vec<&str> = rest.splitn(3, '|').collect();
        if p.len() < 3 {
            return LineOutcome::Nothing;
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
        return LineOutcome::Nothing;
    } else if let Some(rest) = line.strip_prefix("DONE|") {
        let p: Vec<&str> = rest.splitn(2, '|').collect();
        if p.len() < 2 {
            return LineOutcome::Nothing;
        }
        let _ = app.emit(
            "pluck://item-done",
            ItemDonePayload {
                job_id,
                item_index: index(p[0]).unwrap_or(1),
                filepath: p[1].to_string(),
            },
        );
        return LineOutcome::Done(p[1].to_string());
    } else if line.starts_with("ERROR") {
        let failure = errors::classified(line, None);
        let _ = app.emit(
            "pluck://error",
            ErrorPayload {
                job_id,
                message: line.to_string(),
                friendly: failure.friendly.clone(),
                kind: failure.kind.to_string(),
            },
        );
        return LineOutcome::Failure(
            errors::kind_of(failure.kind),
            failure.friendly,
        );
    }
    LineOutcome::Nothing
}

/// Line handler for resolved-stream plucks. Unlike [`handle_line`], the item
/// index comes from the batch ordinal we pass in (each episode is its own
/// single-video yt-dlp run, so yt-dlp's own playlist index is always NA).
/// yt-dlp's `ITEM|` lines are ignored — the caller emits `item-start` itself
/// with the real episode title. Returns `(output path on DONE|line,
/// classified failure on ERROR|line)` so the caller can track both the last
/// file for "Open folder" and an episode-level verdict.
pub fn handle_stream_line(
    app: &AppHandle,
    job_id: u64,
    item_index: u64,
    raw: &str,
    throttle: &mut Throttle,
) -> (Option<String>, Option<(FailureKind, String)>) {
    let line = raw.trim_end_matches(['\r', '\n']).trim();

    if let Some(rest) = line.strip_prefix("PROG|") {
        let p: Vec<&str> = rest.split('|').collect();
        if p.len() < 6 {
            return (None, None);
        }
        let downloaded = num(p[1]);
        let total = num(p[2]).or_else(|| num(p[3]));
        let percent = match (downloaded, total) {
            (Some(d), Some(t)) if t > 0.0 => Some((d / t * 100.0).min(100.0)),
            _ => None,
        };
        let finished = percent.map(|pc| pc >= 100.0).unwrap_or(false);
        if !finished && !throttle.ready() {
            return (None, None);
        }
        if window_listening(app) {
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
        }
    } else if let Some(rest) = line.strip_prefix("DONE|") {
        let p: Vec<&str> = rest.splitn(2, '|').collect();
        if p.len() < 2 {
            return (None, None);
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
        return (Some(filepath), None);
    } else if line.starts_with("ERROR") {
        let failure = errors::classified(line, None);
        let _ = app.emit(
            "pluck://error",
            ErrorPayload {
                job_id,
                message: line.to_string(),
                friendly: failure.friendly.clone(),
                kind: failure.kind.to_string(),
            },
        );
        return (None, Some((errors::kind_of(failure.kind), failure.friendly)));
    }
    (None, None)
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
        // No process-group control from tauri-plugin-shell, so collect the
        // whole descendant tree first (children disappear when their parent
        // dies and would be re-parented to init), then SIGKILL bottom-up.
        fn descendants(pid: u32) -> Vec<u32> {
            let mut out = vec![pid];
            let mut i = 0;
            while i < out.len() {
                let parent = out[i];
                if let Ok(output) = std::process::Command::new("pgrep")
                    .arg("-P")
                    .arg(parent.to_string())
                    .output()
                {
                    for line in String::from_utf8_lossy(&output.stdout).lines() {
                        if let Ok(child) = line.trim().parse::<u32>() {
                            if child != pid && !out.contains(&child) {
                                out.push(child);
                            }
                        }
                    }
                }
                i += 1;
            }
            out
        }
        let mut victims = descendants(pid);
        victims.reverse(); // children first, parent last
        for victim in victims {
            let _ = std::process::Command::new("kill")
                .args(["-9", &victim.to_string()])
                .status();
        }
    }
}
