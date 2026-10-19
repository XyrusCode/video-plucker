//! Saved videos, read from each site's own web endpoints.
//!
//! Ported from the mobile app's `BookmarksRepositoryImpl` and
//! `BookmarksParser`. The mobile app reads the same four sites with the
//! cookies the user imported for them; this reads them with the
//! `cookies.txt` files the desktop app already keeps for yt-dlp.
//!
//! Read-only by design. Writing back means calling each site's private
//! mutation endpoint, and every one of them answers HTTP 200 with no error
//! when it declines to act, so a removal cannot be confirmed without
//! re-reading the whole list.
//!
//! One site refusing does not hide the others: each read is independent and
//! its failure comes back as a message for that site.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::redirect::Policy;
use scraper::{Html, Selector};
use serde::Serialize;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use tauri::AppHandle;

use crate::commands::settings_cookie_file;
use crate::pluck;

/// The sites the Saved view reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Site {
    YouTube,
    TikTok,
    Twitter,
    Vk,
}

impl Site {
    pub fn key(self) -> &'static str {
        match self {
            Site::YouTube => "youtube",
            Site::TikTok => "tiktok",
            Site::Twitter => "twitter",
            Site::Vk => "vk",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Site::YouTube => "YouTube",
            Site::TikTok => "TikTok",
            Site::Twitter => "X",
            Site::Vk => "VK",
        }
    }

    /// The URL a cookie profile is matched against. The desktop names a
    /// profile after a substring of the URL it serves, so the site's own
    /// address is what finds its cookies.
    fn cookie_probe(self) -> &'static str {
        match self {
            Site::YouTube => "https://www.youtube.com/",
            Site::TikTok => "https://www.tiktok.com/",
            Site::Twitter => "https://x.com/",
            Site::Vk => "https://vk.com/",
        }
    }

    pub fn from_key(key: &str) -> Option<Site> {
        match key.trim().to_lowercase().as_str() {
            "youtube" => Some(Site::YouTube),
            "tiktok" => Some(Site::TikTok),
            "twitter" | "x" => Some(Site::Twitter),
            "vk" => Some(Site::Vk),
            _ => None,
        }
    }

    fn all() -> [Site; 4] {
        [Site::YouTube, Site::TikTok, Site::Twitter, Site::Vk]
    }
}

/// One saved video, normalised across the sites.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedItem {
    /// The item's id on the site: a video id, a tweet id, a VK object id.
    pub id: String,
    pub site: String,
    pub url: String,
    pub title: String,
    pub author: Option<String>,
    pub thumbnail_url: Option<String>,
    pub duration_seconds: Option<u64>,
}

/// A YouTube list the signed-in account can browse.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedList {
    pub id: String,
    pub label: String,
    pub built_in: bool,
}

/// Everything one read found.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SavedSnapshot {
    pub items: Vec<SavedItem>,
    /// A message per site that could not be read.
    pub failures: HashMap<String, String>,
    /// Sites whose list is longer than the read allows.
    pub truncated: Vec<String>,
    /// The sites with a usable login. A site without one is never read.
    pub connected_sites: Vec<String>,
}

/// The cookies of one site, read from the `cookies.txt` the user imported.
type Cookies = HashMap<String, String>;

const DESKTOP_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) \
     Chrome/120.0.0.0 Safari/537.36";
const MOBILE_USER_AGENT: &str =
    "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) \
     Chrome/120.0.0.0 Mobile Safari/537.36";

const YOUTUBE_ORIGIN: &str = "https://www.youtube.com";
/// INNERTUBE tolerates an older WEB client version for browse calls.
const INNERTUBE_CLIENT_VERSION: &str = "2.20240101.01.00";
const YOUTUBE_MAX_PAGES: usize = 5;
const YOUTUBE_PLAYLISTS_BROWSE_ID: &str = "FEmy_playlists";

/// X: operation names are durable; the query ids rotate with the web bundle.
const TWITTER_BOOKMARK_SEARCH: &str = "vqbH512y_Emfr0P4rXJ0Lw";
/// The public web-app bearer token every browser sends. Not a secret.
const X_BEARER_TOKEN: &str = "AAAAAAAAAAAAAAAAAAAAANRILgAAAAAAnNwIzUejRCOuH5E6I8xnZz4puTs%3D\
                              1Zv7ttfk8LF81IUq16cHjhLTvJu4FA33AGWWjCpTnA";

const TIKTOK_COLLECT_LIST: &str = "https://www.tiktok.com/api/user/collect/item_list/";
const TIKTOK_FAVORITE_LIST: &str = "https://www.tiktok.com/api/favorite/item_list/";

const VK_CHALLENGE: &str = "VK is asking to confirm you are not a robot. Wait a minute, then refresh.";
const VK_RATE_LIMITED: &str = "VK is rate limiting requests. Wait a minute, then refresh.";

