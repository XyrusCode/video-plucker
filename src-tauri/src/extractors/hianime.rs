//! HiAnime extractor (hianime.fm). Separate entry from other HiAnime domains.

use async_trait::async_trait;
use scraper::{Html, Selector};
use serde_json::Value;

use super::{
    client, Episode, EpisodeRef, ExtractError, Extractor, Kind, SearchOpts, SearchResult, Season,
    SeriesDetail, StreamOption, USER_AGENT,
};

const BASE: &str = "https://hianime.fm";
const REFERER: &str = "https://hianime.fm/";

pub struct HiAnime;

impl HiAnime {
    async fn get(&self, url: &str) -> Result<String, ExtractError> {
        let resp = client().get(url).header("Referer", REFERER).send().await?;
        if !resp.status().is_success() {
            return Err(ExtractError::Network(format!("HiAnime HTTP {}", resp.status())));
        }
        resp.text().await.map_err(|e| ExtractError::Parse(e.to_string()))
    }

    async fn get_json(&self, url: &str) -> Result<Value, ExtractError> {
        let text = self.get(url).await?;
        serde_json::from_str(&text).map_err(|e| ExtractError::Parse(e.to_string()))
    }
}

#[async_trait]
impl Extractor for HiAnime {
    fn id(&self) -> &'static str { "hianime" }
    fn label(&self) -> &'static str { "HiAnime" }

    async fn search(&self, query: &str, _opts: &SearchOpts) -> Result<Vec<SearchResult>, ExtractError> {
        let url = format!("{BASE}/search?keyword={}", urlencode(query));
        let html = self.get(&url).await?;
        let results = {
            let doc = Html::parse_document(&html);
            let card = sel(".flw-item, .film_list-wrap .flw-item");
            let link = sel("a.film-poster-ahref, .film-name a, a.dynamic-name");
            let img = sel("img.film-poster-img, img");
            let mut out = Vec::new();
            for c in doc.select(&card) {
                let a = match c.select(&link).next() { Some(a) => a, None => continue };
                let href = a.value().attr("href").unwrap_or_default();
                let title = a.value().attr("title")
                    .map(|s| s.to_string())
                    .or_else(|| {
                        let t = a.text().collect::<String>().trim().to_string();
                        if t.is_empty() { None } else { Some(t) }
                    })
                    .unwrap_or_default();
                if title.is_empty() { continue; }
                let id = href.trim_start_matches('/').split('?').next().unwrap_or(href).to_string();
                let poster = c.select(&img).next().and_then(|i| i.value().attr("data-src").or(i.value().attr("src"))).map(|s| s.to_string());
                let type_text = c.text().collect::<String>().to_lowercase();
                let kind = if type_text.contains("movie") { Kind::Movie } else { Kind::Series };
                out.push(SearchResult { id, title, poster, kind, site: "hianime".into(), year: None });
            }
            out
        };
        Ok(results)
    }

