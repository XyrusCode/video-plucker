use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

use crate::download::{self, DonePayload, Throttle};
use crate::{DownloadJob, DownloadState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    kind: String, // "video" | "playlist"
    title: String,
    thumbnail: Option<String>,
    duration: Option<f64>,
    heights: Vec<u32>,
    entry_count: Option<u64>,
    entries: Vec<String>,
}

fn last_thumbnail(v: &Value) -> Option<String> {
    v.get("thumbnails")?
        .as_array()?
        .last()?
        .get("url")?
        .as_str()
        .map(String::from)
}

#[tauri::command]
pub async fn fetch_metadata(
    app: AppHandle,
    url: String,
    playlist_mode: bool,
) -> Result<Metadata, String> {
    let mut args: Vec<String> = vec!["-J".into(), "--no-warnings".into()];
    if playlist_mode {
        // full -J on a large playlist takes minutes; flat is instant
        args.extend(["--flat-playlist".into(), "--yes-playlist".into()]);
    } else {
        args.push("--no-playlist".into());
    }
    args.push(url);

    let output = app
        .shell()
        .sidecar("yt-dlp")
        .map_err(|e| e.to_string())?
        .env("PYTHONIOENCODING", "utf-8")
        .args(args)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = stderr
            .lines()
            .find(|l| l.contains("ERROR"))
            .unwrap_or("yt-dlp could not read this URL")
            .to_string();
        return Err(msg);
    }

    let v: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("unexpected yt-dlp output: {e}"))?;

    if v.get("_type").and_then(Value::as_str) == Some("playlist") {
        let entries = v
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let titles = entries
            .iter()
            .take(50)
            .map(|e| {
                e.get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("(untitled)")
                    .to_string()
            })
            .collect();
        let count = v
            .get("playlist_count")
            .and_then(Value::as_u64)
            .unwrap_or(entries.len() as u64);
        let thumbnail =
            last_thumbnail(&v).or_else(|| entries.first().and_then(last_thumbnail));
        Ok(Metadata {
            kind: "playlist".into(),
            title: v
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Playlist")
                .into(),
            thumbnail,
            duration: None,
            heights: vec![],
            entry_count: Some(count),
            entries: titles,
        })
    } else {
        let mut heights: Vec<u32> = v
            .get("formats")
            .and_then(Value::as_array)
            .map(|fs| {
                fs.iter()
                    .filter_map(|f| f.get("height").and_then(Value::as_u64))
                    .map(|h| h as u32)
                    .collect()
            })
            .unwrap_or_default();
        heights.sort_unstable();
        heights.dedup();
        heights.reverse();
        Ok(Metadata {
            kind: "video".into(),
            title: v
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("(untitled)")
                .into(),
            thumbnail: v
                .get("thumbnail")
                .and_then(Value::as_str)
                .map(String::from)
                .or_else(|| last_thumbnail(&v)),
            duration: v.get("duration").and_then(Value::as_f64),
            heights,
            entry_count: None,
            entries: vec![],
        })
    }
}

#[tauri::command]
pub async fn start_download(
    app: AppHandle,
    state: State<'_, DownloadState>,
    job_id: u64,
    url: String,
    quality: String,
    dest_dir: String,
    playlist_mode: bool,
) -> Result<(), String> {
    let args = download::build_args(&url, &quality, &dest_dir, playlist_mode)?;

    let (mut rx, child) = app
        .shell()
        .sidecar("yt-dlp")
        .map_err(|e| e.to_string())?
        .env("PYTHONIOENCODING", "utf-8")
        .args(args)
        .spawn()
        .map_err(|e| e.to_string())?;

    let cancelled = Arc::new(AtomicBool::new(false));
    state.0.lock().unwrap().insert(
        job_id,
        DownloadJob {
            pid: child.pid(),
            cancelled: cancelled.clone(),
        },
    );

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut throttle = Throttle::new(Duration::from_millis(150));
        while let Some(event) = rx.recv().await {
            match event {
                // progress arrives on stderr in quiet mode (--print enables it),
                // so both streams are parsed identically
                CommandEvent::Stdout(bytes) | CommandEvent::Stderr(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    for line in text.lines() {
                        download::handle_line(&app_handle, job_id, line, &mut throttle);
                    }
                }
                CommandEvent::Terminated(payload) => {
                    let was_cancelled = cancelled.load(Ordering::SeqCst);
                    let _ = app_handle.emit(
                        "download://done",
                        DonePayload {
                            job_id,
                            ok: payload.code == Some(0) && !was_cancelled,
                            cancelled: was_cancelled,
                        },
                    );
                    app_handle
                        .state::<DownloadState>()
                        .0
                        .lock()
                        .unwrap()
                        .remove(&job_id);
                }
                _ => {}
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn cancel_download(state: State<'_, DownloadState>, job_id: u64) -> Result<(), String> {
    let jobs = state.0.lock().unwrap();
    let job = jobs.get(&job_id).ok_or("download not found")?;
    job.cancelled.store(true, Ordering::SeqCst);
    download::kill_tree(job.pid);
    Ok(())
}
