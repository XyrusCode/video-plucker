//! LulaCloud resolver (follow-up).
//!
//! LulaCloud (lulacloudx / lulastream family) is an embed/stream HOST, not a
//! searchable catalog — other sites (e.g. LuciferDonghua) embed it for
//! playback. It therefore never appears in the search dropdown
//! (`searchable() == false`); it exists as a resolver another extractor can
//! call with an embed URL to obtain the underlying `.m3u8`. Not yet wired.

use async_trait::async_trait;

use super::{
    EpisodeRef, ExtractError, Extractor, SearchOpts, SearchResult, SeriesDetail, StreamOption,
};

pub struct LulaCloud;

#[async_trait]
impl Extractor for LulaCloud {
    fn id(&self) -> &'static str {
        "lulacloud"
    }
    fn label(&self) -> &'static str {
        "LulaCloud"
    }
    fn searchable(&self) -> bool {
        false
    }
    fn available(&self) -> bool {
        false
    }

    async fn search(&self, _q: &str, _o: &SearchOpts) -> Result<Vec<SearchResult>, ExtractError> {
        Err(ExtractError::Unavailable("LulaCloud is a stream host, not a searchable site.".into()))
    }
    async fn detail(&self, _id: &str, _o: &SearchOpts) -> Result<SeriesDetail, ExtractError> {
        Err(ExtractError::Unavailable("LulaCloud is a stream host, not a searchable site.".into()))
    }
    async fn resolve_streams(&self, _ep: &EpisodeRef) -> Result<Vec<StreamOption>, ExtractError> {
        Err(ExtractError::Unavailable("LulaCloud resolver is not implemented yet.".into()))
    }
}
