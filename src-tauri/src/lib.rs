mod commands;
mod pluck;
mod sidecar;
mod tray;

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager, WindowEvent};

pub struct PluckJob {
    pub pid: u32,
    pub cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct PluckState(pub Mutex<HashMap<u64, PluckJob>>);

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
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
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
            commands::cancel_pluck
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                kill_all_jobs(app_handle);
            }
        });
}