/// One client for all four reads, deliberately unable to hammer a site.
///
/// Redirects are off because a single read must stay a single request. VK
/// answers a client it does not like with a redirect chain that never
/// settles, and a followed chain turns one read into twenty requests, which
/// is what trips VK's flood control.
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(DESKTOP_USER_AGENT)
            .redirect(Policy::none())
            .timeout(Duration::from_secs(20))
            .build()
            .expect("failed to build HTTP client")
    })
}

/// Read the cookies of a site from the profile the user imported for it.
fn site_cookies(app: &AppHandle, site: Site) -> Cookies {
    let dir = pluck::app_cookies_dir(app);
    let Some(path) = settings_cookie_file(&dir, site.cookie_probe()) else {
        return HashMap::new();
    };
    parse_netscape_cookies(Path::new(&path))
}

/// Parse a Netscape `cookies.txt` into a name/value map.
///
/// HttpOnly cookies are written with a `#HttpOnly_` prefix, which looks
/// like a comment but is not one: those are the session cookies the sites
/// actually need, so they are read and the prefix is dropped.
fn parse_netscape_cookies(path: &Path) -> Cookies {
    let mut out = HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return out;
    };
    for line in text.lines() {
        let mut line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#HttpOnly_") {
            line = rest;
        } else if line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() >= 7 {
            let name = fields[5].trim();
            if !name.is_empty() {
                out.insert(name.to_string(), fields[6].trim().to_string());
            }
        }
    }
    out
}

fn cookie_header(cookies: &Cookies) -> String {
    cookies
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ")
}

fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        if let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            map.insert(name, value);
        }
    }
    map
}

/// A request that must answer with a body. A 3xx is a refusal here, not a
/// page that moved.
async fn get_text(url: &str, head: HeaderMap) -> Result<String, String> {
    let response = client()
        .get(url)
        .headers(head)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {} from the site", status.as_u16()));
    }
    response.text().await.map_err(|e| e.to_string())
}

/// Read the saved videos of every site that has a usable login.
///
/// One site refusing does not hide the others, so each read is independent
/// and its failure is reported for that site alone.
pub async fn fetch_all(
    app: &AppHandle,
    sites: Option<Vec<String>>,
    youtube_list_id: Option<String>,
) -> SavedSnapshot {
    let wanted: Vec<Site> = match sites {
        Some(keys) => keys.iter().filter_map(|k| Site::from_key(k)).collect(),
        None => Site::all().to_vec(),
    };

    let mut snapshot = SavedSnapshot::default();
    for site in wanted {
        let cookies = site_cookies(app, site);
        if cookies.is_empty() {
            // No login for this site: it is not read, and not offered.
            continue;
        }
        snapshot.connected_sites.push(site.key().to_string());
        let list_id = youtube_list_id.clone().unwrap_or_else(|| "WL".to_string());
        let read = match site {
            Site::YouTube => fetch_youtube(&cookies, &list_id).await,
            Site::TikTok => fetch_tiktok(&cookies).await,
            Site::Twitter => fetch_twitter(&cookies).await,
            Site::Vk => fetch_vk(&cookies).await,
        };
        match read {
            Ok((items, truncated)) => {
                snapshot.items.extend(items);
                if truncated {
                    snapshot.truncated.push(site.key().to_string());
                }
            }
            Err(message) => {
                snapshot.failures.insert(site.key().to_string(), message);
            }
        }
    }
    snapshot
}

/// The YouTube lists the signed-in account can browse.
pub async fn youtube_lists(app: &AppHandle) -> Vec<SavedList> {
    let cookies = site_cookies(app, Site::YouTube);
    if cookies.is_empty() {
        return Vec::new();
    }
    let mut lists = vec![
        SavedList {
            id: "WL".into(),
            label: "Watch Later".into(),
            built_in: true,
        },
        SavedList {
            id: "LL".into(),
            label: "Liked videos".into(),
            built_in: true,
        },
    ];
    // Best effort: a failure here still leaves Watch Later and Liked usable.
    if let Ok(body) = innertube_post(
        &cookies,
        json!({ "browseId": YOUTUBE_PLAYLISTS_BROWSE_ID }),
        "https://www.youtube.com/feed/playlists",
    )
    .await
    {
        lists.extend(parse_youtube_playlists(&body));
    }
    lists
}

// -----------------------------------------------------------------------------------
// YouTube
// -----------------------------------------------------------------------------------

/// Read a YouTube list, following continuations up to [`YOUTUBE_MAX_PAGES`].
///
/// A long Watch Later list runs to thousands of items and the view only
/// needs enough to browse and pluck from, so the read is capped and the cap
/// is reported back rather than letting the list look complete when it is
/// not.
async fn fetch_youtube(cookies: &Cookies, list_id: &str) -> Result<(Vec<SavedItem>, bool), String> {
    let mut items: Vec<SavedItem> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut continuation: Option<String> = None;
    let mut page = 0;

    loop {
        // A continuation request carries the token *instead of* a browseId,
        // YouTube rejects the two together.
        let payload = match &continuation {
            Some(token) => json!({ "continuation": token }),
            None => json!({ "browseId": format!("VL{list_id}") }),
        };
        let body = innertube_post(
            cookies,
            payload,
            &format!("https://www.youtube.com/playlist?list={list_id}"),
        )
        .await?;
        if let Some(error) = innertube_error(&body) {
            return Err(format!("YouTube: {error}"));
        }
        let (page_items, next) = parse_youtube_playlist_page(&body);
        for item in page_items {
            if seen.insert(item.id.clone()) {
                items.push(item);
            }
        }
        continuation = next;
        page += 1;
        if continuation.is_none() || page >= YOUTUBE_MAX_PAGES {
            break;
        }
    }

    Ok((items, continuation.is_some()))
}

