//! Streaming-site extractors.
//!
//! Each supported site implements [`Extractor`] to (1) search a catalog,
//! (2) list a title's seasons/episodes, and (3) resolve a chosen episode down
//! to a concrete stream URL (`.m3u8`/`.mp4`) that the existing yt-dlp pipeline
//! can download. Every extractor is isolated behind the registry so a break in
//! one site never takes down the others.

pub mod allanime;
pub mod decode;
pub mod luciferdonghua;
pub mod lulacloud;

use std::sync::OnceLock;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Firefox UA reused for every extractor request AND handed to yt-dlp, so the
/// resolved stream is fetched with the same identity that produced the token.
pub const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:150.0) Gecko/20100101 Firefox/150.0";

/// Whether a title is a single film or an episodic series. Drives the UI:
/// movies download immediately, series show the season/episode picker.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Movie,
    Series,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub poster: Option<String>,
    pub kind: Kind,
    pub site: String,
    pub year: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    /// Episode label as the site knows it ("1", "2", "12.5"). Passed back
    /// verbatim to `resolve_streams` — never reformat it.
    pub number: String,
    pub id: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Season {
    pub number: u32,
    pub episodes: Vec<Episode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesDetail {
    pub id: String,
    pub title: String,
    pub kind: Kind,
    /// Sites without a season concept (e.g. AllAnime) emit exactly one season.
    pub seasons: Vec<Season>,
}

/// Identifies one episode to resolve. `show_id`/`episode` come straight from a
/// [`SearchResult`]/[`Episode`]; `translation` is "sub" or "dub". `episode_id`
/// is the site's own per-episode key (e.g. the episode page URL) for sites
/// where the display number alone can't locate the episode; sites keyed by
/// number (AllAnime) simply mirror `episode` here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeRef {
    pub site: String,
    pub show_id: String,
    pub episode: String,
    pub episode_id: String,
    pub translation: String,
}

/// One playable stream variant. `height` is `None` when the URL is a master
/// playlist that itself lists resolutions (let yt-dlp pick with a height
/// filter); `Some` when it's already a single fixed-resolution stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamOption {
    pub height: Option<u32>,
    pub url: String,
    pub is_hls: bool,
    pub referer: Option<String>,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOpts {
    /// "sub" or "dub".
    pub translation: String,
}

/// Advertised to the frontend so the site dropdown can list what's available
/// and grey out sites that are known-broken without breaking the others.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteInfo {
    pub id: String,
    pub label: String,
    pub available: bool,
}

/// A localized failure. `Unavailable` is used both for "this site isn't wired
/// up yet" and "the site changed and the scraper broke" — the UI shows a
/// "currently unavailable" state rather than crashing.
#[derive(Debug, Clone)]
pub enum ExtractError {
    Network(String),
    Parse(String),
    Unavailable(String),
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtractError::Network(m) => write!(f, "network error: {m}"),
            ExtractError::Parse(m) => write!(f, "could not read the site's response: {m}"),
            ExtractError::Unavailable(m) => write!(f, "{m}"),
        }
    }
}

impl From<reqwest::Error> for ExtractError {
    fn from(e: reqwest::Error) -> Self {
        ExtractError::Network(e.to_string())
    }
}

#[async_trait]
pub trait Extractor: Send + Sync {
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    /// A site the user can type a query into. Pure stream hosts (lulacloud)
    /// return false and never appear in the site dropdown.
    fn searchable(&self) -> bool {
        true
    }
    /// Whether the site is wired up and believed working. Searchable-but-
    /// unavailable sites show greyed in the UI.
    fn available(&self) -> bool {
        true
    }

    async fn search(
        &self,
        query: &str,
        opts: &SearchOpts,
    ) -> Result<Vec<SearchResult>, ExtractError>;

    async fn detail(&self, id: &str, opts: &SearchOpts) -> Result<SeriesDetail, ExtractError>;

    async fn resolve_streams(&self, ep: &EpisodeRef) -> Result<Vec<StreamOption>, ExtractError>;
}

/// Process-wide reqwest client (connection keep-alive, shared across all
/// extractor calls). Built once with the era-correct UA.
pub fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .expect("failed to build HTTP client")
    })
}

/// Look up an extractor (searchable sites AND resolver-only hosts) by id.
pub fn get(site: &str) -> Option<Box<dyn Extractor>> {
    match site {
        "allanime" => Some(Box::new(allanime::AllAnime)),
        "luciferdonghua" => Some(Box::new(luciferdonghua::LuciferDonghua)),
        "lulacloud" => Some(Box::new(lulacloud::LulaCloud)),
        _ => None,
    }
}

/// The sites the frontend offers in its search dropdown, in display order.
pub fn searchable_sites() -> Vec<SiteInfo> {
    ["allanime", "luciferdonghua"]
        .iter()
        .filter_map(|id| get(id))
        .filter(|e| e.searchable())
        .map(|e| SiteInfo {
            id: e.id().to_string(),
            label: e.label().to_string(),
            available: e.available(),
        })
        .collect()
}
