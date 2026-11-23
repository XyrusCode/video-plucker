//! AnimeXin extractor (animexin.dev) - WordPress donghua site.
//! Best-effort scraping; returns Unavailable on structural changes.

use async_trait::async_trait;
use scraper::{Html, Selector};

use super::{
    client, Episode, EpisodeRef, ExtractError, Extractor, Kind, SearchOpts, SearchResult, Season,
    SeriesDetail, StreamOption, USER_AGENT,
};

const BASE: &str = "https://animexin.dev";
const REFERER: &str = "https://animexin.dev/";

pub struct AnimeXin;
impl AnimeXin {
    async fn get(&self, url: &str) -> Result<String, ExtractError> {
        let resp = client().get(url).header("Referer", REFERER).send().await?;
        if !resp.status().is_success() { return Err(ExtractError::Network(format!("AnimeXin HTTP {}", resp.status()))); }
        resp.text().await.map_err(|e| ExtractError::Parse(e.to_string()))
    }
}

#[async_trait]
impl Extractor for AnimeXin {
    fn id(&self) -> &'static str { "animexin" }
    fn label(&self) -> &'static str { "AnimeXin" }

    async fn search(&self, query: &str, _opts: &SearchOpts) -> Result<Vec<SearchResult>, ExtractError> {
        let url = format!("{BASE}/?s={}", urlencode(query));
        let html = self.get(&url).await?;
        let doc = Html::parse_document(&html);
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        // WordPress anime themes: .bs .bsx a, article a, .listupd a
        for a in doc.select(&sel(".bsx a, .listupd .bs a, article.bs a, .tt a[rel='bookmark']")) {
            let href = a.value().attr("href").unwrap_or_default().to_string();
            if href.is_empty() || !href.contains("animexin") { continue; }
            if !seen.insert(href.clone()) { continue; }
            let title = a.value().attr("title").map(|s| s.to_string()).or_else(|| {
                let t = a.select(&sel(".tt, h2, .ntitle")).next().map(|n| n.text().collect::<String>().trim().to_string());
                t.filter(|s| !s.is_empty())
            }).unwrap_or_default();
            if title.is_empty() { continue; }
            let poster = a.select(&sel("img")).next().and_then(|i| i.value().attr("data-src").or(i.value().attr("src"))).map(|s| s.to_string());
            out.push(SearchResult { id: href, title, poster, kind: Kind::Series, site: "animexin".into(), year: None });
        }
        Ok(out)
    }

    async fn detail(&self, id: &str, _opts: &SearchOpts) -> Result<SeriesDetail, ExtractError> {
        let html = self.get(id).await?;
        let doc = Html::parse_document(&html);
        let title = doc.select(&sel("h1.entry-title, .entry-title, h1")).next().map(|n| n.text().collect::<String>().trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "(untitled)".into());
        let mut episodes = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for a in doc.select(&sel(".eplister a, .episodelist a, ul li a[href*='episode'], a[href*='-episode-']")) {
            let href = a.value().attr("href").unwrap_or_default().to_string();
            if href.is_empty() { continue; }
            if !seen.insert(href.clone()) { continue; }
            let text = a.text().collect::<String>();
            let number = extract_number(&text).or_else(|| extract_number(&href)).unwrap_or_else(|| (episodes.len()+1).to_string());
            episodes.push(Episode { number, id: href, title: Some(text.trim().to_string()).filter(|s| !s.is_empty()) });
        }
        episodes.sort_by(|a,b| a.number.parse::<f64>().unwrap_or(f64::MAX).partial_cmp(&b.number.parse::<f64>().unwrap_or(f64::MAX)).unwrap_or(std::cmp::Ordering::Equal));
        if episodes.is_empty() { return Err(ExtractError::Unavailable("AnimeXin: no episodes found (site structure may have changed)".into())); }
        let kind = if episodes.len() <= 1 { Kind::Movie } else { Kind::Series };
        Ok(SeriesDetail { id: id.to_string(), title, kind, seasons: vec![Season { number: 1, episodes }] })
    }

    async fn resolve_streams(&self, ep: &EpisodeRef) -> Result<Vec<StreamOption>, ExtractError> {
        let html = self.get(&ep.episode_id).await?;
        let mut urls = extract_media_urls(&html);
        // Also look for iframe/embed sources common in WP anime themes
        let doc = Html::parse_document(&html);
        for iframe in doc.select(&sel("iframe[src], iframe[data-src]")) {
            let src = iframe.value().attr("src").or(iframe.value().attr("data-src")).unwrap_or_default();
            if src.starts_with("http") && !urls.contains(&src.to_string()) { urls.push(src.to_string()); }
        }
        // try select.mirror options
        for opt in doc.select(&sel("select.mirror option[value], .mobius option[value]")) {
            let val = opt.value().attr("value").unwrap_or_default();
            if val.starts_with("http") && !urls.contains(&val.to_string()) { urls.push(val.to_string()); }
        }
        if urls.is_empty() { return Err(ExtractError::Unavailable("AnimeXin: no stream URL found (site structure may have changed)".into())); }
        Ok(urls.into_iter().map(|url| StreamOption { height: None, is_hls: url.contains(".m3u8"), url, referer: Some(REFERER.to_string()), headers: vec![("User-Agent".to_string(), USER_AGENT.to_string())] }).collect())
    }
}

fn sel(s: &str) -> Selector { Selector::parse(s).unwrap_or_else(|_| Selector::parse("_none_").unwrap()) }
fn urlencode(s: &str) -> String { let mut o=String::new(); for b in s.bytes(){ match b { b'A'..=b'Z'|b'a'..=b'z'|b'0'..=b'9'|b'-'|b'_'|b'.'|b'~'=>o.push(b as char), b' '=>o.push('+'), _=>o.push_str(&format!("%{b:02X}")) } } o }
fn extract_number(text: &str) -> Option<String> { let lower=text.to_lowercase(); for k in ["episode","ep-","ep "] { if let Some(p)=lower.find(k){ let n: String=text[p+k.len()..].chars().skip_while(|c|!c.is_ascii_digit()).take_while(|c|c.is_ascii_digit()||*c=='.').collect(); if !n.is_empty(){return Some(n);} } } None }
fn extract_media_urls(html: &str) -> Vec<String> { let mut out=Vec::new(); for needle in [".m3u8",".mp4"] { let mut i=0; while let Some(p)=html[i..].find(needle){ let p=i+p; let before=&html[..p]; let start=before.rfind(['"','\'','(', ' ','=']).map(|x|x+1).unwrap_or(0); let after=&html[p..]; let end=after.find(['"','\'','>', '<',' ',')']).map(|x|p+x).unwrap_or(html.len()); let u=&html[start..end]; if u.starts_with("http") && !out.contains(&u.to_string()){ out.push(u.to_string()); } i=p+needle.len(); } } out }
