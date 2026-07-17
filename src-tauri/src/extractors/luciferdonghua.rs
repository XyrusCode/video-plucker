//! LuciferDonghua extractor.
//!
//! A WordPress "Dooplay"-themed donghua streaming site. Search and episode
//! lists are scraped from plain HTML. Each episode page has a server dropdown
//! (`select.mirror`) whose options load different embed hosts into `#pembed`;
//! most servers are hosts yt-dlp resolves natively (Rumble, OK.ru,
//! Dailymotion), so we pick a good server and hand yt-dlp the iframe URL.

use async_trait::async_trait;
use scraper::{Html, Selector};

use super::{
    client, Episode, EpisodeRef, ExtractError, Extractor, Kind, SearchOpts, SearchResult, Season,
    SeriesDetail, StreamOption, USER_AGENT,
};

const BASE: &str = "https://luciferdonghua.in";
const REFERER: &str = "https://luciferdonghua.in/";

pub struct LuciferDonghua;

impl LuciferDonghua {
    async fn get(&self, url: &str) -> Result<String, ExtractError> {
        let resp = client()
            .get(url)
            .header("Referer", REFERER)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(ExtractError::Network(format!(
                "LuciferDonghua HTTP {}",
                resp.status()
            )));
        }
        resp.text()
            .await
            .map_err(|e| ExtractError::Parse(e.to_string()))
    }
}

#[async_trait]
impl Extractor for LuciferDonghua {
    fn id(&self) -> &'static str {
        "luciferdonghua"
    }
    fn label(&self) -> &'static str {
        "LuciferDonghua"
    }

    async fn search(
        &self,
        query: &str,
        _opts: &SearchOpts,
    ) -> Result<Vec<SearchResult>, ExtractError> {
        let url = format!("{BASE}/?s={}", urlencode(query));
        let html = self.get(&url).await?;
        // scraper's Html/Selector are not Send; keep them in a sync scope so no
        // non-Send value is held across an await point.
        let results = {
            let doc = Html::parse_document(&html);
            let card = sel("article.bs");
            let link = sel("a[href]");
            let img = sel("img");
            let typez = sel(".typez");

            doc.select(&card)
                .filter_map(|c| {
                    let a = c.select(&link).next()?;
                    let href = a.value().attr("href")?.to_string();
                    let title = a
                        .value()
                        .attr("title")
                        .map(str::to_string)
                        .unwrap_or_default();
                    if title.is_empty() {
                        return None;
                    }
                    let poster = c
                        .select(&img)
                        .next()
                        .and_then(|i| i.value().attr("src"))
                        .map(strip_photon);
                    let type_txt = c
                        .select(&typez)
                        .next()
                        .map(|t| t.text().collect::<String>())
                        .unwrap_or_default();
                    let kind = if type_txt.eq_ignore_ascii_case("movie")
                        || title.contains("[Movie]")
                    {
                        Kind::Movie
                    } else {
                        Kind::Series
                    };
                    Some(SearchResult {
                        id: href,
                        title,
                        poster,
                        kind,
                        site: "luciferdonghua".into(),
                        year: None,
                    })
                })
                .collect::<Vec<_>>()
        };
        Ok(results)
    }

    async fn detail(&self, id: &str, _opts: &SearchOpts) -> Result<SeriesDetail, ExtractError> {
        // `id` is the /anime/<slug>/ page URL.
        let html = self.get(id).await?;
        let (title, mut episodes) = {
            let doc = Html::parse_document(&html);
            let title = doc
                .select(&sel("h1.entry-title"))
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_else(|| "(untitled)".into());

            let li = sel("div.eplister ul li a");
            let num = sel(".epl-num");
            let ttl = sel(".epl-title");
            let episodes: Vec<Episode> = doc
                .select(&li)
                .filter_map(|a| {
                    let href = a.value().attr("href")?.to_string();
                    let num_raw = a
                        .select(&num)
                        .next()
                        .map(|n| n.text().collect::<String>())
                        .unwrap_or_default();
                    let number = clean_epnum(&num_raw);
                    if number.is_empty() {
                        return None;
                    }
                    let ep_title = a
                        .select(&ttl)
                        .next()
                        .map(|t| t.text().collect::<String>().trim().to_string());
                    Some(Episode {
                        number,
                        id: href,
                        title: ep_title,
                    })
                })
                .collect();
            (title, episodes)
        };

        // The list is newest-first; present ascending by episode number.
        episodes.sort_by(|a, b| {
            let fa: f64 = a.number.parse().unwrap_or(f64::MAX);
            let fb: f64 = b.number.parse().unwrap_or(f64::MAX);
            fa.partial_cmp(&fb).unwrap_or(std::cmp::Ordering::Equal)
        });

        let kind = if episodes.len() <= 1 { Kind::Movie } else { Kind::Series };
        Ok(SeriesDetail {
            id: id.to_string(),
            title,
            kind,
            seasons: vec![Season { number: 1, episodes }],
        })
    }

    async fn resolve_streams(&self, ep: &EpisodeRef) -> Result<Vec<StreamOption>, ExtractError> {
        // `episode_id` is the episode page URL.
        let html = self.get(&ep.episode_id).await?;

        // Read the server dropdown: (rank, /v/N/ value URL). Prefer hosts
        // yt-dlp resolves natively; the first iframe on the base page is
        // server 1, so an empty server list still falls back to it.
        let mut servers: Vec<(u8, String)> = {
            let doc = Html::parse_document(&html);
            doc.select(&sel("select.mirror option"))
                .filter_map(|o| {
                    let value = o.value().attr("value")?.to_string();
                    if value.is_empty() {
                        return None;
                    }
                    let label = o.text().collect::<String>().to_uppercase();
                    Some((host_rank(&label), value))
                })
                .collect()
        };
        servers.sort_by_key(|(rank, _)| *rank);

        // Try servers best-first: fetch the /v/N/ page and read its #pembed
        // iframe. Fall back to the iframe already on the base page.
        let mut iframe = None;
        for (rank, value) in servers.iter() {
            if *rank == u8::MAX {
                continue; // hostile/unsupported host (StreamSB, self-hosted SPA)
            }
            let page = match self.get(value).await {
                Ok(p) => p,
                Err(_) => continue,
            };
            if let Some(src) = pembed_iframe(&page) {
                iframe = Some(src);
                break;
            }
        }
        let iframe = iframe
            .or_else(|| pembed_iframe(&html))
            .ok_or_else(|| ExtractError::Unavailable("no playable server for this episode".into()))?;

        let url = normalize_embed(&iframe);
        Ok(vec![StreamOption {
            // An embed page URL for yt-dlp to extract — height unknown, so
            // yt-dlp's own quality filter selects the variant.
            height: None,
            url,
            is_hls: false,
            referer: Some(REFERER.to_string()),
            headers: vec![("User-Agent".to_string(), USER_AGENT.to_string())],
        }])
    }
}

