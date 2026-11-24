//! The Rune UI: one state struct, event bridge, actions and six views.

use rune::tok::{self, radius, space, type_};
use rune::{App, AppSpec, Cx, El, FileDialogOptions, Key, KeyEvent, TextEdit, Tray, TrayEvent, TrayIcon, TrayItem, TraySpec, Tx, col, el, row, text, text_input};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use crate::compat::{self, Emitter, Manager, UiEvent};
use crate::{commands, dump, history, ledger, search_commands, settings, updater, PluckState};

// ---------------------------------------------------------------------------
// Data

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Pluck,
    Queue,
    History,
    Saved,
    Search,
    Settings,
}

impl Screen {
    fn label(self) -> &'static str {
        match self {
            Screen::Pluck => "Pluck",
            Screen::Queue => "Queue",
            Screen::History => "History",
            Screen::Saved => "Saved",
            Screen::Search => "Search",
            Screen::Settings => "Settings",
        }
    }
}

const SCREENS: [Screen; 6] = [
    Screen::Pluck,
    Screen::Queue,
    Screen::History,
    Screen::Saved,
    Screen::Search,
    Screen::Settings,
];

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    pub thumbnail: Option<String>,
    pub duration: Option<f64>,
    #[serde(default)]
    pub heights: Vec<u32>,
    pub entry_count: Option<u64>,
    #[serde(default)]
    pub entries: Vec<String>,
    #[serde(default)]
    pub source: String,
    pub id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Tile {
    pub url: String,
    pub title: String,
    pub status: String,
    pub meta: Option<Meta>,
    pub selected: bool,
    pub dup: Option<DupView>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub progress: f64,
    #[serde(default)]
    pub speed: f64,
    #[serde(default)]
    pub eta: f64,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub failure_kind: String,
    #[serde(default)]
    pub friendly: String,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub cancelled: bool,
}

#[derive(Clone, Debug)]
pub struct QueueItem {
    pub id: String,
    pub url: String,
    pub title: String,
    pub quality: String,
    pub thumbnail: String,
    pub playlist_mode: bool,
    pub source: String,
    pub added_at: i64,
}

impl QueueItem {
    fn from_value(v: &Value) -> Option<Self> {
        Some(Self {
            id: v.get("itemId").and_then(Value::as_str).unwrap_or_default().to_string(),
            url: v.get("url").and_then(Value::as_str).unwrap_or_default().to_string(),
            title: v.get("title").and_then(Value::as_str).unwrap_or("Untitled").to_string(),
            quality: v.get("quality").and_then(Value::as_str).unwrap_or("480").to_string(),
            thumbnail: v.get("thumbnailUrl").and_then(Value::as_str).unwrap_or_default().to_string(),
            playlist_mode: v.get("playlistMode").and_then(Value::as_bool).unwrap_or(false),
            source: v.get("source").and_then(Value::as_str).unwrap_or_default().to_string(),
            added_at: v.get("addedAt").and_then(Value::as_i64).unwrap_or(0),
        })
    }

    fn to_value(&self) -> Value {
        serde_json::json!({
            "itemId": self.id,
            "url": self.url,
            "title": self.title,
            "quality": self.quality,
            "thumbnailUrl": self.thumbnail,
            "playlistMode": self.playlist_mode,
            "source": self.source,
            "addedAt": self.added_at,
        })
    }
}

