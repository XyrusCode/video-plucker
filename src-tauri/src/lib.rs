mod bookmarks;
mod commands;
mod compat;
mod dump;
mod engine;
mod errors;
mod extractors;
mod history;
mod ledger;
mod platform;
mod pluck;
mod retry;
mod search_commands;
mod settings;
mod sidecar;
mod ui;
mod updater;

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use rune::{App, Titlebar};

use crate::ui::AppState;

pub struct PluckJob {
    pub pid: u32,
    pub cancelled: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct PluckState(pub Mutex<HashMap<u64, PluckJob>>);

pub fn run() {
    // A second launch hands its arguments to this process over a named pipe
    // and returns false; without this guard two apps would fight over the
    // ledger and the download queue.
    if !rune::single_instance("tech.xyruscode.videoplucker") {
        return;
    }

    let rx = compat::init(compat::default_data_dir(), compat::default_resource_dir());

    let mut app = App::new(ui::spec())
        .titlebar(Titlebar::new(40.0))
        .keep_running()
        .on_close(|s: &mut AppState, _| {
            compat::set_window_visible(false);
            let _ = settings::save(&s.settings);
        })
        .on_open_url(|s: &mut AppState, url: &str, _| {
            ui::handle_deep_link(s, url);
        })
        .init(move || AppState::new(rx));

    let handle = app.handle();
    ui::attach_tray(&app);

    // Cold start from a yt-plucker:// link: the scheme reaches argv (on
    // Windows and Linux) rather than on_open_url.
    for arg in std::env::args() {
        if arg.starts_with("yt-plucker://") {
            let link = arg.clone();
            handle.update(move |s: &mut AppState, _| {
                ui::handle_deep_link(s, &link);
            });
        }
    }

    app.run();
}
