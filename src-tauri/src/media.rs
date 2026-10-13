//! Online media: photos (Unsplash, Openverse), GIFs (GIPHY, Openverse) and
//! videos (YouTube search; YouTube/Vimeo link previews via oEmbed).
//!
//! Openverse and oEmbed need no key. Unsplash, GIPHY and YouTube search use
//! the user's own API keys (see [`crate::secrets`]).

use std::time::Duration;

use anyhow::{Context, bail};
use serde::Serialize;
use serde_json::Value;

const UA: &str = concat!("Cadenza/", env!("CARGO_PKG_VERSION"), " (presentation app; +https://github.com/)");
const MAX_DOWNLOAD: u64 = 40 * 1024 * 1024;

#[derive(Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub id: String,
    pub provider: String,
    /// "image", "gif" or "video".
    pub kind: String,
    pub title: String,
    pub thumb: String,
    /// What gets downloaded (images) or embedded (videos).
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub author: String,
    pub author_url: String,
    /// Page of the item on the provider's site.
    pub page_url: String,
    pub license: String,
    /// Unsplash: endpoint that must be pinged when a photo is used.
    pub track_url: String,
}

#[derive(Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaPage {
    pub items: Vec<MediaItem>,
    /// Opaque cursor for the next page (None = no more results).
    pub next: Option<String>,
}

fn agent() -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(25)))
            .http_status_as_error(false)
            .build(),
    )
}

fn get_json(url: &str, headers: &[(&str, &str)]) -> anyhow::Result<Value> {
    let mut req = agent().get(url).header("User-Agent", UA);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let mut resp = req.call().context("Could not reach the service — check your internet connection")?;
    let status = resp.status().as_u16();
    let body = resp.body_mut().read_to_string().unwrap_or_default();
    match status {
        200..=299 => Ok(serde_json::from_str(&body)?),
        401 | 403 => bail!("The service rejected the API key (HTTP {status}). Check it in Settings ▸ Integrations."),
        429 => bail!("Too many requests — please wait a moment and try again."),
        _ => bail!("The service answered with HTTP {status}"),
    }
}

fn enc(s: &str) -> String {
    crate::util::encode_uri_component(s)
}

fn s(v: &Value, path: &[&str]) -> String {
    let mut cur = v;
    for p in path {
        cur = &cur[*p];
    }
    cur.as_str().unwrap_or_default().to_string()
}

fn n(v: &Value, path: &[&str]) -> u32 {
    let mut cur = v;
    for p in path {
        cur = &cur[*p];
    }
    cur.as_u64().or_else(|| cur.as_str().and_then(|x| x.parse().ok())).unwrap_or(0) as u32
}

const UTM: &str = "utm_source=cadenza&utm_medium=referral";

fn with_utm(url: &str) -> String {
    if url.is_empty() {
        return String::new();
    }
    format!("{url}{}{UTM}", if url.contains('?') { "&" } else { "?" })
}