#[derive(Clone, Debug)]
pub struct CookieRow {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedRow {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    pub thumbnail_url: Option<String>,
    #[serde(default)]
    pub site: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRow {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    pub poster: Option<String>,
    #[serde(default)]
    pub site: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeRow {
    #[serde(default)]
    pub number: String,
    #[serde(default)]
    pub id: String,
    pub title: Option<String>,
    #[serde(default)]
    pub selected: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonView {
    pub number: u32,
    #[serde(default)]
    pub episodes: Vec<EpisodeRow>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub seasons: Vec<SeasonView>,
}

#[derive(Clone, Debug)]
pub struct SiteRow {
    pub key: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DupView {
    #[serde(default)] pub title: String,
    #[serde(default)] pub file_name: String,
    #[serde(default)] pub source: String,
    pub downloaded_at: Option<i64>,
}

fn from_value<T: DeserializeOwned>(v: &Value) -> Option<T> {
    serde_json::from_value(v.clone()).ok()
}

// ---------------------------------------------------------------------------
// State

pub struct AppState {
    pub settings: settings::Settings,
    pub screen: Screen,
    pub urls: TextEdit,
    pub playlist_mode: bool,
    pub analyzing: bool,
    pub tiles: Vec<Tile>,
    pub jobs: Vec<Job>,
    pub queue: Vec<QueueItem>,
    pub history: Vec<Value>,
    pub cookies: Vec<CookieRow>,
    pub cookie_name: TextEdit,
    pub sites: Vec<SiteRow>,
    pub site_index: usize,
    pub translation: usize,
    pub query: TextEdit,
    pub results: Vec<SearchRow>,
    pub detail: Option<Detail>,
    pub season: String,
    pub range: TextEdit,
    pub saved: Vec<SavedRow>,
    pub saved_sites: Vec<String>,
    pub saved_site_index: usize,
    pub youtube_lists: Vec<(String, String)>,
    pub youtube_list_index: usize,
    pub settings_pane: usize,
    pub open_select: Option<String>,
    pub terms_open: bool,
    pub update: Option<updater::UpdateInfo>,
    pub toast: Option<String>,
    pub busy: String,
    pub rx: Option<compat::UiReceiver>,
    pub engine: String,
}

const TRANSLATIONS: [&str; 2] = ["sub", "dup"];

impl AppState {
    pub fn new(rx: compat::UiReceiver) -> Self {
        let settings = settings::load();
        let history = settings::load_records("plucks.json");
        let queue = settings::load_records("queue.json")
            .iter()
            .filter_map(QueueItem::from_value)
            .collect();
        let cookies = commands::list_cookies(compat::app_handle())
            .into_iter()
            .map(|c| CookieRow {
                name: c.name,
                path: c.path,
            })
            .collect();
        let terms_open = !settings.terms_accepted;
        Self {
            settings,
            screen: Screen::Pluck,
            urls: TextEdit::new(""),
            playlist_mode: false,
            analyzing: false,
            tiles: Vec::new(),
            jobs: history.iter().filter_map(|v| from_value(v)).collect(),
            queue,
            history,
            cookies,
            cookie_name: TextEdit::new(""),
            sites: crate::extractors::searchable_sites()
                .into_iter()
                .map(|x| SiteRow {
                    key: x.id,
                    name: x.label,
                })
                .collect(),
            site_index: 0,
            translation: 0,
            query: TextEdit::new(""),
            results: Vec::new(),
            detail: None,
            season: String::new(),
            range: TextEdit::new(""),
            saved: Vec::new(),
            saved_sites: Vec::new(),
            saved_site_index: 0,
            youtube_lists: Vec::new(),
            youtube_list_index: 0,
            settings_pane: 0,
            open_select: None,
            terms_open,
            update: None,
            toast: None,
            busy: String::new(),
            rx: Some(rx),
            engine: "Ready".into(),
        }
    }

    fn persist_settings(&self) {
        let _ = settings::save(&self.settings);
    }

    fn persist_records(&self) {
        let queue: Vec<Value> = self.queue.iter().map(QueueItem::to_value).collect();
        let _ = settings::save_records("queue.json", &queue);
        let _ = settings::save_records("plucks.json", &self.history);
    }

    fn next_job_id(&mut self) -> u64 {
        let id = self.settings.next_job_id;
        self.settings.next_job_id = id + 1;
        id
    }

    fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some(msg.into());
    }
}

// ---------------------------------------------------------------------------
// Actions

fn emit_ui(name: &str, value: Value) {
    let _ = compat::app_handle().emit(name, value);
}

pub fn analyze(s: &mut AppState) {
    let raw = s.urls.text.clone();
    let urls: Vec<String> = raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    if urls.is_empty() {
        s.toast("Paste a video or playlist link first");
        return;
    }
    s.tiles = urls
        .iter()
        .map(|u| Tile {
            url: u.clone(),
            title: u.clone(),
            status: "Analyzing…".into(),
            meta: None,
            selected: true,
            dup: None,
        })
        .collect();
    s.analyzing = true;
    let playlist = s.playlist_mode;
    let browser = s.settings.cookies_browser.clone();
    compat::async_runtime::spawn(async move {
        for (i, url) in urls.iter().enumerate() {
            let result = commands::fetch_metadata(
                compat::app_handle(),
                url.clone(),
                playlist,
                if browser == "none" { None } else { Some(browser.clone()) },
            )
            .await;
            match result {
                Ok(meta) => {
                    let dup = meta.id.as_ref().and_then(|id| {
                        ledger::find_duplicate(
                            compat::app_handle(),
                            id.clone(),
                            std::path::Path::new(&settings::load().dest_dir)
                                .to_string_lossy()
                                .into_owned(),
                        )
                    });
                    emit_ui(
                        "ui://metadata",
                        serde_json::json!({"index": i, "meta": serde_json::to_value(meta).unwrap_or(Value::Null), "dup": serde_json::to_value(dup).unwrap_or(Value::Null)}),
                    );
                }
                Err(e) => emit_ui("ui://meta-error", serde_json::json!({"index": i, "error": e})),
            }
        }
        emit_ui("ui://analyze-done", Value::Null);
    });
}

pub fn start_pluck(s: &mut AppState, allow_duplicate: bool) {
    let selected: Vec<Tile> = s.tiles.iter().filter(|t| t.selected).cloned().collect();
    if selected.is_empty() {
        s.toast("Select at least one analyzed link");
        return;
    }
    let quality = s.settings.quality.clone();
    let dest = s.settings.dest_dir.clone();
    let browser = s.settings.cookies_browser.clone();
    let playlist = s.playlist_mode;
    let parallel = s.settings.download_mode == "parallel";
    let max = s.settings.max_concurrent_downloads.max(1) as usize;

    for tile in selected {
        if !allow_duplicate && tile.dup.is_some() {
            s.toast("Duplicate found — use Download anyway to override");
            continue;
        }
        let id = s.next_job_id();
        let meta = tile.meta.clone();
        let (title, source) = match &meta {
            Some(m) => (m.title.clone(), m.source.clone()),
            None => (tile.title.clone(), String::new()),
        };
        let job = Job {
            id,
            title: title.clone(),
            status: "Queued".into(),
            progress: 0.0,
            speed: 0.0,
            eta: 0.0,
            items: Vec::new(),
            failure_kind: String::new(),
            friendly: String::new(),
            paused: false,
            done: false,
            cancelled: false,
        };
        s.jobs.push(job);
        let url = tile.url.clone();
        let q = quality.clone();
        let d = dest.clone();
        let b = browser.clone();
        compat::async_runtime::spawn(async move {
            if let Err(e) = commands::start_pluck(
                compat::app_handle(),
                compat::app_handle().state::<PluckState>(),
                id,
                url,
                q,
                d,
                playlist,
                if b == "none" { None } else { Some(b) },
                Some(title),
                Some(source),
            )
            .await
            {
                emit_ui("ui://start-error", serde_json::json!({"jobId": id, "error": e}));
            }
        });
    }
    s.persist_settings();
    s.persist_records();
    let _ = (parallel, max);
}

pub fn cancel_job(s: &mut AppState, id: u64) {
    let state: crate::compat::State<PluckState> = compat::app_handle().state();
    let _ = commands::cancel_pluck(state, id);
    if let Some(job) = s.jobs.iter_mut().find(|j| j.id == id) {
        job.status = "Cancelled".into();
        job.cancelled = true;
    }
}

pub fn pause_job(s: &mut AppState, id: u64) {
    let state: crate::compat::State<PluckState> = compat::app_handle().state();
    let _ = commands::pause_pluck(state, id);
}

pub fn retry_job(s: &mut AppState, id: u64) {
    if let Some(job) = s.jobs.iter().find(|j| j.id == id).cloned() {
        let tile = Tile {
            url: String::new(),
            title: job.title.clone(),
            status: String::new(),
            meta: None,
            selected: true,
            dup: None,
        };
        s.tiles = vec![tile];
        s.toast("Use the URL field to re-analyze, then Pluck again");
    }
}

pub fn park_job(s: &mut AppState, id: u64) {
    if let Some(job) = s.jobs.iter().find(|j| j.id == id) {
        let item = QueueItem {
            id: format!("q{}", job.id),
            url: String::new(),
            title: job.title.clone(),
            quality: s.settings.quality.clone(),
            thumbnail: String::new(),
            playlist_mode: s.playlist_mode,
            source: String::new(),
            added_at: now_millis(),
        };
        s.queue.push(item);
        s.persist_records();
        cancel_job(s, id);
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn start_queued(s: &mut AppState, index: usize) {
    if let Some(item) = s.queue.get(index).cloned() {
        s.tiles = vec![Tile {
            url: item.url.clone(),
            title: item.title.clone(),
            status: "Ready".into(),
            meta: None,
            selected: true,
            dup: None,
        }];
        s.screen = Screen::Pluck;
        s.toast("Queued item loaded — review and Pluck");
    }
}

pub fn remove_queued(s: &mut AppState, index: usize) {
    if index < s.queue.len() {
        s.queue.remove(index);
        s.persist_records();
    }
}

pub fn clear_finished(s: &mut AppState) {
    s.jobs.retain(|j| !j.done);
    s.persist_records();
}

pub fn refresh_history(s: &mut AppState) {
    s.history = history::history_entries(compat::app_handle())
        .into_iter()
        .filter_map(|h| serde_json::to_value(h).ok())
        .collect();
}

pub fn run_search(s: &mut AppState) {
    let site = s.sites.get(s.site_index).map(|x| x.key.clone()).unwrap_or_default();
    if site.is_empty() {
        s.toast("No search sites available");
        return;
    }
    let q = s.query.text.clone();
    let tr = TRANSLATIONS[s.translation].to_string();
    s.busy = "Searching…".into();
    compat::async_runtime::spawn(async move {
        let r = search_commands::search_content(site, q, tr).await;
        match r {
            Ok(results) => emit_ui(
                "ui://search",
                serde_json::to_value(results).unwrap_or(Value::Null),
            ),
            Err(e) => emit_ui("ui://search-error", Value::String(e)),
        }
    });
}

pub fn open_detail(s: &mut AppState, id: String) {
    let site = s.sites.get(s.site_index).map(|x| x.key.clone()).unwrap_or_default();
    let tr = TRANSLATIONS[s.translation].to_string();
    s.busy = "Loading episodes…".into();
    compat::async_runtime::spawn(async move {
        match search_commands::get_series_detail(site, id, tr).await {
            Ok(detail) => emit_ui("ui://detail", serde_json::to_value(detail).unwrap_or(Value::Null)),
            Err(e) => emit_ui("ui://search-error", Value::String(e)),
        }
    });
}

pub fn download_selection(s: &mut AppState) {
    let Some(detail) = s.detail.clone() else { return };
    let selected: Vec<search_commands::EpisodeSel> = detail
        .seasons
        .iter()
        .flat_map(|s| s.episodes.iter())
        .filter(|e| e.selected)
        .map(|e| search_commands::EpisodeSel {
            episode: e.number.clone(),
            episode_id: Some(e.id.clone()),
            title: e.title.clone(),
        })
        .collect();
    if selected.is_empty() {
        s.toast("Select at least one episode");
        return;
    }
    let id = s.next_job_id();
    let site = s.sites.get(s.site_index).map(|x| x.key.clone()).unwrap_or_default();
    let show_id = detail.id.clone();
    let title = detail.title.clone();
    let tr = TRANSLATIONS[s.translation].to_string();
    let q = s.settings.quality.clone();
    let dest = s.settings.dest_dir.clone();
    s.jobs.push(Job {
        id,
        title: title.clone(),
        status: "Queued".into(),
        ..Job {
            id: 0,
            title: String::new(),
            status: String::new(),
            progress: 0.0,
            speed: 0.0,
            eta: 0.0,
            items: Vec::new(),
            failure_kind: String::new(),
            friendly: String::new(),
            paused: false,
            done: false,
            cancelled: false,
        }
    });
    compat::async_runtime::spawn(async move {
        let _ = search_commands::start_stream_pluck(
            compat::app_handle(),
            id,
            site,
            show_id,
            title,
            selected,
            tr,
            q,
            dest,
        )
        .await;
    });
}

pub fn load_saved(s: &mut AppState) {
    compat::async_runtime::spawn(async move {
        match crate::bookmarks::list_youtube_lists(compat::app_handle()).await {
            Ok(lists) => emit_ui(
                "ui://lists",
                serde_json::to_value(lists).unwrap_or(Value::Null),
            ),
            Err(_) => {}
        }
    });
    let sites: Option<Vec<String>> = if s.saved_sites.is_empty() {
        None
    } else {
        Some(vec![s.saved_sites[s.saved_site_index].clone()])
    };
    let list = s.youtube_lists.get(s.youtube_list_index).map(|x| x.0.clone());
    compat::async_runtime::spawn(async move {
        match crate::bookmarks::fetch_saved(compat::app_handle(), sites, list).await {
            Ok(snapshot) => emit_ui("ui://saved", serde_json::to_value(snapshot).unwrap_or(Value::Null)),
            Err(e) => emit_ui("ui://saved-error", Value::String(e)),
        }
    });
}

pub fn handle_deep_link(s: &mut AppState, url: &str) {
    let parsed = match url::Url::parse(url) {
        Ok(u) => u,
        Err(_) => return,
    };
    let action = {
        let path = parsed.path().trim_start_matches('/');
        if path.is_empty() {
            parsed.host_str().unwrap_or("").to_string()
        } else {
            path.to_string()
        }
    };
    let mut link = String::new();
    let mut quality: Option<String> = None;
    for (k, v) in parsed.query_pairs() {
        match k.as_ref() {
            "url" => link = v.into_owned(),
            "quality" => quality = Some(v.into_owned()),
            _ => {}
        }
    }
    if link.is_empty() {
        return;
    }
    compat::set_window_visible(true);
    s.screen = Screen::Pluck;
    s.urls = TextEdit::new(link.clone());
    s.tiles = vec![Tile {
        url: link,
        title: String::new(),
        status: "Analyzing…".into(),
        meta: None,
        selected: true,
        dup: None,
    }];
    if let Some(q) = quality {
        if let Some(i) = QUALITIES.iter().position(|x| *x == q.as_str()) {
            s.settings.quality = q;
            s.persist_settings();
            let _ = i;
        }
    }
    if action == "pluck" {
        analyze(s);
    } else {
        analyze(s);
    }
}

// ---------------------------------------------------------------------------
// Event bridge

pub fn apply_event(s: &mut AppState, event: UiEvent) {
    let p = event.payload;
    match event.name.as_str() {
        "pluck://progress" => {
            if let Some(id) = p.get("jobId").and_then(Value::as_u64) {
                if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) {
                    j.progress = p.get("percent").and_then(Value::as_f64).unwrap_or(j.progress);
                    j.speed = p.get("speed").and_then(Value::as_f64).unwrap_or(0.0);
                    j.eta = p.get("eta").and_then(Value::as_f64).unwrap_or(0.0);
                    j.status = "Downloading".into();
                }
            }
        }
        "pluck://item-start" => {
            if let Some(id) = p.get("jobId").and_then(Value::as_u64) {
                if let Some(title) = p.get("title").and_then(Value::as_str) {
                    if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) {
                        j.items.push(title.to_string());
                        j.status = "Downloading".into();
                    }
                }
            }
        }
        "pluck://item-done" => {}
        "pluck://error" => {
            let friendly = p.get("friendly").and_then(Value::as_str).unwrap_or("Download failed").to_string();
            if let Some(id) = p.get("jobId").and_then(Value::as_u64) {
                if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) {
                    j.friendly = friendly.clone();
                    j.failure_kind = p.get("kind").and_then(Value::as_str).unwrap_or("other").to_string();
                }
            }
            s.toast(friendly);
        }
        "pluck://status" => {
            if let (Some(id), Some(msg)) = (p.get("jobId").and_then(Value::as_u64), p.get("message").and_then(Value::as_str)) {
                if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) {
                    j.status = msg.to_string();
                }
            }
        }
        "pluck://done" => {
            if let Some(id) = p.get("jobId").and_then(Value::as_u64) {
                if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) {
                    j.done = true;
                    j.progress = if p.get("ok").and_then(Value::as_bool).unwrap_or(false) { 100.0 } else { j.progress };
                    j.status = if p.get("ok").and_then(Value::as_bool).unwrap_or(false) { "Done" }
                        else if p.get("cancelled").and_then(Value::as_bool).unwrap_or(false) { "Cancelled" }
                        else { "Failed" }.into();
                    j.friendly = p.get("friendly").and_then(Value::as_str).unwrap_or("").to_string();
                }
            }
            refresh_history(s);
            s.persist_records();
        }
        "pluck://paused" => {
            if let Some(id) = p.get("jobId").and_then(Value::as_u64) {
                if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) {
                    j.paused = true;
                    j.status = "Paused".into();
                }
            }
        }
        "pluck://cookies" => {
            if let Some(file) = p.get("file").and_then(Value::as_str) {
                s.toast(format!("Using cookies: {file}"));
            }
        }
        "ui://metadata" => {
            let i = p.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            if let Some(tile) = s.tiles.get_mut(i) {
                tile.meta = from_value(p.get("meta").unwrap_or(&Value::Null));
                tile.dup = from_value(p.get("dup").unwrap_or(&Value::Null));
                tile.status = "Ready".into();
                if let Some(m) = &tile.meta {
                    tile.title = m.title.clone();
                }
            }
        }
        "ui://meta-error" => {
            let i = p.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            if let Some(tile) = s.tiles.get_mut(i) {
                tile.status = p.get("error").and_then(Value::as_str).unwrap_or("Failed").to_string();
            }
        }
        "ui://analyze-done" => s.analyzing = false,
        "ui://start-error" => {
            if let Some(msg) = p.get("error").and_then(Value::as_str) {
                s.toast(msg.to_string());
            }
        }
        "ui://search" => s.results = from_value(&p).unwrap_or_default(),
        "ui://search-error" | "ui://saved-error" => {
            s.busy.clear();
            if let Some(msg) = p.as_str() {
                s.toast(msg.to_string());
            }
        }
        "ui://detail" => {
            s.busy.clear();
            s.detail = from_value(&p);
        }
        "ui://saved" => {
            s.saved = p.get("items").and_then(from_value).unwrap_or_default();
            s.saved_sites = p.get("connectedSites").and_then(from_value).unwrap_or_default();
        }
        "ui://lists" => {
            #[derive(Deserialize)]
            struct List {
                id: String,
                label: String,
            }
            s.youtube_lists = from_value::<Vec<List>>(&p)
                .unwrap_or_default()
                .into_iter()
                .map(|l| (l.id, l.label))
                .collect();
        }
        "ui://update" => s.update = from_value(&p),
        "ui://toast" => {
            if let Some(msg) = p.as_str() {
                s.toast(msg.to_string());
            }
        }
        "deep-link-received" => {
            if let Some(url) = p.get("url").and_then(Value::as_str) {
                let link = url.to_string();
                handle_deep_link(s, &link);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// View

const QUALITIES: [&str; 8] = ["480", "720", "1080", "1440", "2160", "best", "mp3", "m4a"];
const BROWSERS: [&str; 7] = ["none", "chrome", "edge", "firefox", "brave", "opera", "vivaldi"];
const SETTINGS_PANES: [&str; 5] = ["Downloads", "Updates", "Cookies", "Backup & Restore", "About"];

pub fn view(s: &AppState, cx: &mut Cx<AppState>) -> El {
    let dark = match s.settings.theme_mode.as_str() {
        "dark" => true,
        "light" => false,
        _ => cx.system_dark(),
    };
    let t = tok::theme(dark);
    let mut root = col().key("window").w_full().h_full().fill(t.bg);

    // 40 pt custom title bar
    root = root.child(
        row()
            .key("titlebar")
            .h(40.0)
            .w_full()
            .px(space::S3)
            .center_y()
            .justify_between()
            .fill(t.card)
            .child(
                text("Video Plucker")
                    .key("app-title")
                    .font(type_::TITLE)
                    .color(t.text),
            )
    );

    let mut body = row().key("body").grow(1.0).min_h(0.0);
    // Rail
    let mut rail = col()
        .key("rail")
        .w(180.0)
        .h_full()
        .py(space::S3)
        .px(space::S2)
        .gap(space::S1)
        .fill(t.card);
    for screen in SCREENS {
        let active = s.screen == screen;
        rail = rail.child(
            el()
                .key(format!("nav-{:?}", screen))
                .role("button")
                .label(screen.label())
                .h(36.0)
                .px(space::S3)
                .center_y()
                .radius(radius::MD)
                .fill(if active { t.indicator } else { t.card })
                .hover(t.hover)
                .on_press(move |s: &mut AppState| {
                    s.screen = screen;
                    s.open_select = None;
                })
                .child(text(screen.label()).font(type_::LABEL).color(if active { t.text } else { t.text_secondary })),
        );
    }
    body = body.child(rail);

    let content = match s.screen {
        Screen::Pluck => pluck_view(s, t, dark),
        Screen::Queue => queue_view(s, t),
        Screen::History => history_view(s, t),
        Screen::Saved => saved_view(s, t),
        Screen::Search => search_view(s, t),
        Screen::Settings => settings_view(s, t),
    };
    body = body.child(
        col()
            .key("content")
            .grow(1.0)
            .min_h(0.0)
            .px(space::S6)
            .py(space::S3)
            .gap(space::S3)
            .child(content),
    );
    root = root.child(body);

    // Status bar
    let active = s.jobs.iter().filter(|j| !j.done).count();
    root = root.child(
        row()
            .key("statusbar")
            .h(28.0)
            .w_full()
            .px(space::S3)
            .center_y()
            .justify_between()
            .fill(t.card)
            .child(
                row()
                    .key("engine")
                    .gap(space::S2)
                    .center_y()
                    .child(el().key("engine-dot").size(7.0, 7.0).radius(4.0).fill(accent()))
                    .child(text(if s.busy.is_empty() { s.engine.clone() } else { s.busy.clone() }).font(type_::LABEL).color(t.text_secondary)),
            )
            .child(text(format!("{active} active · {}", s.settings.dest_dir)).font(type_::LABEL).color(t.text_secondary))
            .child(text(env!("CARGO_PKG_VERSION")).font(type_::LABEL).color(t.text_secondary)),
    );

    if let Some(msg) = &s.toast {
        root = root.child(
            el()
                .key("toast")
                .absolute(0.0, 44.0)
                .px(space::S3)
                .py(space::S2)
                .radius(radius::MD)
                .fill(t.card)
                .shadow(0.0, 8.0, t.shadow)
                .child(text(msg.clone()).font(type_::LABEL).color(t.text)),
        );
    }
    if s.terms_open {
        root = root.child(terms_dialog(t));
    }
    if s.update.is_some() {
        root = root.child(update_banner(s, t));
    }
    root
}

fn section_title(title: &str, t: tok::Theme) -> El {
    text(title.to_string()).key("section-title").font(type_::TITLE).color(t.text)
}

fn button(key: &'static str, label: &str, t: tok::Theme, primary: bool, action: impl Fn(&mut AppState) + 'static) -> El {
    el()
        .key(key)
        .role("button")
        .label(label.to_string())
        .focusable(true)
        .h(32.0)
        .px(space::S3)
        .center()
        .radius(radius::MD)
        .fill(if primary { accent() } else { t.control_off })
        .hover(if primary { accent().alpha(0.88) } else { t.hover })
        .press(t.press)
        .on_press(action)
        .child(
            text(label.to_string())
                .font(type_::LABEL)
                .color(if primary { rune::Color::WHITE } else { t.text })
                .a11y_hidden(),
        )
}

fn picker(
    key: &'static str,
    label: &str,
    options: &[&str],
    current: usize,
    is_open: bool,
    t: tok::Theme,
    choose: impl Fn(&mut AppState, usize) + Clone + 'static,
) -> El {
    let shown = options.get(current).copied().unwrap_or("");
    let mut stack = col().key(key).gap(space::S1);
    stack = stack.child(
        el()
            .key(format!("{key}-trigger"))
            .role("button")
            .label(label.to_string())
            .h(32.0)
            .px(space::S3)
            .center_y()
            .justify_between()
            .radius(radius::MD)
            .fill(t.control_off)
            .on_press(move |s: &mut AppState| {
                s.open_select = if s.open_select.as_deref() == Some(key) {
                    None
                } else {
                    Some(key.to_string())
                };
            })
            .child(text(shown.to_string()).font(type_::LABEL).color(t.text)),
    );
    if is_open {
        let mut list = col().key(format!("{key}-options")).gap(1.0);
        for (i, opt) in options.iter().enumerate() {
            let o = (*opt).to_string();
            let choose = choose.clone();
            list = list.child(
                el()
                    .key(format!("{key}-opt-{i}"))
                    .role("option")
                    .label(o.clone())
                    .h(28.0)
                    .px(space::S3)
                    .center_y()
                    .radius(radius::SM)
                    .fill(if i == current { t.indicator } else { t.card })
                    .hover(t.hover)
                    .on_press(move |s: &mut AppState| {
                        choose(s, i);
                        s.open_select = None;
                    })
                    .child(text(o).font(type_::LABEL).color(t.text)),
            );
        }
        stack = stack.child(list);
    }
    stack
}

fn accent() -> rune::Color {
    tok::accent(0, true)
}

fn button_tx(
    key: &'static str,
    label: &str,
    t: tok::Theme,
    primary: bool,
    action: impl Fn(&mut AppState, &mut Tx) + 'static,
) -> El {
    el()
        .key(key)
        .role("button")
        .label(label.to_string())
        .focusable(true)
        .h(32.0)
        .px(space::S3)
        .center()
        .radius(radius::MD)
        .fill(if primary { accent() } else { t.control_off })
        .hover(if primary { accent().alpha(0.88) } else { t.hover })
        .press(t.press)
        .on_press_tx(action)
        .child(
            text(label.to_string())
                .font(type_::LABEL)
                .color(if primary { rune::Color::WHITE } else { t.text })
                .a11y_hidden(),
        )
}

fn pluck_view(s: &AppState, t: tok::Theme, dark: bool) -> El {
    let mut root = col().key("view-pluck").w_full().grow(1.0).min_h(0.0).gap(space::S3);
    root = root.child(section_title("Pluck", t));

    root = root.child(
        col()
            .key("input")
            .gap(space::S2)
            .child(
                text_input(&s.urls, type_::BODY)
                    .key("urls")
                    .placeholder("Paste one or more video or playlist links…", t.text_secondary)
                    .w_full()
                    .h(72.0)
                    .padding(space::S3)
                    .radius(radius::MD)
                    .fill(t.card)
                    .color(t.text)
                    .on_edit(|s: &mut AppState| &mut s.urls),
            )
            .child(
                row()
                    .key("input-actions")
                    .gap(space::S2)
                    .center_y()
                    .child(
                        el().key("playlist").role("switch").label("Playlist").checked(s.playlist_mode)
                            .focusable(true).h(28.0).px(space::S3).center_y().radius(radius::MD)
                            .fill(if s.playlist_mode { accent() } else { t.control_off })
                            .on_press(|s: &mut AppState| s.playlist_mode = !s.playlist_mode)
                            .child(text("Playlist").font(type_::LABEL).color(if s.playlist_mode { rune::Color::WHITE } else { t.text })),
                    )
                    .child(button("analyze", "Analyze", t, false, analyze))
                    .child(button("clear-input", "Clear", t, false, |s: &mut AppState| s.urls.clear())),
            ),
    );

    if !s.tiles.is_empty() {
        let mut grid = col().key("tiles").gap(space::S2);
        for (i, tile) in s.tiles.iter().enumerate() {
            let selected = tile.selected;
            let status = tile.status.clone();
            let title = tile.title.clone();
            grid = grid.child(
                row()
                    .key(format!("tile-{i}"))
                    .h(48.0)
                    .px(space::S3)
                    .gap(space::S3)
                    .center_y()
                    .radius(radius::MD)
                    .fill(if selected { t.indicator } else { t.card })
                    .hover(t.hover)
                    .on_press(move |s: &mut AppState| s.tiles[i].selected = !s.tiles[i].selected)
                    .child(
                        el().key(format!("tile-check-{i}"))
                            .size(14.0, 14.0)
                            .radius(3.0)
                            .fill(if selected { accent() } else { t.control_off }),
                    )
                    .child(text(title).font(type_::BODY).color(t.text).grow(1.0).shrink())
                    .child(text(status).font(type_::LABEL).color(t.text_secondary)),
            );
        }
        root = root.child(grid);
        let has_dup = s.tiles.iter().any(|x| x.dup.is_some());
        if has_dup {
            root = root.child(
                row().key("dup-banner").gap(space::S2).center_y()
                    .child(text("A duplicate was found in your library.").font(type_::LABEL).color(t.text))
                    .child(button("download-anyway", "Download anyway", t, false, |s: &mut AppState| start_pluck(s, true))),
            );
        }
        root = root.child(
            row().key("quality-row").gap(space::S3).center_y()
                .child(picker("quality", "Quality", &QUALITIES, quality_index(&s.settings.quality), s.open_select.as_deref() == Some("quality"), t, |s, i| {
                    s.settings.quality = QUALITIES[i].to_string();
                    s.persist_settings();
                }))
                .child(picker("browser", "Browser cookies", &BROWSERS, browser_index(&s.settings.cookies_browser), s.open_select.as_deref() == Some("browser"), t, |s, i| {
                    s.settings.cookies_browser = BROWSERS[i].to_string();
                    s.persist_settings();
                }))
                .child(button_tx("browse", "Destination…", t, false, browse_destination))
                .child(button("pluck", "Pluck", t, true, |s: &mut AppState| start_pluck(s, false))),
        );
        root = root.child(text(format!("Destination: {}", s.settings.dest_dir)).font(type_::LABEL).color(t.text_secondary));
    }

    // Downloads table
    let mut table = col().key("downloads").gap(1.0);
    for j in s.jobs.iter().rev() {
        let (id, title, status, progress, friendly, paused, done) =
            (j.id, j.title.clone(), j.status.clone(), j.progress, j.friendly.clone(), j.paused, j.done);
        let mut r = row()
            .key(format!("job-{id}"))
            .h(38.0)
            .px(space::S3)
            .gap(space::S3)
            .center_y()
            .radius(radius::SM)
            .fill(t.card)
            .child(text(title).font(type_::BODY).color(t.text).grow(1.0).shrink())
            .child(text(format!("{progress:.0}%")).font(type_::LABEL).color(t.text_secondary))
            .child(text(status).font(type_::LABEL).color(t.text_secondary));
        if !friendly.is_empty() {
            r = r.child(text(friendly).font(type_::LABEL).color(t.text_secondary).shrink());
        }
        if done {
            r = r.child(button("job-retry", "Retry", t, false, move |s: &mut AppState| retry_job(s, id)));
        } else {
            let label = if paused { "Resume" } else { "Pause" };
            r = r.child(button("job-pause", label, t, false, move |s: &mut AppState| pause_job(s, id)));
            r = r.child(button("job-cancel", "Cancel", t, false, move |s: &mut AppState| cancel_job(s, id)));
            r = r.child(button("job-park", "Park", t, false, move |s: &mut AppState| park_job(s, id)));
        }
        table = table.child(r);
    }
    root = root.child(table);
    if s.jobs.iter().any(|j| j.done) {
        root = root.child(button("clear-finished", "Clear finished", t, false, clear_finished));
    }
    let _ = dark;
    root
}

fn quality_index(q: &str) -> usize {
    QUALITIES.iter().position(|x| *x == q).unwrap_or(0)
}
fn browser_index(b: &str) -> usize {
    BROWSERS.iter().position(|x| *x == b).unwrap_or(0)
}

fn browse_destination(s: &mut AppState, tx: &mut Tx) {
    let opts = FileDialogOptions {
        title: "Choose download folder".into(),
        ..Default::default()
    };
    tx.open_folder(opts, |s: &mut AppState, result, _tx| {
        if let Ok(Some(path)) = result {
            s.settings.dest_dir = path.to_string_lossy().into_owned();
            s.persist_settings();
        }
    });
}

fn queue_view(s: &AppState, t: tok::Theme) -> El {
    let mut root = col().key("view-queue").w_full().grow(1.0).min_h(0.0).gap(space::S3)
        .child(section_title("Queue", t));
    let mut table = col().key("queue-list").gap(1.0);
    for (i, item) in s.queue.iter().enumerate() {
        table = table.child(
            row().key(format!("q-{i}")).h(38.0).px(space::S3).gap(space::S3).center_y().fill(t.card)
                .child(text(item.title.clone()).font(type_::BODY).color(t.text).grow(1.0).shrink())
                .child(text(item.quality.clone()).font(type_::LABEL).color(t.text_secondary))
                .child(button("q-start", "Start", t, false, move |s: &mut AppState| start_queued(s, i)))
                .child(button("q-remove", "Remove", t, false, move |s: &mut AppState| remove_queued(s, i))),
        );
    }
    root = root.child(table);
    if !s.queue.is_empty() {
        root = root.child(button("q-all", "Download all", t, true, |s: &mut AppState| {
            if !s.queue.is_empty() {
                start_queued(s, 0);
            }
        }));
    }
    root
}

fn history_view(s: &AppState, t: tok::Theme) -> El {
    let mut root = col().key("view-history").w_full().grow(1.0).min_h(0.0).gap(space::S3)
        .child(section_title("History", t));
    let mut table = col().key("history-list").gap(1.0);
    for (i, h) in s.history.iter().enumerate() {
        let title = h.get("title").and_then(Value::as_str).unwrap_or("Untitled").to_string();
        let platform = h.get("platform").and_then(Value::as_str).unwrap_or("").to_string();
        let status = h.get("status").and_then(Value::as_str).unwrap_or("").to_string();
        let id = h.get("id").and_then(Value::as_str).unwrap_or("").to_string();
        let url = h.get("url").and_then(Value::as_str).unwrap_or("").to_string();
        table = table.child(
            row().key(format!("h-{i}")).h(38.0).px(space::S3).gap(space::S3).center_y().fill(t.card)
                .child(text(title).font(type_::BODY).color(t.text).grow(1.0).shrink())
                .child(text(platform).font(type_::LABEL).color(t.text_secondary))
                .child(text(status).font(type_::LABEL).color(t.text_secondary))
                .child(button("h-again", "Download again", t, false, move |s: &mut AppState| {
                    if !url.is_empty() {
                        s.urls = TextEdit::new(url.clone());
                        s.screen = Screen::Pluck;
                        analyze(s);
                    }
                }))
                .child(button("h-remove", "Remove", t, false, move |s: &mut AppState| {
                    let _ = history::history_remove(compat::app_handle(), id.clone());
                    refresh_history(s);
                })),
        );
    }
    root = root.child(table);
    root = root.child(button("h-clear", "Clear history", t, false, |s: &mut AppState| {
        let _ = history::history_clear(compat::app_handle());
        refresh_history(s);
    }));
    root
}

fn saved_view(s: &AppState, t: tok::Theme) -> El {
    let mut root = col().key("view-saved").w_full().grow(1.0).min_h(0.0).gap(space::S3)
        .child(section_title("Saved", t));
    let sites: Vec<String> = if s.saved_sites.is_empty() { vec!["All connected".into()] } else { s.saved_sites.clone() };
    root = root.child(
        row().key("saved-controls").gap(space::S3).center_y()
            .child(picker("saved-site", "Site", &sites.iter().map(|x| x.as_str()).collect::<Vec<_>>(), s.saved_site_index, s.open_select.as_deref() == Some("saved-site"), t, |s, i| {
                s.saved_site_index = i;
                load_saved(s);
            }))
            .child(button("saved-refresh", "Refresh", t, false, |s: &mut AppState| load_saved(s)))
            .child(button("saved-all", "Download all", t, true, |s: &mut AppState| {
                let links: Vec<String> = s.saved.iter().map(|x| x.url.clone()).collect();
                s.urls = TextEdit::new(links.join("\n"));
                s.screen = Screen::Pluck;
                analyze(s);
            })),
    );
    let mut grid = col().key("saved-grid").gap(space::S2).grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false });
    for item in &s.saved {
        grid = grid.child(
            row().key(format!("saved-{}", item.id)).h(44.0).px(space::S3).center_y().fill(t.card)
                .child(text(item.title.clone()).font(type_::BODY).color(t.text).grow(1.0).shrink())
                .child(text(item.site.clone()).font(type_::LABEL).color(t.text_secondary)),
        );
    }
    root = root.child(grid);
    root
}

fn search_view(s: &AppState, t: tok::Theme) -> El {
    let mut root = col().key("view-search").w_full().grow(1.0).min_h(0.0).gap(space::S3)
        .child(section_title("Search", t));
    let site_names: Vec<String> = s.sites.iter().map(|x| x.name.clone()).collect();
    root = root.child(
        row().key("search-controls").gap(space::S3).center_y()
            .child(picker("site", "Site", &site_names.iter().map(|x| x.as_str()).collect::<Vec<_>>(), s.site_index, s.open_select.as_deref() == Some("site"), t, |s, i| {
                s.site_index = i;
            }))
            .child(
                text_input(&s.query, type_::BODY)
                    .key("query")
                    .placeholder("Search anime…", t.text_secondary)
                    .w(220.0)
                    .h(32.0)
                    .px(space::S3)
                    .radius(radius::MD)
                    .fill(t.card)
                    .color(t.text)
                    .on_edit(|s: &mut AppState| &mut s.query)
                    .on_submit(|s: &mut AppState, _| run_search(s)),
            )
            .child(picker("translation", "Track", &TRANSLATIONS, s.translation, s.open_select.as_deref() == Some("translation"), t, |s, i| s.translation = i))
            .child(button("search-go", "Search", t, true, run_search)),
    );
    let mut grid = col().key("results").gap(space::S2).grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false });
    for r in &s.results {
        let id = r.id.clone();
        let title = r.title.clone();
        grid = grid.child(
            el().key(format!("result-{id}")).h(52.0).px(space::S3).center_y().fill(t.card).hover(t.hover)
                .on_press(move |s: &mut AppState| open_detail(s, id.clone()))
                .child(text(title).font(type_::BODY).color(t.text)),
        );
    }
    root = root.child(grid);
    if let Some(detail) = &s.detail {
        let mut eps = col().key("episodes").gap(2.0).h(200.0).scroll(rune::ScrollSpec { stick_bottom: false });
        let mut idx = 0usize;
        for season in &detail.seasons {
            for ep in &season.episodes {
                let i = idx;
                idx += 1;
                let label = ep.title.clone().unwrap_or_else(|| format!("Episode {}", ep.number));
                let selected = ep.selected;
                eps = eps.child(
                    el().key(format!("ep-{i}")).h(30.0).px(space::S3).center_y().fill(if selected { t.indicator } else { t.card })
                        .hover(t.hover).on_press(move |s: &mut AppState| {
                            if let Some(d) = s.detail.as_mut() {
                                let mut n = 0;
                                'outer: for season in d.seasons.iter_mut() {
                                    for e in season.episodes.iter_mut() {
                                        if n == i {
                                            e.selected = !e.selected;
                                            break 'outer;
                                        }
                                        n += 1;
                                    }
                                }
                            }
                        })
                        .child(text(label).font(type_::LABEL).color(t.text)),
                );
            }
        }
        root = root.child(
            col().key("detail").gap(space::S2).child(text(detail.title.clone()).font(type_::TITLE).color(t.text)).child(eps)
                .child(button("download-selection", "Download selected", t, true, download_selection)),
        );
    }
    root
}