/// The cookie that authenticates INNERTUBE calls.
///
/// It is HttpOnly, so a browser login never captures it: only an imported
/// cookies.txt carries it, and without it every call is signed out.
fn youtube_sapi_sid(cookies: &Cookies) -> Option<String> {
    cookies
        .get("SAPISID")
        .or_else(|| cookies.get("__Secure-3PAPISID"))
        .or_else(|| cookies.get("__Secure-1PAPISID"))
        .cloned()
}

/// `SAPISIDHASH {timestamp}_{sha1(timestamp + ' ' + SAPISID + ' ' + origin)}`,
/// the auth header YouTube's own JS sends.
fn sapisid_hash(sapi_sid: &str) -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut hasher = Sha1::new();
    hasher.update(format!("{timestamp} {sapi_sid} {YOUTUBE_ORIGIN}"));
    let digest = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    format!("SAPISIDHASH {timestamp}_{digest}")
}

async fn innertube_post(
    cookies: &Cookies,
    payload: Value,
    referer: &str,
) -> Result<String, String> {
    let sapi_sid = youtube_sapi_sid(cookies).ok_or_else(|| {
        "YouTube needs an imported cookies.txt. The session cookie it needs is HttpOnly, \
         so a browser login cannot be used here."
            .to_string()
    })?;
    let mut body = json!({
        "context": {
            "client": {
                "clientName": "WEB",
                "clientVersion": INNERTUBE_CLIENT_VERSION,
                "hl": "en",
                "gl": "US",
            }
        }
    });
    if let (Some(target), Some(source)) = (body.as_object_mut(), payload.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    let response = client()
        .post("https://www.youtube.com/youtubei/v1/browse")
        .headers(headers(&[
            ("authorization", sapisid_hash(&sapi_sid).as_str()),
            ("x-goog-authuser", "0"),
            ("x-origin", YOUTUBE_ORIGIN),
            ("x-youtube-client-name", "1"),
            ("x-youtube-client-version", INNERTUBE_CLIENT_VERSION),
            ("origin", YOUTUBE_ORIGIN),
            ("content-type", "application/json"),
            ("user-agent", DESKTOP_USER_AGENT),
            ("cookie", cookie_header(cookies).as_str()),
            ("referer", referer),
        ]))
        .body(serde_json::to_string(&body).map_err(|e| e.to_string())?)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {} from YouTube", status.as_u16()));
    }
    response.text().await.map_err(|e| e.to_string())
}

// -----------------------------------------------------------------------------------
// X
// -----------------------------------------------------------------------------------

/// X keeps its bookmarks behind an auth-only GraphQL timeline.
async fn fetch_twitter(cookies: &Cookies) -> Result<(Vec<SavedItem>, bool), String> {
    let ct0 = cookies
        .get("ct0")
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| not_signed_in(Site::Twitter))?;
    if cookies
        .get("auth_token")
        .map(|v| v.trim().is_empty())
        .unwrap_or(true)
    {
        return Err(not_signed_in(Site::Twitter));
    }

    // The bookmark timeline has no "videos only" switch of its own, so the
    // search timeline is queried with a `filter:videos` raw query.
    let variables = json!({
        "rawQuery": "filter:videos",
        "count": 50,
        "withDownvotePerspective": false,
    });
    let features = twitter_features();
    let url = format!(
        "https://x.com/i/api/graphql/{TWITTER_BOOKMARK_SEARCH}/BookmarkSearchTimeline"
    );
    let response = client()
        .get(url)
        .query(&[("variables", &variables.to_string()), ("features", &features.to_string())])
        .headers(headers(&[
            ("authorization", format!("Bearer {X_BEARER_TOKEN}").as_str()),
            ("x-csrf-token", ct0.as_str()),
            ("x-twitter-active-user", "yes"),
            ("x-twitter-auth-type", "OAuth2Session"),
            ("x-twitter-client-language", "en"),
            ("user-agent", DESKTOP_USER_AGENT),
            ("cookie", cookie_header(cookies).as_str()),
            ("referer", "https://x.com/i/bookmarks"),
        ]))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {} from X", status.as_u16()));
    }
    let body = response.text().await.map_err(|e| e.to_string())?;
    if let Some(message) = graphql_error(&body) {
        return Err(format!("X: {message}"));
    }
    Ok((parse_twitter_bookmarks(&body), false))
}

