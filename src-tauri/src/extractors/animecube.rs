//! AnimeCube extractor (animecube.live) - donghua-focused site.
//! Best-effort HTML/API scraping; returns Unavailable on structural changes.

use async_trait::async_trait;
use scraper::{Html, Selector};
use serde_json::Value;

use super::{
    client, Episode, EpisodeRef, ExtractError, Extractor, Kind, SearchOpts, SearchResult, Season,
    SeriesDetail, StreamOption, USER_AGENT,
};

const BASE: &str = "https://animecube.live";
const REFERER: &str = "https://animecube.live/";

pub struct AnimeCube;
impl AnimeCube {
    async fn get(&self, url: &str) -> Result<String, ExtractError> {
        let resp = client().get(url).header("Referer", REFERER).send().await?;
        if !resp.status().is_success() { return Err(ExtractError::Network(format!("AnimeCube HTTP {}", resp.status()))); }
        resp.text().await.map_err(|e| ExtractError::Parse(e.to_string()))
    }
}

#[async_trait]
impl Extractor for AnimeCube {
    fn id(&self) -> &'static str { "animecube" }
    fn label(&self) -> &'static str { "AnimeCube" }

    async fn search(&self, query: &str, _opts: &SearchOpts) -> Result<Vec<SearchResult>, ExtractError> {
        let url = format!("{BASE}/search?q={}", urlencode(query));
        let html = self.get(&url).await?;
        let doc = Html::parse_document(&html);
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for a in doc.select(&sel("a[href*='/watch/'], a[href*='/series/'], a[href*='/donghua/']")) {
            let href = a.value().attr("href").unwrap_or_default();
            if href.is_empty() { continue; }
            let full = if href.starts_with("http") { href.to_string() } else { format!("{BASE}{}", if href.starts_with('/') { href.to_string() } else { format!("/{href}") }) };
            if !seen.insert(full.clone()) { continue; }
            let title = a.value().attr("title").map(|s| s.to_string()).or_else(|| {
                let t = a.text().collect::<String>().trim().to_string();
                if t.len() < 2 { None } else { Some(t) }
            }).unwrap_or_default();
            if title.is_empty() { continue; }
            let poster = a.select(&sel("img")).next().and_then(|i| i.value().attr("src").or(i.value().attr("data-src"))).map(|s| if s.starts_with("http") { s.to_string() } else { format!("{BASE}{s}") });
            out.push(SearchResult { id: full, title, poster, kind: Kind::Series, site: "animecube".into(), year: None });
        }
        if out.is_empty() {
            // Try to parse Next.js/embedded JSON data
            if let Some(data) = extract_next_data(&html) {
                if let Some(arr) = find_results_array(&data) {
                    for item in arr {
                        let title = item.get("title").or(item.get("name")).and_then(|v| v.as_str()).unwrap_or_default().to_string();
                        let slug = item.get("slug").or(item.get("id")).and_then(|v| v.as_str()).unwrap_or_default().to_string();
                        if title.is_empty() || slug.is_empty() { continue; }
                        let full = if slug.starts_with("http") { slug } else { format!("{BASE}/series/{slug}") };
                        let poster = item.get("poster").or(item.get("image")).and_then(|v| v.as_str()).map(|s| s.to_string());
                        out.push(SearchResult { id: full, title, poster, kind: Kind::Series, site: "animecube".into(), year: None });
                    }
                }
            }
        }
        Ok(out)
    }

    async fn detail(&self, id: &str, _opts: &SearchOpts) -> Result<SeriesDetail, ExtractError> {
        let html = self.get(id).await?;
        let doc = Html::parse_document(&html);
        let title = doc.select(&sel("h1, .title, [class*='title']")).next().map(|n| n.text().collect::<String>().trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "(untitled)".into());
        let mut episodes = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for a in doc.select(&sel("a[href*='/watch/'], a[href*='episode']")) {
            let href = a.value().attr("href").unwrap_or_default();
            if href.is_empty() { continue; }
            let full = if href.starts_with("http") { href.to_string() } else { format!("{BASE}{}", if href.starts_with('/') { href.to_string() } else { format!("/{href}") }) };
            if !seen.insert(full.clone()) { continue; }
            let text = a.text().collect::<String>();
            let number = extract_number(&text).unwrap_or_else(|| (episodes.len()+1).to_string());
            episodes.push(Episode { number, id: full, title: Some(text.trim().to_string()).filter(|s| !s.is_empty()) });
        }
        if episodes.is_empty() {
            if let Some(data) = extract_next_data(&html) {
                if let Some(arr) = find_episodes_array(&data) {
                    for ep in arr {
                        let num = ep.get("number").or(ep.get("episode")).and_then(|v| v.as_str().map(|s| s.to_string()).or_else(|| v.as_i64().map(|n| n.to_string()))).unwrap_or_else(|| (episodes.len()+1).to_string());
                        let slug = ep.get("slug").or(ep.get("id")).and_then(|v| v.as_str().map(|s| s.to_string()).or_else(|| v.as_i64().map(|n| n.to_string()))).unwrap_or_default();
                        let full = if slug.starts_with("http") { slug } else { format!("{BASE}/watch/{slug}") };
                        episodes.push(Episode { number: num, id: full, title: ep.get("title").and_then(|v| v.as_str()).map(|s| s.to_string()) });
                    }
                }
            }
        }
        episodes.sort_by(|a,b| a.number.parse::<f64>().unwrap_or(f64::MAX).partial_cmp(&b.number.parse::<f64>().unwrap_or(f64::MAX)).unwrap_or(std::cmp::Ordering::Equal));
        if episodes.is_empty() { return Err(ExtractError::Unavailable("AnimeCube: no episodes found (site structure may have changed)".into())); }
        let kind = if episodes.len() <= 1 { Kind::Movie } else { Kind::Series };
        Ok(SeriesDetail { id: id.to_string(), title, kind, seasons: vec![Season { number: 1, episodes }] })
    }