fn settings_view(s: &AppState, t: tok::Theme) -> El {
    let mut root = col().key("view-settings").w_full().grow(1.0).min_h(0.0).gap(space::S3)
        .child(section_title("Settings", t));
    let mut panes = row().key("settings-panes").gap(space::S2);
    for (i, name) in SETTINGS_PANES.iter().enumerate() {
        let active = s.settings_pane == i;
        panes = panes.child(
            el().key(format!("pane-{i}")).h(30.0).px(space::S3).center_y().radius(radius::MD)
                .fill(if active { t.indicator } else { t.card }).hover(t.hover)
                .on_press(move |s: &mut AppState| s.settings_pane = i)
                .child(text((*name).to_string()).font(type_::LABEL).color(t.text)),
        );
    }
    root = root.child(panes);

    match s.settings_pane {
        0 => {
            root = root.child(picker("quality", "Default quality", &QUALITIES, quality_index(&s.settings.quality), s.open_select.as_deref() == Some("quality"), t, |s, i| {
                s.settings.quality = QUALITIES[i].to_string(); s.persist_settings();
            }));
            let modes = ["queue", "parallel"];
            root = root.child(picker("mode", "Download mode", &modes, if s.settings.download_mode == "parallel" { 1 } else { 0 }, s.open_select.as_deref() == Some("mode"), t, move |s, i| {
                s.settings.download_mode = modes[i].to_string(); s.persist_settings();
            }));
            let mut conc = row().key("concurrency").gap(space::S3).center_y()
                .child(text("Concurrent downloads".to_string()).font(type_::LABEL).color(t.text));
            for n in 1..=8u32 {
                let active = s.settings.max_concurrent_downloads == n;
                conc = conc.child(
                    el().key(format!("conc-{n}")).size(26.0, 26.0).center().radius(radius::SM)
                        .fill(if active { accent() } else { t.control_off })
                        .on_press(move |s: &mut AppState| { s.settings.max_concurrent_downloads = n; s.persist_settings(); })
                        .child(text(n.to_string()).font(type_::LABEL).color(if active { rune::Color::WHITE } else { t.text })),
                );
            }
            root = root.child(conc);
            let themes = ["system", "light", "dark"];
            root = root.child(picker("theme", "Theme", &themes, theme_index(&s.settings.theme_mode), s.open_select.as_deref() == Some("theme"), t, move |s, i| {
                s.settings.theme_mode = themes[i].to_string(); s.persist_settings();
            }));
            root = root.child(button_tx("browse", "Destination…", t, false, browse_destination));
            root = root.child(text(format!("Destination: {}", s.settings.dest_dir)).font(type_::LABEL).color(t.text_secondary));
            root = root.child(button("clear-temp", "Clear temporary files", t, false, |s: &mut AppState| {
                let state: crate::compat::State<PluckState> = compat::app_handle().state();
                match commands::clear_temp(state, s.settings.dest_dir.clone()) {
                    Ok(n) => s.toast(format!("Removed {n} temporary items")),
                    Err(e) => s.toast(e),
                }
            }));
        }
        1 => {
            root = root.child(
                el().key("auto-update").role("switch").checked(s.settings.auto_update).focusable(true)
                    .h(30.0).px(space::S3).center_y().radius(radius::MD)
                    .fill(if s.settings.auto_update { accent() } else { t.control_off })
                    .on_press(|s: &mut AppState| { s.settings.auto_update = !s.settings.auto_update; s.persist_settings(); })
                    .child(text(if s.settings.auto_update { "Auto-update: on" } else { "Auto-update: off" }.to_string()).font(type_::LABEL).color(rune::Color::WHITE)),
            );
            root = root.child(button_tx("check-update", "Check for updates", t, false, check_update));
        }
        2 => {
            root = root.child(
                row().key("cookie-input").gap(space::S2).center_y()
                    .child(
                        text_input(&s.cookie_name, type_::BODY).key("cookie-name").placeholder("Profile name", t.text_secondary)
                            .w(180.0).h(32.0).px(space::S3).radius(radius::MD).fill(t.card).color(t.text)
                            .on_edit(|s: &mut AppState| &mut s.cookie_name),
                    )
                    .child(button_tx("import-cookie", "Import cookies.txt…", t, false, import_cookie)),
            );
            let mut list = col().key("cookies").gap(1.0);
            for (i, c) in s.cookies.iter().enumerate() {
                let name = c.name.clone();
                list = list.child(
                    row().key(format!("cookie-{i}")).h(32.0).px(space::S3).gap(space::S3).center_y().fill(t.card)
                        .child(text(name.clone()).font(type_::LABEL).color(t.text).grow(1.0))
                        .child(button("cookie-del", "Remove", t, false, move |s: &mut AppState| {
                            let _ = commands::delete_cookie(compat::app_handle(), name.clone());
                            s.cookies = commands::list_cookies(compat::app_handle()).into_iter()
                                .map(|c| CookieRow { name: c.name, path: c.path }).collect();
                        })),
                );
            }
            root = root.child(list);
        }
        3 => {
            root = root.child(button_tx("export-dump", "Export backup…", t, false, export_dump));
            root = root.child(button_tx("import-dump", "Import backup…", t, false, import_dump));
        }
        _ => {
            root = root.child(text("Video Plucker 6.0.0".to_string()).font(type_::TITLE).color(t.text));
            root = root.child(text("A native desktop app for turning video links into files.".to_string()).font(type_::BODY).color(t.text_secondary));
            root = root.child(button("terms", "Terms of Use", t, false, |s: &mut AppState| s.terms_open = true));
        }
    }
    root
}

