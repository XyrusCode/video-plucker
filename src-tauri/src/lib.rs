mod commands;
mod extractors;
mod pluck;
mod search_commands;
mod sidecar;
mod tray;

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use url::Url;

pub struct PluckJob {
    pub pid: u32,
    pub cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct PluckState(pub Mutex<HashMap<u64, PluckJob>>);

#[derive(Clone, Serialize)]
struct DeepLinkPayload {
    action: String,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    quality: Option<String>,
}

/// Parse argv for yt-plucker:// deep-link URLs and emit a `deep-link-received`
/// event so the frontend can act on them.
fn handle_deep_link_argv(app: &AppHandle, argv: &[String]) {
    for arg in argv {
        let trimmed = arg.trim();
        if !trimmed.starts_with("yt-plucker://") {
            continue;
        }

        let parsed = match Url::parse(trimmed) {
            Ok(u) => u,
            Err(_) => continue,
        };

        // /analyze?url=...  or  /pluck?url=...&quality=...
        let path = parsed.path().trim_start_matches('/');
        if path.is_empty() {
            continue;
        }

        let mut payload = DeepLinkPayload {
            action: path.to_string(),
            url: String::new(),
            quality: None,
        };

        for (key, value) in parsed.query_pairs() {
            match key.as_ref() {
                "url" => payload.url = value.into_owned(),
                "quality" => payload.quality = Some(value.into_owned()),
                _ => {}
            }
        }

        if payload.url.is_empty() {
            continue;
        }

        show_main_window(app);

        if let Err(e) = app.emit("deep-link-received", payload) {
            eprintln!("Failed to emit deep-link-received event: {e}");
        }
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn kill_all_jobs(app: &AppHandle) {
    if let Some(state) = app.try_state::<PluckState>() {
        let jobs = state.0.lock().unwrap();
        for job in jobs.values() {
            job.cancelled
                .store(true, std::sync::atomic::Ordering::SeqCst);
            pluck::kill_tree(job.pid);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            show_main_window(app);
            handle_deep_link_argv(app, argv);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .manage(PluckState::default())
        .setup(|app| {
            tray::create_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // X button hides to tray; downloads keep running
            WindowEvent::CloseRequested { api, .. } => {
                let _ = window.hide();
                api.prevent_close();
            }
            // minimize also goes to tray
            WindowEvent::Resized(_) => {
                if window.is_minimized().unwrap_or(false) {
                    let _ = window.hide();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::fetch_metadata,
            commands::start_pluck,
            commands::cancel_pluck,
            commands::import_platform_cookies,
            commands::clear_platform_cookies,
            commands::get_platform_cookies_status,
            search_commands::list_sites,
            search_commands::search_content,
            search_commands::get_series_detail,
            search_commands::resolve_streams,
            search_commands::start_stream_pluck
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                kill_all_jobs(app_handle);
            }
        });
}
