//! Tauri commands for the streaming-site search feature.
//!
//! Search/detail/resolve are thin wrappers over the [`extractors`] registry.
//! `start_stream_pluck` owns the resolve-then-download loop: it resolves each
//! chosen episode to a fresh stream URL (tokens expire, so this must happen at
//! download time — never from a persisted URL) and drives a single batch job
//! through the same `pluck://` event contract the playlist UI already renders.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

use crate::extractors::{self, EpisodeRef, SearchOpts, SearchResult, SeriesDetail, SiteInfo, StreamOption};
use crate::pluck::{self, DonePayload, ErrorPayload, ItemStartPayload, Throttle};
use crate::{PluckJob, PluckState};

/// One episode the user selected for download. `episode` is the site's own
/// label; `episode_id` is its unique key (e.g. page URL) when the label alone
/// can't locate it; `title` is what the per-item row displays.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeSel {
    pub episode: String,
    pub episode_id: Option<String>,
    pub title: Option<String>,
}

#[tauri::command]
pub async fn list_sites() -> Vec<SiteInfo> {
    extractors::searchable_sites()
}

#[tauri::command]
pub async fn search_content(
    site: String,
    query: String,
    translation: String,
) -> Result<Vec<SearchResult>, String> {
    let ex = extractors::get(&site).ok_or_else(|| format!("unknown site: {site}"))?;
    ex.search(query.trim(), &SearchOpts { translation })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_series_detail(
    site: String,
    id: String,
    translation: String,
) -> Result<SeriesDetail, String> {
    let ex = extractors::get(&site).ok_or_else(|| format!("unknown site: {site}"))?;
    ex.detail(&id, &SearchOpts { translation })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resolve_streams(
    site: String,
    show_id: String,
    episode: String,
    translation: String,
) -> Result<Vec<StreamOption>, String> {
    let ex = extractors::get(&site).ok_or_else(|| format!("unknown site: {site}"))?;
    ex.resolve_streams(&EpisodeRef {
        site,
        show_id,
        episode_id: episode.clone(),
        episode,
        translation,
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_stream_pluck(
    app: AppHandle,
    job_id: u64,
    site: String,
    show_id: String,
    title: String,
    episodes: Vec<EpisodeSel>,
    translation: String,
    quality: String,
    dest_dir: String,
) -> Result<(), String> {
    // Register the job up front (pid 0 until the first episode spawns) so
    // cancel_pluck can flip the flag even during the initial resolve.
    let cancelled = Arc::new(AtomicBool::new(false));
    app.state::<PluckState>().0.lock().unwrap().insert(
        job_id,
        PluckJob {
            pid: 0,
            cancelled: cancelled.clone(),
        },
    );

    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        run_stream_batch(
            app2,
            job_id,
            site,
            show_id,
            title,
            episodes,
            translation,
            quality,
            dest_dir,
            cancelled,
        )
        .await;
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_stream_batch(
    app: AppHandle,
    job_id: u64,
    site: String,
    show_id: String,
    title: String,
    episodes: Vec<EpisodeSel>,
    translation: String,
    quality: String,
    dest_dir: String,
    cancelled: Arc<AtomicBool>,
) {
    let total = episodes.len() as u64;
    let finish = |ok: bool, was_cancelled: bool| {
        let _ = app.emit(
            "pluck://done",
            DonePayload {
                job_id,
                ok,
                cancelled: was_cancelled,
            },
        );
        app.state::<PluckState>().0.lock().unwrap().remove(&job_id);
    };

    let ex = match extractors::get(&site) {
        Some(e) => e,
        None => {
            let _ = app.emit(
                "pluck://error",
                ErrorPayload {
                    job_id,
                    message: format!("unknown site: {site}"),
                },
            );
            finish(false, false);
            return;
        }
    };

    let archive = match pluck::archive_path(&app, job_id) {
        Ok(a) => a.to_string_lossy().into_owned(),
        Err(e) => {
            let _ = app.emit("pluck://error", ErrorPayload { job_id, message: e });
            finish(false, false);
            return;
        }
    };

    let mut ok_all = true;
    let mut any_done = false;

    for (i, ep) in episodes.iter().enumerate() {
        if cancelled.load(Ordering::SeqCst) {
            break;
        }
        let ordinal = i as u64 + 1;
        let ep_title = ep
            .title
            .clone()
            .unwrap_or_else(|| format!("Episode {}", ep.episode));

        let _ = app.emit(
            "pluck://item-start",
            ItemStartPayload {
                job_id,
                item_index: ordinal,
                item_count: total,
                title: ep_title.clone(),
            },
        );

        // Fresh resolve every time — stream tokens are short-lived.
        let eref = EpisodeRef {
            site: site.clone(),
            show_id: show_id.clone(),
            episode: ep.episode.clone(),
            episode_id: ep
                .episode_id
                .clone()
                .unwrap_or_else(|| ep.episode.clone()),
            translation: translation.clone(),
        };
        let opts = match ex.resolve_streams(&eref).await {
            Ok(o) => o,
            Err(e) => {
                emit_ep_error(&app, job_id, &ep.episode, &e.to_string());
                ok_all = false;
                continue;
            }
        };
        let (stream, ytdlp_quality) = match pick_stream(&opts, &quality) {
            Some(x) => x,
            None => {
                emit_ep_error(&app, job_id, &ep.episode, "no matching stream quality");
                ok_all = false;
                continue;
            }
        };

        let out_name = stream_out_name(&title, &ep.episode, total);
        let args = match pluck::build_args(
            &stream.url,
            &ytdlp_quality,
            &dest_dir,
            false,
            &archive,
            stream.referer.as_deref(),
            &stream.headers,
            Some(out_name.as_str()),
            None,
        ) {
            Ok(a) => a,
            Err(e) => {
                emit_ep_error(&app, job_id, &ep.episode, &e);
                ok_all = false;
                continue;
            }
        };

        let spawn = app
            .shell()
            .sidecar("yt-dlp")
            .and_then(|c| c.env("PYTHONIOENCODING", "utf-8").args(args).spawn());
        let (mut rx, child) = match spawn {
            Ok(v) => v,
            Err(e) => {
                emit_ep_error(&app, job_id, &ep.episode, &e.to_string());
                ok_all = false;
                continue;
            }
        };
        // Point cancellation at the currently-running episode.
        if let Some(j) = app.state::<PluckState>().0.lock().unwrap().get_mut(&job_id) {
            j.pid = child.pid();
        }

        let mut throttle = Throttle::new(Duration::from_millis(150));
        let mut ep_ok = false;
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(bytes) | CommandEvent::Stderr(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    for line in text.lines() {
                        pluck::handle_stream_line(&app, job_id, ordinal, line, &mut throttle);
                    }
                }
                CommandEvent::Terminated(payload) => {
                    ep_ok = payload.code == Some(0);
                }
                _ => {}
            }
        }

        if cancelled.load(Ordering::SeqCst) {
            break;
        }
        if ep_ok {
            any_done = true;
        } else {
            ok_all = false;
        }
    }

    let was_cancelled = cancelled.load(Ordering::SeqCst);
    // Re-resolution is required on any resume, so the archive is disposable.
    let _ = std::fs::remove_file(&archive);
    finish(ok_all && any_done && !was_cancelled, was_cancelled);
}

fn emit_ep_error(app: &AppHandle, job_id: u64, episode: &str, msg: &str) {
    let _ = app.emit(
        "pluck://error",
        ErrorPayload {
            job_id,
            message: format!("Episode {episode}: {msg}"),
        },
    );
}

/// Choose the stream variant for the requested quality and the format tier to
/// hand yt-dlp. Discrete variants (fixed-resolution URLs) are matched in Rust
/// and downloaded as-is; a master playlist keeps the requested tier so yt-dlp's
/// own height filter picks the variant.
fn pick_stream(opts: &[StreamOption], quality: &str) -> Option<(StreamOption, String)> {
    if opts.is_empty() {
        return None;
    }
    let target: Option<u32> = match quality {
        "2160" => Some(2160),
        "1440" => Some(1440),
        "1080" => Some(1080),
        "720" => Some(720),
        "480" => Some(480),
        _ => None, // best / mp3 / m4a
    };

    let discretes: Vec<&StreamOption> = opts.iter().filter(|o| o.height.is_some()).collect();
    if !discretes.is_empty() {
        let chosen = match target {
            Some(t) => discretes
                .iter()
                .filter(|o| o.height.unwrap() <= t)
                .max_by_key(|o| o.height.unwrap())
                .or_else(|| discretes.iter().min_by_key(|o| o.height.unwrap())),
            None => discretes.iter().max_by_key(|o| o.height.unwrap()),
        }?;
        // A single fixed-resolution stream: just take it, unless the user asked
        // for audio-only extraction (which yt-dlp still applies to the stream).
        let q = match quality {
            "mp3" | "m4a" => quality.to_string(),
            _ => "best".to_string(),
        };
        return Some(((*chosen).clone(), q));
    }

    // Only master playlist(s): let yt-dlp filter variants with the chosen tier.
    Some((opts[0].clone(), quality.to_string()))
}

/// Build the output filename (yt-dlp appends `.%(ext)s`). Movies get the bare
/// title; series episodes get `Title - E<ep>`.
fn stream_out_name(title: &str, episode: &str, total: u64) -> String {
    let base = sanitize(title);
    if total > 1 {
        format!("{base} - E{}", episode)
    } else {
        base
    }
}

/// Strip characters that are illegal in filenames or would spawn subdirectories.
fn sanitize(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => ' ',
            c if (c as u32) < 0x20 => ' ',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').trim();
    if trimmed.is_empty() {
        "video".to_string()
    } else {
        trimmed.to_string()
    }
}