fn theme_index(m: &str) -> usize {
    match m { "light" => 1, "dark" => 2, _ => 0 }
}

fn check_update(_s: &mut AppState, tx: &mut Tx) {
    let _ = tx;
    compat::async_runtime::spawn(async move {
        match updater::check(true).await {
            Ok(Some(info)) => {
                emit_ui("ui://update", serde_json::to_value(info).unwrap_or(Value::Null));
            }
            Ok(None) => emit_ui("ui://toast", Value::String("You're up to date".into())),
            Err(e) => emit_ui("ui://toast", Value::String(e)),
        }
    });
}

fn import_cookie(s: &mut AppState, tx: &mut Tx) {
    let name = s.cookie_name.text.trim().to_string();
    if name.is_empty() {
        s.toast("Enter a profile name first");
        return;
    }
    let opts = FileDialogOptions {
        title: "Choose cookies.txt".into(),
        ..Default::default()
    };
    tx.open_file(opts, move |s: &mut AppState, result, _tx| {
        if let Ok(Some(path)) = result {
            match commands::import_cookie(compat::app_handle(), name.clone(), path.to_string_lossy().into_owned()) {
                Ok(saved) => {
                    s.cookie_name.clear();
                    s.toast(format!("Imported {saved}"));
                    s.cookies = commands::list_cookies(compat::app_handle()).into_iter()
                        .map(|c| CookieRow { name: c.name, path: c.path }).collect();
                }
                Err(e) => s.toast(e),
            }
        }
    });
}

