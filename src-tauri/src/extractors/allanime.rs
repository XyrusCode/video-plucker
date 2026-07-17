//! AllAnime extractor.
//!
//! AllAnime exposes a GraphQL API (the same one `ani-cli` drives). Queries go
//! out as GET requests with url-encoded `variables`/`query` params. Episode
//! sources arrive XOR-obfuscated (see [`super::decode`]) and point at a
//! provider "clock" endpoint that returns the final `.m3u8`/`.mp4` links.

use async_trait::async_trait;
use serde_json::{json, Value};

use super::decode::allanime_source;
use super::{
    client, Episode, EpisodeRef, ExtractError, Extractor, Kind, SearchOpts, SearchResult, Season,
    SeriesDetail, StreamOption, USER_AGENT,
};

// These four constants are the whole "contract" with AllAnime. If the site
// moves, this is where the fix lands.
const API: &str = "https://api.allanime.day/api";
const CLOCK_BASE: &str = "https://api.allanime.day";
const REFERER: &str = "https://allanime.to";

const SEARCH_GQL: &str = "query($search: SearchInput $limit: Int $page: Int $translationType: VaildTranslationTypeEnumType $countryOrigin: VaildCountryOriginEnumType) { shows(search: $search limit: $limit page: $page translationType: $translationType countryOrigin: $countryOrigin) { edges { _id name thumbnail availableEpisodes airedStart __typename } } }";
const EPISODES_GQL: &str = "query($showId: String!) { show(_id: $showId) { _id name thumbnail availableEpisodesDetail } }";
const SOURCES_GQL: &str = "query($showId: String! $translationType: VaildTranslationTypeEnumType! $episodeString: String!) { episode(showId: $showId translationType: $translationType episodeString: $episodeString) { episodeString sourceUrls } }";

/// Providers we know how to resolve, best first. Each `sourceUrls` entry names
/// its provider in `sourceName`; anything not listed here is skipped.
const PROVIDER_PREFERENCE: &[&str] = &[
    "Default", "Sak", "S-mp4", "Luf-mp4", "Kir", "Yt-mp4", "Vid-mp4", "Ok",
];

pub struct AllAnime;

impl AllAnime {
    async fn gql(&self, variables: Value, query: &str) -> Result<Value, ExtractError> {
        let resp = client()
            .get(API)
            .header("Referer", REFERER)
            .query(&[
                ("variables", variables.to_string()),
                ("query", query.to_string()),
            ])
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(ExtractError::Network(format!("AllAnime HTTP {}", resp.status())));
        }
        let body: Value = resp
            .json()
            .await
            .map_err(|e| ExtractError::Parse(e.to_string()))?;
        Ok(body)
    }
}

