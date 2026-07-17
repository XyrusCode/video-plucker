//! AllAnime extractor.
//!
//! AllAnime exposes a GraphQL API (the same one `ani-cli` drives). Search and
//! episode-list are plain JSON via POST; episode sources arrive either plain or
//! wrapped in an AES-256-CTR-encrypted `tobeparsed` field (see
//! [`super::decode`]). Each `sourceUrl` is additionally XOR-obfuscated and
//! points at a provider "clock" endpoint returning the final `.m3u8`/`.mp4`.

use async_trait::async_trait;
use serde_json::{json, Value};

use super::decode::{allanime_source, allanime_tobeparsed};
use super::{
    client, Episode, EpisodeRef, ExtractError, Extractor, Kind, SearchOpts, SearchResult, Season,
    SeriesDetail, StreamOption, USER_AGENT,
};

// These constants are the whole "contract" with AllAnime; if the site moves,
// this is where the fix lands. Referer AND Origin must both be youtu-chan.com —
// the API returns 403 Forbidden otherwise.
const API: &str = "https://api.allanime.day/api";
const CLOCK_BASE: &str = "https://allanime.day";
const REFERER: &str = "https://youtu-chan.com";

const SEARCH_GQL: &str = "query( $search: SearchInput $limit: Int $page: Int $translationType: VaildTranslationTypeEnumType $countryOrigin: VaildCountryOriginEnumType ) { shows( search: $search limit: $limit page: $page translationType: $translationType countryOrigin: $countryOrigin ) { edges { _id name thumbnail availableEpisodes airedStart __typename } } }";
const EPISODES_GQL: &str = "query ($showId: String!) { show( _id: $showId ) { _id name thumbnail availableEpisodesDetail } }";
const SOURCES_GQL: &str = "query ($showId: String!, $translationType: VaildTranslationTypeEnumType!, $episodeString: String!) { episode( showId: $showId translationType: $translationType episodeString: $episodeString ) { episodeString sourceUrls } }";

/// Providers we know how to resolve, best first. Each `sourceUrls` entry names
/// its provider in `sourceName`; anything not listed here is skipped.
const PROVIDER_PREFERENCE: &[&str] = &[
    "Default", "Sak", "S-mp4", "Luf-mp4", "Kir", "Yt-mp4", "Vid-mp4", "Ok",
];

pub struct AllAnime;

impl AllAnime {
    /// POST a GraphQL body and return the raw response text.
    async fn post(&self, body: Value) -> Result<String, ExtractError> {
        let resp = client()
            .post(API)
            .header("Referer", REFERER)
            .header("Origin", REFERER)
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(ExtractError::Network(format!(
                "AllAnime HTTP {}",
                resp.status()
            )));
        }
        resp.text()
            .await
            .map_err(|e| ExtractError::Parse(e.to_string()))
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
        let body = json!({
            "variables": {
                "search": { "allowAdult": false, "allowUnknown": false, "query": query },
                "limit": 40,
                "page": 1,
                "translationType": opts.translation,
                "countryOrigin": "ALL",
            },
            "query": SEARCH_GQL,
        });
        let text = self.post(body).await?;
        let v: Value =
            serde_json::from_str(&text).map_err(|e| ExtractError::Parse(e.to_string()))?;
        let edges = v
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
        let body = json!({ "variables": { "showId": id }, "query": EPISODES_GQL });
        let text = self.post(body).await?;
        let v: Value =
            serde_json::from_str(&text).map_err(|e| ExtractError::Parse(e.to_string()))?;
        let show = v
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
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        // The API returns episodes newest-first as strings; sort ascending
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
        let body = json!({
            "variables": {
                "showId": ep.show_id,
                "translationType": ep.translation,
                "episodeString": ep.episode,
            },
            "query": SOURCES_GQL,
        });
        let text = self.post(body).await?;

        // Sources are plain JSON or an encrypted `tobeparsed` blob. Decrypt if
        // present, then pull every {sourceUrl, sourceName} out of the JSON.
        let tobeparsed = find_tobeparsed(&text).map(str::to_string);
        let sources_json = match tobeparsed {
            Some(blob) => allanime_tobeparsed(&blob)
                .ok_or_else(|| ExtractError::Parse("could not decrypt sources".into()))?,
            None => text,
        };
        let v: Value = serde_json::from_str(&sources_json)
            .map_err(|e| ExtractError::Parse(e.to_string()))?;
        let mut sources: Vec<(String, String)> = Vec::new();
        collect_sources(&v, &mut sources);
        if sources.is_empty() {
            return Err(ExtractError::Unavailable(
                "no sources for this episode".into(),
            ));
        }

        // Keep only providers we can resolve, ordered by preference.
        sources.retain(|(_, name)| PROVIDER_PREFERENCE.contains(&name.as_str()));
        sources.sort_by_key(|(_, name)| {
            PROVIDER_PREFERENCE
                .iter()
                .position(|p| *p == name.as_str())
                .unwrap_or(usize::MAX)
        });

        let mut options: Vec<StreamOption> = Vec::new();
        for (source_url, _) in sources.iter().take(4) {
            let decoded = match allanime_source(source_url) {
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
            format!("{CLOCK_BASE}{}", decoded_path.replacen("/clock", "/clock.json", 1))
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
                let is_hls =
                    l.get("hls").and_then(Value::as_bool).unwrap_or(url.contains(".m3u8"));
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

/// Extract the base64 value of a `"tobeparsed":"..."` field, if present. The
/// value is base64 (contains no `"`), so scanning to the next quote is safe.
fn find_tobeparsed(text: &str) -> Option<&str> {
    let key = "\"tobeparsed\":\"";
    let start = text.find(key)? + key.len();
    let rest = &text[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// Recursively collect every `{sourceUrl, sourceName}` pair, regardless of how
/// the (plain or decrypted) response is nested.
fn collect_sources(v: &Value, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(map) => {
            if let Some(Value::String(su)) = map.get("sourceUrl") {
                let name = map
                    .get("sourceName")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                out.push((su.clone(), name));
            }
            for child in map.values() {
                collect_sources(child, out);
            }
        }
        Value::Array(arr) => {
            for child in arr {
                collect_sources(child, out);
            }
        }
        _ => {}
    }
}

/// Pull a pixel height out of strings like "1080", "1080p", "720P", "auto".
fn parse_height(s: &str) -> Option<u32> {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}