fn export_dump(s: &mut AppState, tx: &mut Tx) {
    let settings_value = serde_json::to_value(&s.settings).unwrap_or(Value::Null);
    let parked: Vec<(String, Value)> = s.queue.iter().map(|q| (q.id.clone(), q.to_value())).collect();
    let opts = FileDialogOptions {
        title: "Export backup".into(),
        ..Default::default()
    };
    tx.save_file(opts, move |s: &mut AppState, result, _tx| {
        if let Ok(Some(path)) = result {
            let handle = compat::app_handle();
            let prefs = settings_value.clone();
            let items = parked.clone();
            match dump::export_dump(handle, path.to_string_lossy().into_owned(), prefs, items) {
                Ok(counts) => s.toast(format!("Exported {} ledger entries", counts.ledger_entries)),
                Err(e) => s.toast(e),
            }
        }
    });
}

fn import_dump(s: &mut AppState, tx: &mut Tx) {
    let opts = FileDialogOptions {
        title: "Import backup".into(),
        ..Default::default()
    };
    tx.open_file(opts, |s: &mut AppState, result, _tx| {
        if let Ok(Some(path)) = result {
            match dump::import_dump(compat::app_handle(), path.to_string_lossy().into_owned()) {
                Ok(result) => {
                    if let Some(loaded) = settings::from_dump_prefs(&result.prefs) {
                        s.settings = loaded;
                        let _ = settings::save(&s.settings);
                    }
                    if let Some(items) = result.queue.as_array() {
                        for item in items {
                            if let Some(q) = QueueItem::from_value(item) {
                                if !s.queue.iter().any(|x| x.id == q.id) {
                                    s.queue.push(q);
                                }
                            }
                        }
                        s.persist_records();
                    }
                    s.cookies = commands::list_cookies(compat::app_handle()).into_iter()
                        .map(|c| CookieRow { name: c.name, path: c.path }).collect();
                    refresh_history(s);
                    s.toast("Backup imported");
                }
                Err(e) => s.toast(e),
            }
        }
    });
}