#[async_trait]
impl Extractor for AllAnime {
    fn id(&self) -> &'static str {
        "allanime"
    }
    fn label(&self) -> &'static str {
        "AllAnime"
    }

    async fn search(
        &self,
        query: &str,
        opts: &SearchOpts,
    ) -> Result<Vec<SearchResult>, ExtractError> {
        let variables = json!({
            "search": { "allowAdult": false, "allowUnknown": false, "query": query },
            "limit": 40,
            "page": 1,
            "translationType": opts.translation,
            "countryOrigin": "ALL",
        });
        let body = self.gql(variables, SEARCH_GQL).await?;
        let edges = body
            .pointer("/data/shows/edges")
            .and_then(Value::as_array)
            .ok_or_else(|| ExtractError::Parse("no shows in response".into()))?;

        let results = edges
            .iter()
            .filter_map(|e| {
                let id = e.get("_id")?.as_str()?.to_string();
                let title = e.get("name")?.as_str()?.to_string();
                // availableEpisodes.{sub,dub}: a single episode => treat as movie.
                let sub_count = e
                    .pointer("/availableEpisodes/sub")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let kind = if sub_count <= 1 { Kind::Movie } else { Kind::Series };
                let year = e
                    .pointer("/airedStart/year")
                    .and_then(Value::as_i64)
                    .map(|y| y as i32);
                Some(SearchResult {
                    id,
                    title,
                    poster: e.get("thumbnail").and_then(Value::as_str).map(String::from),
                    kind,
                    site: "allanime".into(),
                    year,
                })
            })
            .collect();
        Ok(results)
    }

    async fn detail(&self, id: &str, opts: &SearchOpts) -> Result<SeriesDetail, ExtractError> {
        let body = self.gql(json!({ "showId": id }), EPISODES_GQL).await?;
        let show = body
            .pointer("/data/show")
            .ok_or_else(|| ExtractError::Parse("show not found".into()))?;
        let title = show
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("(untitled)")
            .to_string();

        let mut eps: Vec<String> = show
            .pointer(&format!("/availableEpisodesDetail/{}", opts.translation))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // The API returns episodes newest-first and as strings; sort ascending
        // numerically so the UI lists 1,2,3,… (decimals like "12.5" included).
        eps.sort_by(|a, b| {
            let fa: f64 = a.parse().unwrap_or(f64::MAX);
            let fb: f64 = b.parse().unwrap_or(f64::MAX);
            fa.partial_cmp(&fb).unwrap_or(std::cmp::Ordering::Equal)
        });

        let kind = if eps.len() <= 1 { Kind::Movie } else { Kind::Series };
        let episodes = eps
            .into_iter()
            .map(|number| Episode {
                id: number.clone(),
                number,
                title: None,
            })
            .collect();

        Ok(SeriesDetail {
            id: id.to_string(),
            title,
            kind,
            // AllAnime has no season concept — always exactly one season.
            seasons: vec![Season { number: 1, episodes }],
        })
    }

    async fn resolve_streams(&self, ep: &EpisodeRef) -> Result<Vec<StreamOption>, ExtractError> {
        let variables = json!({
            "showId": ep.show_id,
            "translationType": ep.translation,
            "episodeString": ep.episode,
        });
        let body = self.gql(variables, SOURCES_GQL).await?;
        let sources = body
            .pointer("/data/episode/sourceUrls")
            .and_then(Value::as_array)
            .ok_or_else(|| ExtractError::Unavailable("no sources for this episode".into()))?;

        // Order candidates by our provider preference, then priority.
        let mut candidates: Vec<&Value> = sources
            .iter()
            .filter(|s| {
                s.get("sourceName")
                    .and_then(Value::as_str)
                    .map(|n| PROVIDER_PREFERENCE.contains(&n))
                    .unwrap_or(false)
            })
            .collect();
        candidates.sort_by_key(|s| {
            s.get("sourceName")
                .and_then(Value::as_str)
                .and_then(|n| PROVIDER_PREFERENCE.iter().position(|p| *p == n))
                .unwrap_or(usize::MAX)
        });

        let mut options: Vec<StreamOption> = Vec::new();
        // Try providers in order; gather links from the first few that work so
        // the quality menu has real choices without fetching every provider.
        for src in candidates.iter().take(4) {
            let raw = match src.get("sourceUrl").and_then(Value::as_str) {
                Some(u) => u,
                None => continue,
            };
            let decoded = match allanime_source(raw) {
                Some(d) => d,
                None => continue,
            };
            if let Ok(links) = self.fetch_clock(&decoded).await {
                options.extend(links);
            }
            if options.iter().any(|o| o.height.is_some()) {
                break;
            }
        }

        if options.is_empty() {
            return Err(ExtractError::Unavailable(
                "no playable stream found for this episode".into(),
            ));
        }
        // Dedupe by URL, keep the highest resolution first.
        options.sort_by(|a, b| b.height.unwrap_or(0).cmp(&a.height.unwrap_or(0)));
        options.dedup_by(|a, b| a.url == b.url);
        Ok(options)
    }
}

impl AllAnime {
    /// Turn a decoded provider path (`/apivtwo/clock?id=...`) into concrete
    /// stream links via the provider's `clock.json` endpoint.
    async fn fetch_clock(&self, decoded_path: &str) -> Result<Vec<StreamOption>, ExtractError> {
        let path = if decoded_path.starts_with("http") {
            decoded_path.to_string()
        } else {
            format!("{CLOCK_BASE}{}", decoded_path.replacen("/clock?", "/clock.json?", 1))
        };
        let resp = client().get(&path).header("Referer", REFERER).send().await?;
        if !resp.status().is_success() {
            return Err(ExtractError::Network(format!("clock HTTP {}", resp.status())));
        }
        let body: Value = resp
            .json()
            .await
            .map_err(|e| ExtractError::Parse(e.to_string()))?;
        let links = body
            .get("links")
            .and_then(Value::as_array)
            .ok_or_else(|| ExtractError::Parse("clock response had no links".into()))?;

        let headers = vec![("User-Agent".to_string(), USER_AGENT.to_string())];
        let out = links
            .iter()
            .filter_map(|l| {
                let url = l.get("link").and_then(Value::as_str)?.to_string();
                // Skip provider templates we don't expand (e.g. wixmp {resolution}).
                if url.contains('{') {
                    return None;
                }
                let is_hls = l.get("hls").and_then(Value::as_bool).unwrap_or(url.contains(".m3u8"));
                let height = l
                    .get("resolutionStr")
                    .and_then(Value::as_str)
                    .and_then(parse_height);
                Some(StreamOption {
                    height,
                    url,
                    is_hls,
                    referer: Some(REFERER.to_string()),
                    headers: headers.clone(),
                })
            })
            .collect();
        Ok(out)
    }
}

/// Pull a pixel height out of strings like "1080", "1080p", "720P", "auto".
fn parse_height(s: &str) -> Option<u32> {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}