fn twitter_features() -> Value {
    json!({
        "graphql_timeline_v2_bookmark_timeline": true,
        "responsive_web_graphql_exclude_directive_enabled": true,
        "responsive_web_graphql_timeline_navigation_enabled": true,
        "tweetypie_unmention_optimization_enabled": true,
        "responsive_web_edit_tweet_api_enabled": true,
        "graphql_is_translatable_rweb_tweet_is_translatable_enabled": true,
        "view_counts_everywhere_api_enabled": true,
        "longform_notifs_consumption_enabled": true,
        "responsive_web_twitter_article_tweet_consumption_enabled": true,
        "tweet_awards_web_tipping_enabled": false,
        "freedom_of_speech_not_reached_appeal_enabled": true,
        "standardized_nudges_misinfo": true,
        "tweet_with_visibility_results_prefer_gql_limited_actions_policy_enabled": true,
        "rweb_video_timestamps_enabled": true,
        "longform_notifs_rich_text_read_enabled": true,
        "responsive_web_enhance_cards_enabled": false,
    })
}

// -----------------------------------------------------------------------------------
// TikTok
// -----------------------------------------------------------------------------------

/// Saved items if there are any, otherwise the liked ones.
async fn fetch_tiktok(cookies: &Cookies) -> Result<(Vec<SavedItem>, bool), String> {
    let sec_uid = match cookies.get("sec_user_id").filter(|v| !v.is_empty()) {
        Some(id) => id.clone(),
        None => tiktok_sec_uid(cookies)
            .await?
            .ok_or_else(|| not_signed_in(Site::TikTok))?,
    };

    let saved = tiktok_item_list(TIKTOK_COLLECT_LIST, cookies, &sec_uid).await?;
    if !saved.is_empty() {
        return Ok((saved, false));
    }
    let liked = tiktok_item_list(TIKTOK_FAVORITE_LIST, cookies, &sec_uid).await?;
    Ok((liked, false))
}