fn terms_dialog(t: tok::Theme) -> El {
    el().key("terms-backdrop").absolute(0.0, 0.0).w_full().h_full().center()
        .fill(rune::Color::BLACK.alpha(0.4))
        .child(
            col().key("terms-dialog").role("dialog").label("Terms of Use").modal(true, |s: &mut AppState| {
                s.settings.terms_accepted = true;
                s.terms_open = false;
                s.persist_settings();
            }).w(440.0).padding(space::S6).gap(space::S3).fill(t.card).radius(radius::LG)
                .shadow(8.0, 28.0, t.shadow)
                .child(text("Terms of Use".to_string()).font(type_::TITLE).color(t.text))
                .child(text("Video Plucker downloads videos you choose, to your own device. You are responsible for the content you download and for complying with the terms of the sites you use. Your cookies stay on this device.".to_string())
                    .font(type_::BODY).color(t.text_secondary).wrap().w_full())
                .child(button("terms-accept", "Accept", t, true, |s: &mut AppState| {
                    s.settings.terms_accepted = true;
                    s.terms_open = false;
                    s.persist_settings();
                })),
        )
}

fn update_banner(s: &AppState, t: tok::Theme) -> El {
    let version = s.update.as_ref().map(|u| u.version.clone()).unwrap_or_default();
    el().key("update-banner").absolute(0.0, 44.0).w_full().px(space::S3).py(space::S2)
        .fill(t.indicator)
        .child(
            row().key("update-row").w_full().gap(space::S3).center_y()
                .child(text(format!("Version {version} is available").to_string()).font(type_::LABEL).color(t.text).grow(1.0))
                .child(button_tx("update-install", "Install", t, true, install_update))
                .child(button("update-later", "Later", t, false, |s: &mut AppState| s.update = None)),
        )
}

