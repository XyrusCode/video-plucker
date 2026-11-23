//! AnimeHeaven extractor (animeheaven.me).
//!
//! Public HTML scraping: search page, anime detail page, and episode gate.php
//! which returns a direct download/stream link. Stream links are served from
//! animeheaven's CDN with referer checks.

use async_trait::async_trait;
use scraper::{Html, Selector};

use super::{
    client, Episode, EpisodeRef, ExtractError, Extractor, Kind, SearchOpts, SearchResult, Season,
    SeriesDetail, StreamOption, USER_AGENT,
};

const BASE: &str = "https://animeheaven.me";
const REFERER: &str = "https://animeheaven.me/";

pub struct AnimeHeaven;

impl AnimeHeaven {
    async fn get(&self, url: &str) -> Result<String, ExtractError> {
        let resp = client()
            .get(url)
            .header("Referer", REFERER)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(ExtractError::Network(format!(
                "AnimeHeaven HTTP {}",
                resp.status()
            )));
        }
        resp.text()
            .await
            .map_err(|e| ExtractError::Parse(e.to_string()))
    }
}

#[async_trait]
impl Extractor for AnimeHeaven {
    fn id(&self) -> &'static str {
        "animeheaven"
    }
    fn label(&self) -> &'static str {
        "AnimeHeaven"
    }

    async fn search(
        &self,
        query: &str,
        _opts: &SearchOpts,
    ) -> Result<Vec<SearchResult>, ExtractError> {
        let url = format!("{BASE}/search.php?s={}", urlencode(query));
        let html = self.get(&url).await?;
        let results = {
            let doc = Html::parse_document(&html);
            // AnimeHeaven search results: .iepsdiv > a.c contains poster+title
            // Also sometimes .similarimg > a
            let card = sel(".iepsdiv a.c, .similarimg a.c, a.c[href*='anime.php']");
            let img = sel("img");
            let title_sel = sel(".iepst2, .tt, .similarname");

            let mut out = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for a in doc.select(&card) {
                let href = match a.value().attr("href") {
                    Some(h) => h.to_string(),
                    None => continue,
                };
                let full_url = if href.starts_with("http") {
                    href.clone()
                } else {
                    format!("{BASE}/{href}")
                };
                if !seen.insert(full_url.clone()) {
                    continue;
                }
                let title = a
                    .select(&title_sel)
                    .next()
                    .map(|t| t.text().collect::<String>().trim().to_string())
                    .or_else(|| a.value().attr("title").map(|s| s.to_string()))
                    .unwrap_or_default();
                if title.is_empty() {
                    continue;
                }
                let poster = a
                    .select(&img)
                    .next()
                    .and_then(|i| i.value().attr("src"))
                    .map(|s| absolutize(s));
                out.push(SearchResult {
                    id: full_url,
                    title,
                    poster,
                    kind: Kind::Series,
                    site: "animeheaven".into(),
                    year: None,
                });
            }
            out
        };
        Ok(results)
    }

    async fn detail(&self, id: &str, _opts: &SearchOpts) -> Result<SeriesDetail, ExtractError> {
        let html = self.get(id).await?;
        let (title, mut episodes) = {
            let doc = Html::parse_document(&html);
            let title = doc
                .select(&sel("h1, .infotitle, .c, [class*='title']"))
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| "(untitled)".into());

            // Episodes are in .linetitle2 > a or similar episode list links
            // Typical: <div class="line1"><a href="episode.php?..."><div class="watch2 bc">Episode N</div></a></div>
            let ep_links = sel("a[href*='episode.php'], a[href*='gate.php']");
            let mut eps = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for a in doc.select(&ep_links) {
                let href = match a.value().attr("href") {
                    Some(h) => h.to_string(),
                    None => continue,
                };
                let full = if href.starts_with("http") { href } else { format!("{BASE}/{href}") };
                if !seen.insert(full.clone()) { continue; }
                let text = a.text().collect::<String>();
                let number = extract_episode_number(&text).unwrap_or_else(|| (eps.len() + 1).to_string());
                eps.push(Episode {
                    number,
                    id: full,
                    title: Some(text.trim().to_string()).filter(|t| !t.is_empty()),
                });
            }
            (title, eps)
        };

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
        // episode_id is episode.php URL; fetch it to find gate.php or direct link
        let html = self.get(&ep.episode_id).await?;
        // Look for gate.php link or direct video source
        let gate_re = regex_lite_find(&html, r#"href=['"]([^'"]*gate\.php[^'"]*)['"']"#);
        let target = if let Some(g) = gate_re {
            if g.starts_with("http") { g } else { format!("{BASE}/{g}") }
        } else {
            ep.episode_id.clone()
        };

        let page = if target != ep.episode_id { self.get(&target).await? } else { html };
        // Find direct media links (mp4, m3u8) in the page
        let mut urls = Vec::new();
        for pat in [
            r#"href=['"]([^'"]+\.mp4[^'"]*)['"]"#,
            r#"src=['"]([^'"]+\.mp4[^'"]*)['"]"#,
            r#"href=['"]([^'"]+\.m3u8[^'"]*)['"]"#,
            r#"src=['"]([^'"]+\.m3u8[^'"]*)['"]"#,
            r#"(https?://[^\s"'<>]+\.mp4[^\s"'<>]*)"#,
            r#"(https?://[^\s"'<>]+\.m3u8[^\s"'<>]*)"#,
        ] {
            if let Some(u) = regex_lite_find(&page, pat) {
                let full = if u.starts_with("http") { u } else { format!("{BASE}/{u}") };
                if !urls.contains(&full) {
                    urls.push(full);
                }
            }
        }

        if urls.is_empty() {
            // Fallback: return the gate/episode page itself for yt-dlp to attempt
            urls.push(target);
        }

        Ok(urls.into_iter().map(|url| {
            let is_hls = url.contains(".m3u8");
            StreamOption {
                height: None,
                url,
                is_hls,
                referer: Some(REFERER.to_string()),
                headers: vec![("User-Agent".to_string(), USER_AGENT.to_string())],
            }
        }).collect())
    }
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).unwrap_or_else(|_| Selector::parse("_none_").unwrap())
}