async fn tiktok_sec_uid(cookies: &Cookies) -> Result<Option<String>, String> {
    let body = get_text(
        "https://www.tiktok.com/passport/web/account/info/?aid=1988",
        headers(&[
            ("user-agent", DESKTOP_USER_AGENT),
            ("cookie", cookie_header(cookies).as_str()),
            ("referer", "https://www.tiktok.com/"),
        ]),
    )
    .await?;
    let Ok(root) = serde_json::from_str::<Value>(&body) else {
        return Ok(None);
    };
    Ok(root
        .get("data")
        .and_then(|d| d.get("sec_user_id"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from))
}

async fn tiktok_item_list(
    endpoint: &str,
    cookies: &Cookies,
    sec_uid: &str,
) -> Result<Vec<SavedItem>, String> {
    let url = format!(
        "{endpoint}?aid=1988&app_language=en&app_name=tiktok_web&count=30&cursor=0\
         &secUid={sec_uid}&coverFormat=2"
    );
    let body = get_text(
        &url,
        headers(&[
            ("user-agent", DESKTOP_USER_AGENT),
            ("cookie", cookie_header(cookies).as_str()),
            ("referer", "https://www.tiktok.com/"),
        ]),
    )
    .await?;
    Ok(parse_tiktok_item_list(&body))
}

// -----------------------------------------------------------------------------------
// VK
// -----------------------------------------------------------------------------------

/// VK's mobile favourites page has no JSON endpoint, so the video cards are
/// lifted out of the HTML.
async fn fetch_vk(cookies: &Cookies) -> Result<(Vec<SavedItem>, bool), String> {
    if cookies
        .get("remixsid")
        .map(|v| v.trim().is_empty())
        .unwrap_or(true)
    {
        return Err(not_signed_in(Site::Vk));
    }
    let response = client()
        .get("https://m.vk.com/fave?section=video")
        .headers(headers(&[
            ("user-agent", MOBILE_USER_AGENT),
            ("cookie", cookie_header(cookies).as_str()),
        ]))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    if status.is_redirection() {
        // VK bounces rather than answers when it thinks the client is a bot.
        return Err(VK_RATE_LIMITED.to_string());
    }
    if !status.is_success() {
        return Err(format!("HTTP {} from VK", status.as_u16()));
    }
    let html = response.text().await.map_err(|e| e.to_string())?;
    // The challenge page is a 200, so it parses to zero items and the view
    // would claim nothing is saved.
    if is_vk_challenge(&html) {
        return Err(VK_CHALLENGE.to_string());
    }
    Ok((parse_vk_fave_html(&html), false))
}

/// Markers of VK's bot check, which it serves with a plain 200.
///
/// `showcaptcha` and `captcha` are VK's own asset and script names rather
/// than prose, so they survive a copy change. None of them appear in
/// ordinary favourites markup, which is what keeps a genuinely empty list
/// empty.
fn is_vk_challenge(html: &str) -> bool {
    let lower = html.to_lowercase();
    ["showcaptcha", "captcha", "confirm you are not a robot", "not a robot"]
        .iter()
        .any(|marker| lower.contains(marker))
}

fn not_signed_in(site: Site) -> String {
    format!(
        "Not signed in to {}. Import that site's cookies.txt in Settings, then try again.",
        site.label()
    )
}

// -----------------------------------------------------------------------------------
// Parsers
// -----------------------------------------------------------------------------------

/// The first error X reports in a GraphQL response.
fn graphql_error(body: &str) -> Option<String> {
    let root: Value = serde_json::from_str(body).ok()?;
    let errors = root.get("errors")?.as_array()?;
    let first = errors.first()?;
    let message = first.get("message").and_then(Value::as_str).unwrap_or("");
    Some(if message.is_empty() {
        "request rejected".to_string()
    } else {
        message.to_string()
    })
}

fn parse_twitter_bookmarks(body: &str) -> Vec<SavedItem> {
    let mut items = Vec::new();
    let Ok(root) = serde_json::from_str::<Value>(body) else {
        return items;
    };
    let Some(instructions) = twitter_instructions(&root) else {
        return items;
    };
    for instruction in instructions.as_array().into_iter().flatten() {
        let Some(entries) = instruction.get("entries").and_then(Value::as_array) else {
            continue;
        };
        for entry in entries {
            let entry_id = entry.get("entryId").and_then(Value::as_str).unwrap_or("");
            if !entry_id.starts_with("tweet-") {
                continue;
            }
            let Some(result) = tweet_result(entry) else {
                continue;
            };
            let Some(legacy) = result.get("legacy") else {
                continue;
            };
            let tweet_id = legacy
                .get("id_str")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .unwrap_or_else(|| entry_id.trim_start_matches("tweet-").to_string());
            let entities = legacy
                .get("extended_entities")
                .or_else(|| legacy.get("entities"));
            let thumbnail = entities
                .and_then(|e| e.get("media"))
                .and_then(|m| m.get(0))
                .and_then(|m| m.get("media_url_https"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty() && !s.contains("t.co"))
                .map(String::from);
            items.push(SavedItem {
                id: tweet_id.clone(),
                site: Site::Twitter.key().to_string(),
                url: format!("https://x.com/i/status/{tweet_id}"),
                title: legacy
                    .get("full_text")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("X post")
                    .to_string(),
                author: twitter_author_label(result),
                thumbnail_url: thumbnail,
                duration_seconds: None,
            });
        }
    }
    items
}

/// The bookmark timeline's instructions, from whichever of X's two response
/// shapes arrived.
fn twitter_instructions(root: &Value) -> Option<&Value> {
    let data = root.get("data")?;
    let timeline = data
        .get("bookmark_timeline")
        .or_else(|| {
            data.get("search_by_raw_query")
                .and_then(|q| q.get("bookmarks_search_timeline"))
        })?;
    timeline.get("timeline")?.get("instructions")
}

fn tweet_result(entry: &Value) -> Option<&Value> {
    let result = entry
        .get("content")
        .and_then(|c| c.get("itemContent"))
        .and_then(|c| c.get("tweet_results"))
        .and_then(|r| r.get("result"))?;
    // Unhydrated tweets arrive wrapped in a { tweet: ... } box.
    Some(
        result
            .get("tweet")
            .or_else(|| result.get("tweetWithVisibilityResults").and_then(|t| t.get("tweet")))
            .unwrap_or(result),
    )
}

fn twitter_author_label(result: &Value) -> Option<String> {
    let user = result
        .get("core")
        .and_then(|c| c.get("user_results"))
        .and_then(|u| u.get("result"))?;
    let legacy = user.get("legacy");
    let name = legacy
        .and_then(|l| l.get("name"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let handle = legacy
        .and_then(|l| l.get("screen_name"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            user.get("rest_id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
        });
    match (name, handle) {
        (Some(name), Some(handle)) => Some(format!("{name} (@{handle})")),
        (Some(name), None) => Some(name.to_string()),
        (None, Some(handle)) => Some(format!("@{handle}")),
        (None, None) => None,
    }
}

fn parse_tiktok_item_list(body: &str) -> Vec<SavedItem> {
    let mut items = Vec::new();
    let Ok(root) = serde_json::from_str::<Value>(body) else {
        return items;
    };
    let Some(list) = root.get("itemList").and_then(Value::as_array) else {
        return items;
    };
    for entry in list {
        let id = entry.get("id").and_then(Value::as_str).unwrap_or("");
        if id.is_empty() {
            continue;
        }
        let author = entry.get("author");
        let unique_id = author
            .and_then(|a| a.get("uniqueId"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let nickname = author
            .and_then(|a| a.get("nickname"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        let author_label = match (nickname, unique_id.is_empty()) {
            (Some(nickname), false) => Some(format!("{nickname} (@{unique_id})")),
            (None, false) => Some(format!("@{unique_id}")),
            (Some(nickname), true) => Some(nickname.to_string()),
            (None, true) => None,
        };
        let video = entry.get("video");
        let duration = video
            .and_then(|v| v.get("duration"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let cover = video
            .and_then(|v| v.get("cover"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(String::from);
        let url = if unique_id.is_empty() {
            format!("https://www.tiktok.com/video/{id}")
        } else {
            format!("https://www.tiktok.com/@{unique_id}/video/{id}")
        };
        items.push(SavedItem {
            id: id.to_string(),
            site: Site::TikTok.key().to_string(),
            url,
            title: entry
                .get("desc")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("TikTok video")
                .to_string(),
            author: author_label,
            thumbnail_url: cover,
            duration_seconds: (duration > 0).then_some(duration as u64),
        });
    }
    items
}

/// VK's mobile favourites page has no JSON endpoint, so the video cards are
/// lifted out of the HTML.
fn parse_vk_fave_html(html: &str) -> Vec<SavedItem> {
    let document = Html::parse_document(html);
    let link_selector = Selector::parse("a[href]").expect("a[href] is a valid selector");
    let image_selector = Selector::parse("img").expect("img is a valid selector");

    let mut items = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for link in document.select(&link_selector) {
        let href = link.value().attr("href").unwrap_or("");
        let Some(id) = vk_object_id(href) else {
            continue;
        };
        if !seen.insert(id.clone()) {
            continue;
        }
        // VK puts the title on the anchor, not on the image.
        let title = link
            .value()
            .attr("title")
            .filter(|t| !t.trim().is_empty())
            .map(String::from)
            .or_else(|| {
                link.select(&image_selector)
                    .next()
                    .and_then(|img| img.value().attr("alt"))
                    .filter(|a| !a.trim().is_empty())
                    .map(String::from)
            })
            .unwrap_or_else(|| format!("VK video {id}"));
        let thumbnail = link
            .select(&image_selector)
            .next()
            .and_then(|img| img.value().attr("src"))
            .filter(|src| src.starts_with("http"))
            .map(String::from);
        items.push(SavedItem {
            id: id.clone(),
            site: Site::Vk.key().to_string(),
            // VK serves both videos and clips from the /video<id> form.
            url: format!("https://vk.com/video{id}"),
            title,
            author: None,
            thumbnail_url: thumbnail,
            duration_seconds: None,
        });
    }
    items
}

/// The object id of a VK link: `-123_456`, where the owner id can be
/// negative and the second half is the item.
fn vk_object_id(href: &str) -> Option<String> {
    let rest = href
        .strip_prefix("/videos")
        .or_else(|| href.strip_prefix("/video"))
        .or_else(|| href.strip_prefix("/clip"))?;
    let id = rest.split(['?', '#']).next().unwrap_or("");
    let (owner, item) = id.split_once('_')?;
    if owner.is_empty() || item.is_empty() {
        return None;
    }
    let numeric = |part: &str| {
        part.trim_start_matches('-')
            .chars()
            .all(|c| c.is_ascii_digit())
            && part.trim_start_matches('-').len() > 0
    };
    if numeric(owner) && numeric(item) {
        Some(id.to_string())
    } else {
        None
    }
}

/// One page of an INNERTUBE playlist response: the videos on it and the
/// token for the next one.
fn parse_youtube_playlist_page(body: &str) -> (Vec<SavedItem>, Option<String>) {
    let mut items: Vec<SavedItem> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let Ok(root) = serde_json::from_str::<Value>(body) else {
        return (items, None);
    };

    let mut collect = |key: &str, value: &Value| {
        if key == "playlistVideoRenderer" {
            if let Some(item) = youtube_item(value) {
                if seen.insert(item.id.clone()) {
                    items.push(item);
                }
            }
        }
    };
    walk_renderers(root.get("contents"), &mut collect);
    walk_renderers(root.get("onResponseReceivedActions"), &mut collect);

    // The next-page token lives wherever YouTube put it this time, so look
    // once at the whole response instead of guessing the container.
    let mut continuation = None;
    walk_renderers(&root, &mut |key, value| {
        if key == "continuationItemRenderer" && continuation.is_none() {
            continuation = value
                .get("continuationEndpoint")
                .and_then(|c| c.get("continuationCommand"))
                .and_then(|c| c.get("token"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(String::from);
        }
    });

    (items, continuation)
}

fn youtube_item(value: &Value) -> Option<SavedItem> {
    let video_id = value.get("videoId").and_then(Value::as_str)?;
    if video_id.is_empty() {
        return None;
    }
    let title = value
        .get("title")
        .and_then(first_run)
        .or_else(|| value.get("headline").and_then(first_run))
        .unwrap_or("YouTube video");
    let channel = value
        .get("shortBylineText")
        .and_then(first_run)
        .or_else(|| value.get("longBylineText").and_then(first_run));
    let mut thumbnail = None;
    if let Some(list) = value
        .get("thumbnail")
        .and_then(|t| t.get("thumbnails"))
        .and_then(Value::as_array)
    {
        for entry in list {
            if let Some(url) = entry.get("url").and_then(Value::as_str) {
                thumbnail = Some(url.to_string());
            }
        }
    }
    // setVideoId identifies this playlist *entry* rather than the video,
    // which is what keeps two copies of the same video in one list apart.
    let entry_id = value
        .get("setVideoId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    Some(SavedItem {
        id: entry_id.unwrap_or(video_id).to_string(),
        site: Site::YouTube.key().to_string(),
        url: format!("https://www.youtube.com/watch?v={video_id}"),
        title: title.to_string(),
        author: channel,
        thumbnail_url: thumbnail,
        duration_seconds: value
            .get("lengthSeconds")
            .and_then(|s| s.as_str())
            .and_then(|s| s.parse::<u64>().ok()),
    })
}

/// The signed-in account's own playlists, out of a `FEmy_playlists` response.
fn parse_youtube_playlists(body: &str) -> Vec<SavedList> {
    let mut lists: Vec<SavedList> = Vec::new();
    let Ok(root) = serde_json::from_str::<Value>(body) else {
        return lists;
    };
    let mut seen: HashSet<String> = HashSet::new();
    let contents = root.get("contents").cloned().unwrap_or(Value::Null);
    walk_renderers(&contents, &mut |key, value| {
        match key {
            "playlistRenderer" => {
                let id = value.get("playlistId").and_then(Value::as_str).unwrap_or("");
                if id.is_empty() || !seen.insert(id.to_string()) {
                    return;
                }
                let label = value
                    .get("title")
                    .and_then(playlist_title)
                    .unwrap_or_else(|| format!("Playlist {id}"));
                lists.push(SavedList {
                    id: id.to_string(),
                    label,
                    built_in: false,
                });
            }
            // Newer responses model the same card as a generic lockup, whose
            // title lives at metadata.lockupMetadataViewModel.title.content.
            "lockupViewModel" => {
                let id = value.get("contentId").and_then(Value::as_str).unwrap_or("");
                if !id.starts_with("PL") || !seen.insert(id.to_string()) {
                    return;
                }
                let title = value
                    .get("metadata")
                    .and_then(|m| m.get("lockupMetadataViewModel"))
                    .and_then(|m| m.get("title"));
                let label = title
                    .and_then(|t| t.get("content"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .or_else(|| title.and_then(playlist_title))
                    .unwrap_or_else(|| format!("Playlist {id}"));
                lists.push(SavedList {
                    id: id.to_string(),
                    label,
                    built_in: false,
                });
            }
            _ => {}
        }
    });
    lists
}

/// The first error YouTube reports in an INNERTUBE response.
///
/// Mutation endpoints answer HTTP 200 with an `error` object rather than a
/// failure status, so without this a refused read would look like a success.
fn innertube_error(body: &str) -> Option<String> {
    let root: Value = serde_json::from_str(body).ok()?;
    let error = root.get("error")?;
    if let Some(alerts) = error.get("alerts").and_then(Value::as_array) {
        for alert in alerts {
            if let Some(message) = alert.get("message").and_then(Value::as_str) {
                if !message.is_empty() {
                    return Some(message.to_string());
                }
            }
        }
    }
    let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
    (code != 0).then(|| format!("YouTube error {code}"))
}

/// Applies `on_renderer` to every `*Renderer` / `*ViewModel` object in the
/// subtree.
fn walk_renderers<F>(value: &Value, on_renderer: &mut F)
where
    F: FnMut(&str, &Value),
{
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if child.is_object() && (key.ends_with("Renderer") || key.ends_with("ViewModel")) {
                    on_renderer(key, child);
                }
                walk_renderers(child, on_renderer);
            }
        }
        Value::Array(items) => {
            for item in items {
                walk_renderers(item, on_renderer);
            }
        }
        _ => {}
    }
}

/// First text of a `{"runs":[{"text":...}]}` or `{"simpleText":...}` object.
fn first_run(value: &Value) -> Option<String> {
    value
        .get("runs")
        .and_then(Value::as_array)
        .and_then(|runs| runs.first())
        .and_then(|run| run.get("text"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| {
            value
                .get("simpleText")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(String::from)
        })
}

/// The title of a playlist card, which is a text object.
fn playlist_title(value: &Value) -> Option<String> {
    value
        .get("simpleText")
        .and_then(|t| t.get("text"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| first_run(value))
}

/// Read the saved videos of every site that has a usable login.
///
/// `sites` narrows the read; without it every site with a login is read.
#[tauri::command]
pub async fn fetch_saved(
    app: AppHandle,
    sites: Option<Vec<String>>,
    youtube_list_id: Option<String>,
) -> Result<SavedSnapshot, String> {
    Ok(fetch_all(&app, sites, youtube_list_id).await)
}

/// The YouTube lists the signed-in account can browse.
#[tauri::command]
pub async fn list_youtube_lists(app: AppHandle) -> Result<Vec<SavedList>, String> {
    Ok(youtube_lists(&app).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_netscape_file_is_read() {
        let dir = std::env::temp_dir().join(format!("vp-cookies-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("youtube.txt");
        std::fs::write(
            &path,
            "# Netscape HTTP Cookie File\n\
             .youtube.com\tTRUE\t/\tTRUE\t0\tSAPISID\tvalue123\n\
             #HttpOnly_.youtube.com\tTRUE\t/\tTRUE\t0\t__Secure-3PAPISID\thidden456\n",
        )
        .unwrap();
        let cookies = parse_netscape_cookies(&path);
        // The HttpOnly cookie is the one that matters, and it is read.
        assert_eq!(cookies.get("SAPISID").map(String::as_str), Some("value123"));
        assert_eq!(
            cookies.get("__Secure-3PAPISID").map(String::as_str),
            Some("hidden456")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_vk_link_yields_its_object_id() {
        assert_eq!(vk_object_id("/video-123_456").as_deref(), Some("-123_456"));
        assert_eq!(vk_object_id("/clip-1_2").as_deref(), Some("-1_2"));
        assert_eq!(vk_object_id("/video-123_456?list=1").as_deref(), Some("-123_456"));
        assert_eq!(vk_object_id("/friends"), None);
        assert_eq!(vk_object_id("/video-abc_def"), None);
    }

    #[test]
    fn a_vk_fave_page_yields_its_videos() {
        let html = r#"
            <a href="/video-1_2" title="A cat"><img src="https://vk.com/cat.jpg" alt="cat"/></a>
            <a href="/video-1_2" title="A cat again"><img src="https://vk.com/cat.jpg"/></a>
            <a href="/video-3_4"><img src="https://vk.com/dog.jpg" alt="A dog"/></a>
        "#;
        let items = parse_vk_fave_html(html);
        assert_eq!(items.len(), 2, "the same video is listed once");
        assert_eq!(items[0].title, "A cat");
        assert_eq!(items[0].url, "https://vk.com/video-1_2");
        assert_eq!(items[1].title, "A dog", "the image's alt is the fallback");
    }

    #[test]
    fn a_tiktok_list_yields_its_items() {
        let body = r#"{"itemList":[{"id":"123","desc":"a clip","author":{"uniqueId":"someone","nickname":"Someone"},"video":{"duration":30,"cover":"https://tiktok/cover.jpg"}}]}"#;
        let items = parse_tiktok_item_list(body);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].url, "https://www.tiktok.com/@someone/video/123");
        assert_eq!(items[0].title, "a clip");
        assert_eq!(items[0].author.as_deref(), Some("Someone (@someone)"));
        assert_eq!(items[0].duration_seconds, Some(30));
    }

    #[test]
    fn a_youtube_page_yields_its_videos_and_the_next_token() {
        let body = r#"{
          "contents": {"playlistVideoListRenderer": {"contents": [
            {"playlistVideoRenderer": {"videoId":"abc","title":{"runs":[{"text":"A video"}]},
              "shortBylineText":{"runs":[{"text":"A channel"}]},
              "lengthSeconds":"125",
              "thumbnail":{"thumbnails":[{"url":"https://yt/1.jpg"},{"url":"https://yt/2.jpg"}]},
              "setVideoId":"abc"}}
          ]}},
          "onResponseReceivedActions": [{"appendContinuationItemsAction": {"continuationItems": [
            {"continuationItemRenderer": {"continuationEndpoint": {"continuationCommand": {"token":"TOKEN"}}}}
          ]}}]
        }"#;
        let (items, continuation) = parse_youtube_playlist_page(body);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "abc");
        assert_eq!(items[0].url, "https://www.youtube.com/watch?v=abc");
        assert_eq!(items[0].title, "A video");
        assert_eq!(items[0].author.as_deref(), Some("A channel"));
        assert_eq!(items[0].duration_seconds, Some(125));
        assert_eq!(
            items[0].thumbnail_url.as_deref(),
            Some("https://yt/2.jpg"),
            "the last thumbnail wins"
        );
        assert_eq!(continuation.as_deref(), Some("TOKEN"));
    }

    #[test]
    fn a_twitter_bookmark_timeline_yields_its_tweets() {
        let body = r#"{"data":{"search_by_raw_query":{"bookmarks_search_timeline":{"timeline":{"instructions":[
          {"entries":[{"entryId":"tweet-42","content":{"itemContent":{"tweet_results":{"result":{
            "legacy":{"id_str":"42","full_text":"a clip"},
            "core":{"user_results":{"result":{"legacy":{"name":"Someone","screen_name":"someone"}}}}
          }}}}}]}
        ]}}}}}"#;
        let items = parse_twitter_bookmarks(body);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].url, "https://x.com/i/status/42");
        assert_eq!(items[0].title, "a clip");
        assert_eq!(items[0].author.as_deref(), Some("Someone (@someone)"));
    }

    #[test]
    fn an_innertube_error_is_reported() {
        let body = r#"{"error":{"alerts":[{"message":"Sign in to continue"}],"code":401}}"#;
        assert_eq!(innertube_error(body).as_deref(), Some("Sign in to continue"));
        assert_eq!(innertube_error(r#"{"contents":{}}"#), None);
    }

    #[test]
    fn a_graphql_error_is_reported() {
        let body = r#"{"errors":[{"message":"Authorization required"}]}"#;
        assert_eq!(graphql_error(body).as_deref(), Some("Authorization required"));
        assert_eq!(graphql_error(r#"{"data":{}}"#), None);
    }
}