fn install_update(s: &mut AppState, tx: &mut Tx) {
    let Some(info) = s.update.clone() else { return };
    let _ = tx;
    compat::async_runtime::spawn(async move {
        match updater::download(&info).await {
            Ok(path) => {
                if let Err(e) = updater::install(&path) {
                    emit_ui("ui://toast", Value::String(e));
                }
            }
            Err(e) => emit_ui("ui://toast", Value::String(e)),
        }
    });
}

pub fn on_key(s: &mut AppState, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if ev.key == Key::Escape && s.open_select.is_some() {
        s.open_select = None;
        return true;
    }
    if ev.key == Key::Command('f') {
        s.screen = Screen::Search;
        return true;
    }
    let _ = tx;
    false
}

pub fn tray_icon() -> TrayIcon {
    const SIZE: u32 = 44;
    let center = (SIZE as f32 - 1.0) * 0.5;
    let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let ring = (1.5 - (dist - 12.5).abs()).clamp(0.0, 1.0);
            let dot = (4.5 - dist).clamp(0.0, 1.0);
            let alpha = (ring.max(dot) * 255.0).round() as u8;
            let idx = ((y * SIZE + x) * 4) as usize;
            pixels[idx..idx + 4].copy_from_slice(&[240, 240, 240, alpha]);
        }
    }
    TrayIcon::rgba(SIZE, SIZE, pixels).template(true)
}