/// Searches a provider. `page` is the cursor returned by the previous page.
pub fn search(provider: &str, query: &str, page: Option<&str>, key: Option<&str>) -> anyhow::Result<MediaPage> {
    let q = query.trim();
    match provider {
        "openverse" | "openverse-gif" => {
            let p: u32 = page.and_then(|p| p.parse().ok()).unwrap_or(1);
            if q.is_empty() {
                return Ok(MediaPage::default());
            }
            let gif = provider == "openverse-gif";
            let url = format!(
                // Anonymous Openverse access allows at most 20 results per page.
                "https://api.openverse.org/v1/images/?q={}&page={p}&page_size=20&mature=false{}",
                enc(q),
                if gif { "&extension=gif" } else { "" }
            );
            let v = get_json(&url, &[])?;
            let items = v["results"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|r| MediaItem {
                            id: s(r, &["id"]),
                            provider: "openverse".into(),
                            kind: if gif { "gif".into() } else { "image".into() },
                            title: s(r, &["title"]),
                            thumb: s(r, &["thumbnail"]),
                            url: s(r, &["url"]),
                            width: n(r, &["width"]),
                            height: n(r, &["height"]),
                            author: s(r, &["creator"]),
                            author_url: s(r, &["creator_url"]),
                            page_url: s(r, &["foreign_landing_url"]),
                            license: format!("CC {} {}", s(r, &["license"]).to_uppercase(), s(r, &["license_version"]))
                                .trim()
                                .to_string(),
                            track_url: String::new(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let pages = n(&v, &["page_count"]);
            Ok(MediaPage {
                items,
                next: (p < pages).then(|| (p + 1).to_string()),
            })
        }
        "unsplash" => {
            let key = key.filter(|k| !k.is_empty()).context("NEEDS_KEY")?;
            let p: u32 = page.and_then(|p| p.parse().ok()).unwrap_or(1);
            let auth = format!("Client-ID {key}");
            let headers = [("Authorization", auth.as_str()), ("Accept-Version", "v1")];
            let (list, total_pages) = if q.is_empty() {
                let v = get_json(&format!("https://api.unsplash.com/photos?page={p}&per_page=30&order_by=popular"), &headers)?;
                (v, 50)
            } else {
                let v = get_json(
                    &format!(
                        "https://api.unsplash.com/search/photos?query={}&page={p}&per_page=30&content_filter=high",
                        enc(q)
                    ),
                    &headers,
                )?;
                let tp = n(&v, &["total_pages"]);
                (v["results"].clone(), tp)
            };
            let items = list
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|r| {
                            let raw = s(r, &["urls", "raw"]);
                            MediaItem {
                                id: s(r, &["id"]),
                                provider: "unsplash".into(),
                                kind: "image".into(),
                                title: s(r, &["alt_description"]),
                                thumb: s(r, &["urls", "small"]),
                                url: if raw.is_empty() {
                                    s(r, &["urls", "regular"])
                                } else {
                                    format!("{raw}&w=2400&q=85&fm=jpg&fit=max")
                                },
                                width: n(r, &["width"]),
                                height: n(r, &["height"]),
                                author: s(r, &["user", "name"]),
                                author_url: with_utm(&s(r, &["user", "links", "html"])),
                                page_url: with_utm(&s(r, &["links", "html"])),
                                license: "Unsplash License".into(),
                                track_url: s(r, &["links", "download_location"]),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(MediaPage {
                items,
                next: (p < total_pages).then(|| (p + 1).to_string()),
            })
        }
        "giphy" => {
            let key = key.filter(|k| !k.is_empty()).context("NEEDS_KEY")?;
            let offset: u32 = page.and_then(|p| p.parse().ok()).unwrap_or(0);
            let url = if q.is_empty() {
                format!("https://api.giphy.com/v1/gifs/trending?api_key={}&limit=30&offset={offset}&rating=pg", enc(key))
            } else {
                format!(
                    "https://api.giphy.com/v1/gifs/search?api_key={}&q={}&limit=30&offset={offset}&rating=pg",
                    enc(key),
                    enc(q)
                )
            };
            let v = get_json(&url, &[])?;
            let items: Vec<MediaItem> = v["data"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|r| {
                            let full = ["downsized_large", "original", "downsized"]
                                .iter()
                                .map(|k| &r["images"][*k])
                                .find(|im| im["url"].as_str().is_some_and(|u| !u.is_empty()))
                                .cloned()
                                .unwrap_or(Value::Null);
                            MediaItem {
                                id: s(r, &["id"]),
                                provider: "giphy".into(),
                                kind: "gif".into(),
                                title: s(r, &["title"]),
                                thumb: s(r, &["images", "fixed_width", "url"]),
                                url: s(&full, &["url"]),
                                width: n(&full, &["width"]),
                                height: n(&full, &["height"]),
                                author: s(r, &["user", "display_name"]),
                                author_url: s(r, &["user", "profile_url"]),
                                page_url: s(r, &["url"]),
                                license: "GIPHY".into(),
                                track_url: String::new(),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let total = n(&v, &["pagination", "total_count"]);
            let next = offset + items.len() as u32;
            Ok(MediaPage {
                next: (!items.is_empty() && next < total.min(4999)).then(|| next.to_string()),
                items,
            })
        }
        "youtube" => {
            let key = key.filter(|k| !k.is_empty()).context("NEEDS_KEY")?;
            if q.is_empty() {
                return Ok(MediaPage::default());
            }
            let mut url = format!(
                "https://www.googleapis.com/youtube/v3/search?part=snippet&type=video&videoEmbeddable=true&safeSearch=moderate&maxResults=24&q={}&key={}",
                enc(q),
                enc(key)
            );
            if let Some(p) = page {
                url.push_str(&format!("&pageToken={}", enc(p)));
            }
            let v = get_json(&url, &[])?;
            let items = v["items"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|r| {
                            let id = s(r, &["id", "videoId"]);
                            (!id.is_empty()).then(|| MediaItem {
                                url: format!("https://www.youtube.com/watch?v={id}"),
                                page_url: format!("https://www.youtube.com/watch?v={id}"),
                                id,
                                provider: "youtube".into(),
                                kind: "video".into(),
                                title: html_unescape(&s(r, &["snippet", "title"])),
                                thumb: s(r, &["snippet", "thumbnails", "medium", "url"]),
                                width: 1280,
                                height: 720,
                                author: s(r, &["snippet", "channelTitle"]),
                                ..Default::default()
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let next = v["nextPageToken"].as_str().map(str::to_string);
            Ok(MediaPage { items, next })
        }
        _ => bail!("Unknown media provider"),
    }
}

fn html_unescape(s: &str) -> String {
    s.replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Preview information for a YouTube or Vimeo link (no key needed).
pub fn oembed(link: &str) -> anyhow::Result<MediaItem> {
    let link = link.trim();
    let (provider, endpoint) = match crate::render::video_embed(link) {
        Some(("youtube", _)) => ("youtube", format!("https://www.youtube.com/oembed?format=json&url={}", enc(link))),
        Some(("vimeo", _)) => ("vimeo", format!("https://vimeo.com/api/oembed.json?url={}", enc(link))),
        _ => bail!("Paste a YouTube or Vimeo link"),
    };
    let v = get_json(&endpoint, &[]).context("This video is private, removed or cannot be embedded")?;
    Ok(MediaItem {
        id: link.to_string(),
        provider: provider.into(),
        kind: "video".into(),
        title: s(&v, &["title"]),
        thumb: s(&v, &["thumbnail_url"]),
        url: link.to_string(),
        width: n(&v, &["width"]).max(1),
        height: n(&v, &["height"]).max(1),
        author: s(&v, &["author_name"]),
        author_url: s(&v, &["author_url"]),
        page_url: link.to_string(),
        ..Default::default()
    })
}

/// Downloads a media file (images/GIFs to embed in the presentation).
pub fn download(url: &str) -> anyhow::Result<(Vec<u8>, String)> {
    let lower = url.to_ascii_lowercase();
    if !lower.starts_with("https://") && !lower.starts_with("http://") {
        bail!("Invalid address");
    }
    let mut resp = agent()
        .get(url)
        .header("User-Agent", UA)
        .call()
        .context("Download failed — check your internet connection")?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        bail!("Download failed (HTTP {status})");
    }
    let ctype = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes = resp
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD)
        .read_to_vec()
        .context("The file is too large (40 MB max)")?;
    let ext = match ctype.split(';').next().unwrap_or("").trim() {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/avif" => "avif",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        _ => "",
    };
    Ok((bytes, ext.to_string()))
}

/// Unsplash requires pinging this endpoint when a photo is used.
pub fn track_unsplash(track_url: &str, key: &str) {
    if track_url.starts_with("https://api.unsplash.com/") {
        let _ = get_json(track_url, &[("Authorization", &format!("Client-ID {key}")), ("Accept-Version", "v1")]);
    }
}
