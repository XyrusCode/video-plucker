//! LuciferDonghua extractor (follow-up).
//!
//! A WordPress-themed donghua streaming site with no public JSON API — search,
//! episode lists, and embed servers all need HTML scraping, and it embeds
//! third-party hosts (the lulacloud family among them) for playback. Wired as
//! an unavailable placeholder so the UI can show a "currently unavailable"
//! state; flip `available()` and fill these in once the scrapers are built.

use async_trait::async_trait;

use super::{
    EpisodeRef, ExtractError, Extractor, SearchOpts, SearchResult, SeriesDetail, StreamOption,
};

const UNAVAILABLE: &str = "LuciferDonghua support is coming in a future update.";

pub struct LuciferDonghua;

#[async_trait]
impl Extractor for LuciferDonghua {
    fn id(&self) -> &'static str {
        "luciferdonghua"
    }
    fn label(&self) -> &'static str {
        "LuciferDonghua"
    }
    fn available(&self) -> bool {
        false
    }

    async fn search(&self, _q: &str, _o: &SearchOpts) -> Result<Vec<SearchResult>, ExtractError> {
        Err(ExtractError::Unavailable(UNAVAILABLE.into()))
    }
    async fn detail(&self, _id: &str, _o: &SearchOpts) -> Result<SeriesDetail, ExtractError> {
        Err(ExtractError::Unavailable(UNAVAILABLE.into()))
    }
    async fn resolve_streams(&self, _ep: &EpisodeRef) -> Result<Vec<StreamOption>, ExtractError> {
        Err(ExtractError::Unavailable(UNAVAILABLE.into()))
    }
}