pub fn tray_menu() -> Vec<TrayItem> {
    vec![
        TrayItem::action("show", "Show"),
        TrayItem::action("quit", "Quit"),
    ]
}

/// Wire the tray to window and process lifecycle.
pub fn attach_tray(app: &App<AppState>) {
    let handle = app.handle();
    let _ = Tray::new(
        TraySpec::new("tech.xyruscode.videoplucker", "Video Plucker", tray_icon())
            .tooltip("Video Plucker")
            .menu(tray_menu()),
        move |event| match event {
            TrayEvent::Activate { .. } => {
                compat::set_window_visible(true);
                handle.open();
            }
            TrayEvent::Menu(id) => match id.as_str() {
                "show" => {
                    compat::set_window_visible(true);
                    handle.open();
                }
                "quit" => {
                    shutdown();
                    handle.quit();
                }
                _ => {}
            },
            _ => {}
        },
    );
}

pub fn shutdown() {
    let jobs = compat::app_handle().state::<PluckState>();
    for job in jobs.0.lock().unwrap().values() {
        job.cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
        crate::pluck::kill_tree(job.pid);
    }
}

fn seen_terms(s: &mut AppState) {
    s.settings.terms_accepted = true;
    s.terms_open = false;
}

pub fn spec() -> AppSpec<AppState> {
    let mut spec = AppSpec::<AppState> {
        scenes: &[
            ("pluck", |s| seen_terms(s)),
            ("queue", |s| { seen_terms(s); s.screen = Screen::Queue; }),
            ("history", |s| { seen_terms(s); s.screen = Screen::History; }),
            ("saved", |s| { seen_terms(s); s.screen = Screen::Saved; }),
            ("search", |s| { seen_terms(s); s.screen = Screen::Search; }),
            ("settings", |s| { seen_terms(s); s.screen = Screen::Settings; }),
            ("terms", |s| s.terms_open = true),
            ("night-owl", |s| { seen_terms(s); s.settings.theme_mode = "dark".into(); }),
        ],
        ..AppSpec::new("Video Plucker", (920.0, 680.0), || unreachable!(), view)
    };
    spec.on_key = Some(on_key);
    spec.setup = Some(|_, waker| compat::set_waker(waker));
    spec.poll = Some(|s: &mut AppState, _, _tx: &mut Tx| {
        let mut changed = false;
        let mut rx = s.rx.take();
        if let Some(rx) = rx.as_mut() {
            while let Ok(event) = rx.try_recv() {
                apply_event(s, event);
                changed = true;
            }
        }
        s.rx = rx;
        changed
    });
    spec
}