    async fn resolve_streams(&self, ep: &EpisodeRef) -> Result<Vec<StreamOption>, ExtractError> {
        let html = self.get(&ep.episode_id).await?;
        let mut urls = extract_media_urls(&html);
        if urls.is_empty() {
            if let Some(data) = extract_next_data(&html) {
                urls.extend(find_media_in_json(&data));
            }
        }
        if urls.is_empty() {
            return Err(ExtractError::Unavailable("AnimeCube: no stream URL found (site structure may have changed)".into()));
        }
        Ok(urls.into_iter().map(|url| StreamOption { height: None, is_hls: url.contains(".m3u8"), url, referer: Some(REFERER.to_string()), headers: vec![("User-Agent".to_string(), USER_AGENT.to_string())] }).collect())
    }
}

fn sel(s: &str) -> Selector { Selector::parse(s).unwrap_or_else(|_| Selector::parse("_none_").unwrap()) }
fn urlencode(s: &str) -> String { let mut o=String::new(); for b in s.bytes(){ match b { b'A'..=b'Z'|b'a'..=b'z'|b'0'..=b'9'|b'-'|b'_'|b'.'|b'~'=>o.push(b as char), b' '=>o.push('+'), _=>o.push_str(&format!("%{b:02X}")) } } o }
fn extract_number(text: &str) -> Option<String> { let lower=text.to_lowercase(); for k in ["episode","ep","eps"] { if let Some(p)=lower.find(k){ let n: String=text[p+k.len()..].chars().skip_while(|c|!c.is_ascii_digit()).take_while(|c|c.is_ascii_digit()||*c=='.').collect(); if !n.is_empty(){return Some(n);} } } let n:String=text.chars().filter(|c|c.is_ascii_digit()||*c=='.').collect(); if n.is_empty(){None}else{Some(n)} }
fn extract_next_data(html: &str) -> Option<Value> { let marker="__NEXT_DATA__"; let pos=html.find(marker)?; let after=&html[pos..]; let start=after.find('>')?+1; let rest=&after[start..]; let end=rest.find("</script>")?; serde_json::from_str(&rest[..end]).ok() }
fn find_results_array(v: &Value) -> Option<&Vec<Value>> { if let Some(a)=v.as_array(){ if a.first().and_then(|x| x.get("title").or(x.get("name"))).is_some() { return Some(a); } } if let Some(o)=v.as_object(){ for (_,val) in o { if let Some(r)=find_results_array(val){ return Some(r); } } } None }
fn find_episodes_array(v: &Value) -> Option<&Vec<Value>> { v.get("episodes").and_then(|e| e.as_array()).or_else(|| { if let Some(o)=v.as_object(){ for (_,val) in o { if let Some(r)=find_episodes_array(val){ return Some(r); } } } None }) }
fn find_media_in_json(v: &Value) -> Vec<String> { let mut out=Vec::new(); match v { Value::String(s)=>{ if s.contains(".m3u8")||s.contains(".mp4"){ if s.starts_with("http"){ out.push(s.clone()); } } }, Value::Array(a)=>{ for x in a { out.extend(find_media_in_json(x)); } }, Value::Object(o)=>{ for (_,x) in o { out.extend(find_media_in_json(x)); } }, _=>{} } out }
fn extract_media_urls(html: &str) -> Vec<String> { let mut out=Vec::new(); for needle in [".m3u8",".mp4"] { let mut i=0; while let Some(p)=html[i..].find(needle){ let p=i+p; let before=&html[..p]; let start=before.rfind(['"','\'','(', ' ','=']).map(|x|x+1).unwrap_or(0); let after=&html[p..]; let end=after.find(['"','\'','>', '<',' ',')']).map(|x|p+x).unwrap_or(html.len()); let u=&html[start..end]; if u.starts_with("http") && !out.contains(&u.to_string()){ out.push(u.to_string()); } i=p+needle.len(); } } out }