    async fn detail(&self, id: &str, _opts: &SearchOpts) -> Result<SeriesDetail, ExtractError> {
        // id is like "watch/naruto-677" or "naruto-677"
        let slug = id.rsplit('/').next().unwrap_or(id);
        let num_id = slug.rsplit('-').next().unwrap_or(slug);
        let url = format!("{BASE}/ajax/v2/episode/list/{num_id}");
        let json = self.get_json(&url).await?;
        let html = json.get("html").and_then(|v| v.as_str()).unwrap_or("");
        let (title, mut episodes) = {
            let doc = Html::parse_document(html);
            let ep_sel = sel("a.ep-item, .ss-list a.ssl-item.ep-item");
            let mut eps = Vec::new();
            for a in doc.select(&ep_sel) {
                let data_id = a.value().attr("data-id").unwrap_or_default().to_string();
                let num = a.value().attr("data-number").unwrap_or_default().to_string();
                let ep_title = a.value().attr("title").map(|s| s.to_string());
                let number = if num.is_empty() { (eps.len()+1).to_string() } else { num };
                if data_id.is_empty() { continue; }
                eps.push(Episode { number, id: data_id, title: ep_title });
            }
            (slug.replace('-', " "), eps)
        };
        episodes.sort_by(|a,b| a.number.parse::<f64>().unwrap_or(f64::MAX).partial_cmp(&b.number.parse::<f64>().unwrap_or(f64::MAX)).unwrap_or(std::cmp::Ordering::Equal));
        let kind = if episodes.len() <= 1 { Kind::Movie } else { Kind::Series };
        // try to get better title from page
        let page_title = self.get(&format!("{BASE}/{id}")).await.ok().and_then(|h| {
            let d = Html::parse_document(&h);
            d.select(&sel("h2.film-name a, h2.film-name, .anisc-detail h2")).next().map(|n| n.text().collect::<String>().trim().to_string())
        }).unwrap_or(title);
        Ok(SeriesDetail { id: id.to_string(), title: page_title, kind, seasons: vec![Season { number: 1, episodes }] })
    }

    async fn resolve_streams(&self, ep: &EpisodeRef) -> Result<Vec<StreamOption>, ExtractError> {
        // HiAnime uses ajax endpoints for servers and sources. Domain/structure may change;
        // return a clear unavailable error if the flow can't be completed.
        let servers_url = format!("{BASE}/ajax/v2/episode/servers?episodeId={}", ep.episode_id);
        let servers_json = self.get_json(&servers_url).await?;
        let servers_html = servers_json.get("html").and_then(|v| v.as_str()).unwrap_or("");
        let doc = Html::parse_document(servers_html);
        let server_sel = sel(".server-item[data-type][data-id]");
        let preferred_type = if ep.translation == "dub" { "dub" } else { "sub" };
        let mut chosen: Option<(String, String)> = None; // (type, id)
        let mut fallback: Option<(String, String)> = None;
        for s in doc.select(&server_sel) {
            let t = s.value().attr("data-type").unwrap_or("").to_string();
            let sid = s.value().attr("data-id").unwrap_or("").to_string();
            if sid.is_empty() { continue; }
            if t == preferred_type { chosen = Some((t, sid)); break; }
            if fallback.is_none() { fallback = Some((t, sid)); }
        }
        let (_, sid) = chosen.or(fallback).ok_or_else(|| ExtractError::Unavailable("No HiAnime server found for episode".into()))?;
        let source_url = format!("{BASE}/ajax/v2/episode/sources?id={sid}");
        let src_json = self.get_json(&source_url).await?;
        let link = src_json.get("link").and_then(|v| v.as_str()).ok_or_else(|| ExtractError::Unavailable("HiAnime source link missing".into()))?;
        // The link is usually an embed (megacloud/vidcloud). We hand it to yt-dlp as embed; yt-dlp may not support it, so we also expose direct if present.
        if let Some(sources) = src_json.get("sources").and_then(|v| v.as_array()) {
            let mut out = Vec::new();
            for s in sources {
                if let Some(u) = s.get("file").and_then(|v| v.as_str()) {
                    out.push(StreamOption { height: None, url: u.to_string(), is_hls: u.contains(".m3u8"), referer: Some(REFERER.to_string()), headers: vec![("User-Agent".to_string(), USER_AGENT.to_string())] });
                }
            }
            if !out.is_empty() { return Ok(out); }
        }
        Ok(vec![StreamOption { height: None, url: link.to_string(), is_hls: link.contains(".m3u8"), referer: Some(REFERER.to_string()), headers: vec![("User-Agent".to_string(), USER_AGENT.to_string())] }])
    }
}

fn sel(s: &str) -> Selector { Selector::parse(s).unwrap_or_else(|_| Selector::parse("_none_").unwrap()) }
fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z'|b'a'..=b'z'|b'0'..=b'9'|b'-'|b'_'|b'.'|b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