fn absolutize(src: &str) -> String {
    if src.starts_with("http") { src.to_string() }
    else if src.starts_with("//") { format!("https:{src}") }
    else { format!("{BASE}/{}", src.trim_start_matches('/')) }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn extract_episode_number(text: &str) -> Option<String> {
    // Find "Episode 12", "Ep 12", or just a number
    let lower = text.to_lowercase();
    for key in ["episode", "ep"] {
        if let Some(pos) = lower.find(key) {
            let after = &text[pos + key.len()..];
            let num: String = after.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit() || *c == '.').collect();
            if !num.is_empty() { return Some(num); }
        }
    }
    let num: String = text.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    if num.is_empty() { None } else { Some(num) }
}

fn regex_lite_find(haystack: &str, pattern: &str) -> Option<String> {
    // Targeted simple extractors for the specific patterns used in resolve_streams.
    if pattern.contains("gate\\.php") {
        return find_attr_url(haystack, "gate.php");
    }
    if pattern.contains(r"\.mp4") {
        return find_media_url(haystack, ".mp4");
    }
    if pattern.contains(r"\.m3u8") {
        return find_media_url(haystack, ".m3u8");
    }
    None
}

fn find_attr_url(haystack: &str, needle: &str) -> Option<String> {
    let mut i = 0;
    while let Some(pos) = haystack[i..].find(needle) {
        let pos = i + pos;
        // walk back to opening quote
        let before = &haystack[..pos];
        let qpos = before.rfind(['"', '\'']);
        let start = qpos.map(|p| p + 1).unwrap_or(pos);
        let after = &haystack[pos..];
        let end = after.find(['"', '\'', '>', '<', ' ']).map(|p| pos + p).unwrap_or(haystack.len());
        let url = haystack[start..end].to_string();
        if url.contains(needle) { return Some(url); }
        i = pos + needle.len();
    }
    None
}

fn find_media_url(haystack: &str, ext: &str) -> Option<String> {
    let mut i = 0;
    while let Some(pos) = haystack[i..].find(ext) {
        let pos = i + pos;
        let before = &haystack[..pos];
        let start = before.rfind(['"', '\'', ' ', '(', '=']).map(|p| p + 1).unwrap_or(0);
        let after = &haystack[pos..];
        let end = after.find(['"', '\'', '>', '<', ' ', ')']).map(|p| pos + p).unwrap_or(haystack.len());
        let url = &haystack[start..end];
        if url.starts_with("http") || url.starts_with('/') || url.contains(ext) {
            return Some(url.to_string());
        }
        i = pos + ext.len();
    }
    None
}
