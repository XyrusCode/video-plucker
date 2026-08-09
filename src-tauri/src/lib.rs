mod commands;
mod extractors;
mod pluck;
mod search_commands;
mod sidecar;
mod tray;

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use url::Url;

pub struct PluckJob {
    pub pid: u32,
    pub cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct PluckState(pub Mutex<HashMap<u64, PluckJob>>);

#[derive(Clone, Serialize, Deserialize)]
struct DeepLinkPayload {
    action: String,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    quality: Option<String>,
}

/// Stores a deep-link that arrived before the frontend was ready to listen.
/// The frontend calls `consume_deep_link` on startup to pick it up.
#[derive(Default)]
struct PendingDeepLink(Mutex<Option<DeepLinkPayload>>);

/// Parse argv for yt-plucker:// deep-link URLs and emit a `deep-link-received`
/// event so the frontend can act on them.  If the frontend hasn't registered a
/// listener yet (startup race), the payload is stored in PendingDeepLink state.
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

        // Protocol URLs are yt-plucker://analyze?url=...&quality=... (action
        // in the authority) or yt-plucker:///analyze?... (action in the path);
        // accept both. The frontend ignores actions it doesn't know.
        let path_action = parsed.path().trim_start_matches('/');
        let action = if path_action.is_empty() {
            parsed.host_str().unwrap_or("")
        } else {
            path_action
        };
        if action.is_empty() {
            continue;
        }

        let mut payload = DeepLinkPayload {
            action: action.to_string(),
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

        // Try emitting — if the frontend hasn't loaded yet the event is
        // silently dropped.  Store a copy in PendingDeepLink as a fallback.
        let _ = app.emit("deep-link-received", payload.clone());

        if let Some(state) = app.try_state::<PendingDeepLink>() {
            *state.0.lock().unwrap() = Some(payload);
        }
    }
}

#[tauri::command]
fn consume_deep_link(state: State<PendingDeepLink>) -> Option<DeepLinkPayload> {
    state.0.lock().unwrap().take()
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
            handle_deep_link_argv(app, &argv);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(PluckState::default())
        .manage(PendingDeepLink::default())
        .setup(|app| {
            tray::create_tray(app.handle())?;
            // A cold launch opened by the yt-plucker:// protocol carries the
            // URL in its own argv; the single-instance callback below only
            // fires for a second instance, so scan here too.
            handle_deep_link_argv(app.handle(), &std::env::args().collect::<Vec<_>>());
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
            commands::import_cookie,
            commands::delete_cookie,
            commands::list_cookies,
            search_commands::list_sites,
            search_commands::search_content,
            search_commands::get_series_detail,
            search_commands::resolve_streams,
            search_commands::start_stream_pluck,
            consume_deep_link
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                kill_all_jobs(app_handle);
            }
        });
}