/// Parse a compiled selector. The selector strings are all static and valid, so
/// this never fails in practice; on a bad selector we return an empty match set
/// via a universal selector rather than panicking.
fn sel(s: &str) -> Selector {
    Selector::parse(s).unwrap_or_else(|_| Selector::parse("_none_").unwrap())
}

/// Rank an embed host from its option label. Lower is better; `u8::MAX` marks
/// hosts we deliberately skip (unextractable/hostile).
fn host_rank(label_upper: &str) -> u8 {
    if label_upper.contains("RUMBLE") {
        0
    } else if label_upper.contains("OK.RU") || label_upper.contains("OKRU") {
        1
    } else if label_upper.contains("DAILYMOTION") {
        2
    } else if label_upper.contains("VID HIDE") || label_upper.contains("VIDHIDE") {
        3
    } else if label_upper.contains("STREAM SB") || label_upper.contains("STREAMSB") {
        u8::MAX
    } else {
        4
    }
}

/// Find the iframe src inside the `#pembed` block. The markup uses uppercase
/// `<IFRAME SRC="...">`, so match case-insensitively.
fn pembed_iframe(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let src = doc
        .select(&sel("#pembed iframe"))
        .next()
        .and_then(|f| f.value().attr("src"))
        .map(str::to_string);
    src.filter(|s| !s.is_empty())
}

/// Normalize an embed iframe URL into something yt-dlp resolves cleanly:
/// protocol-relative → https, and geo.dailymotion player → canonical video URL.
fn normalize_embed(src: &str) -> String {
    let src = if let Some(rest) = src.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        src.to_string()
    };
    if src.contains("dailymotion.com") {
        if let Some(id) = url_param(&src, "video") {
            return format!("https://www.dailymotion.com/video/{id}");
        }
    }
    src
}

/// Extract a query-string parameter value from a URL.
fn url_param(url: &str, key: &str) -> Option<String> {
    let q = url.split('?').nth(1)?;
    for pair in q.split('&') {
        let mut it = pair.splitn(2, '=');
        if it.next() == Some(key) {
            return it.next().map(|v| v.split(['&', '#']).next().unwrap_or(v).to_string());
        }
    }
    None
}

/// Keep the leading numeric part of an episode label ("162 [4K]" -> "162",
/// "09 - 12" -> "09"). Empty if the label has no leading number.
fn clean_epnum(raw: &str) -> String {
    raw.trim()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect()
}

/// Strip the Jetpack/Photon `?resize=...` query to get the full-res poster.
fn strip_photon(src: &str) -> String {
    src.split('?').next().unwrap_or(src).to_string()
}

/// Minimal percent-encoding for a search query in a URL query string.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
