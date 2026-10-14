//! Online media: photos (Openverse, Wikimedia Commons, Pexels, Pixabay), GIFs
//! (GIPHY, Openverse) and videos (YouTube search; YouTube/Vimeo link previews
//! via oEmbed).
//!
//! Openverse, Wikimedia Commons and oEmbed need no key. Pexels, Pixabay, GIPHY
//! and YouTube search use the user's own API keys (see [`crate::secrets`]).

use std::time::Duration;

use anyhow::{Context, bail};
use serde::Serialize;
use serde_json::Value;

// Wikimedia asks API clients for a User-Agent that says how to reach them.
const UA: &str = concat!(
    "Cadenza/",
    env!("CARGO_PKG_VERSION"),
    " (presentation app; +https://github.com/rodolfo-viana/cadenza)"
);
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
        // Pixabay answers a bad key with 400.
        400 | 401 | 403 if status != 400 || body.contains("API key") => {
            bail!("The service rejected the API key (HTTP {status}). Check it in Settings ▸ Integrations.")
        }
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
        "wikimedia" => {
            if q.is_empty() {
                return Ok(MediaPage::default());
            }
            let offset: u32 = page.and_then(|p| p.parse().ok()).unwrap_or(0);
            let url = format!(
                "https://commons.wikimedia.org/w/api.php?action=query&format=json&formatversion=2&generator=search&gsrsearch={}&gsrnamespace=6&gsrlimit=30&gsroffset={offset}&prop=imageinfo&iiprop=url%7Csize%7Cmime%7Cextmetadata&iiurlwidth=400&iiextmetadatafilter=Artist%7CLicenseShortName%7CObjectName",
                enc(&format!("{q} filetype:bitmap"))
            );
            Ok(wikimedia_page(&get_json(&url, &[])?))
        }
        "pexels" => {
            let key = key.filter(|k| !k.is_empty()).context("NEEDS_KEY")?;
            let p: u32 = page.and_then(|p| p.parse().ok()).unwrap_or(1);
            let url = if q.is_empty() {
                format!("https://api.pexels.com/v1/curated?page={p}&per_page=30")
            } else {
                format!("https://api.pexels.com/v1/search?query={}&page={p}&per_page=30", enc(q))
            };
            Ok(pexels_page(&get_json(&url, &[("Authorization", key)])?, p))
        }
        "pixabay" => {
            let key = key.filter(|k| !k.is_empty()).context("NEEDS_KEY")?;
            let p: u32 = page.and_then(|p| p.parse().ok()).unwrap_or(1);
            let url = format!("https://pixabay.com/api/?key={}&q={}&page={p}&per_page=30&safesearch=true", enc(key), enc(q));
            Ok(pixabay_page(&get_json(&url, &[])?, p))
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

fn id(v: &Value) -> String {
    v.as_str().map(str::to_string).or_else(|| v.as_u64().map(|n| n.to_string())).unwrap_or_default()
}

/// Plain text of an HTML snippet (Wikimedia's metadata is HTML).
fn plain(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    html_unescape(&out).split_whitespace().collect::<Vec<_>>().join(" ")
}

fn pexels_page(v: &Value, p: u32) -> MediaPage {
    let items = v["photos"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|r| MediaItem {
                    id: id(&r["id"]),
                    provider: "pexels".into(),
                    kind: "image".into(),
                    title: s(r, &["alt"]),
                    thumb: s(r, &["src", "medium"]),
                    url: s(r, &["src", "large2x"]),
                    width: n(r, &["width"]),
                    height: n(r, &["height"]),
                    author: s(r, &["photographer"]),
                    author_url: s(r, &["photographer_url"]),
                    page_url: s(r, &["url"]),
                    license: "Pexels License".into(),
                })
                .collect()
        })
        .unwrap_or_default();
    MediaPage {
        items,
        next: v["next_page"].as_str().map(|_| (p + 1).to_string()),
    }
}

fn pixabay_page(v: &Value, p: u32) -> MediaPage {
    let items: Vec<MediaItem> = v["hits"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|r| MediaItem {
                    id: id(&r["id"]),
                    provider: "pixabay".into(),
                    kind: "image".into(),
                    title: s(r, &["tags"]),
                    thumb: s(r, &["webformatURL"]),
                    url: s(r, &["largeImageURL"]),
                    width: n(r, &["imageWidth"]),
                    height: n(r, &["imageHeight"]),
                    author: s(r, &["user"]),
                    author_url: format!("https://pixabay.com/users/{}-{}/", s(r, &["user"]), id(&r["user_id"])),
                    page_url: s(r, &["pageURL"]),
                    license: "Pixabay Content License".into(),
                })
                .collect()
        })
        .unwrap_or_default();
    // Pixabay returns at most 500 results per query.
    let total = n(v, &["totalHits"]).min(500);
    MediaPage {
        next: (!items.is_empty() && p * 30 < total).then(|| (p + 1).to_string()),
        items,
    }
}

/// A Wikimedia thumbnail URL (`…/500px-Name.jpg?…`) at another width.
fn resized_thumb(thumb: &str, width: u32) -> Option<String> {
    let (path, query) = thumb.split_once('?').map_or((thumb, ""), |(p, q)| (p, q));
    let (dir, file) = path.rsplit_once('/')?;
    let (px, rest) = file.split_once("px-")?;
    if px.is_empty() || !px.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(format!("{dir}/{width}px-{rest}{}{query}", if query.is_empty() { "" } else { "?" }))
}

fn wikimedia_page(v: &Value) -> MediaPage {
    let mut pages: Vec<&Value> = v["query"]["pages"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
    pages.sort_by_key(|p| p["index"].as_u64().unwrap_or(u64::MAX));
    let items = pages
        .into_iter()
        .filter_map(|r| {
            let info = &r["imageinfo"][0];
            let mime = s(info, &["mime"]);
            // Only formats every web engine shows (no TIFF, no XCF…).
            if !matches!(mime.as_str(), "image/jpeg" | "image/png" | "image/gif" | "image/webp") {
                return None;
            }
            let (original, thumb) = (s(info, &["url"]), s(info, &["thumburl"]));
            let width = n(info, &["width"]);
            // Big photos come as a 1920px rendition; GIFs stay original (animation).
            let url = if width > 1920 && mime != "image/gif" {
                resized_thumb(&thumb, 1920).unwrap_or(original)
            } else {
                original
            };
            let meta = &info["extmetadata"];
            let file = s(r, &["title"]);
            let name = file.trim_start_matches("File:");
            let name = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
            let title = plain(&s(meta, &["ObjectName", "value"]));
            Some(MediaItem {
                id: id(&r["pageid"]),
                provider: "wikimedia".into(),
                kind: if mime == "image/gif" { "gif".into() } else { "image".into() },
                title: if title.is_empty() { name.to_string() } else { title },
                thumb,
                url,
                width,
                height: n(info, &["height"]),
                author: plain(&s(meta, &["Artist", "value"])),
                author_url: String::new(),
                page_url: s(info, &["descriptionurl"]),
                license: plain(&s(meta, &["LicenseShortName", "value"])),
            })
        })
        .collect();
    MediaPage {
        items,
        next: v["continue"]["gsroffset"].as_u64().map(|o| o.to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_pexels_results() {
        let v = json!({
            "page": 1, "per_page": 30, "next_page": "https://api.pexels.com/v1/search/?page=2&per_page=30&query=lake",
            "photos": [{
                "id": 2014422, "width": 3024, "height": 3024, "url": "https://www.pexels.com/photo/lake-2014422/",
                "photographer": "Joey Farina", "photographer_url": "https://www.pexels.com/@joey",
                "alt": "A calm lake", "src": { "medium": "https://images.pexels.com/m.jpeg", "large2x": "https://images.pexels.com/l2.jpeg" }
            }]
        });
        let page = pexels_page(&v, 1);
        let it = &page.items[0];
        assert_eq!((it.id.as_str(), it.provider.as_str(), it.title.as_str()), ("2014422", "pexels", "A calm lake"));
        assert_eq!(
            (it.thumb.as_str(), it.url.as_str()),
            ("https://images.pexels.com/m.jpeg", "https://images.pexels.com/l2.jpeg")
        );
        assert_eq!((it.author.as_str(), it.width), ("Joey Farina", 3024));
        assert_eq!(page.next.as_deref(), Some("2"));
        assert_eq!(pexels_page(&json!({ "photos": [] }), 3).next, None);
    }

    #[test]
    fn reads_pixabay_results() {
        let v = json!({
            "total": 4692, "totalHits": 500,
            "hits": [{
                "id": 195893, "pageURL": "https://pixabay.com/en/blossom-bloom-flower-195893/", "tags": "blossom, bloom, flower",
                "webformatURL": "https://pixabay.com/get/w.jpg", "largeImageURL": "https://pixabay.com/get/l.jpg",
                "imageWidth": 4000, "imageHeight": 2250, "user_id": 48777, "user": "Josch13"
            }]
        });
        let page = pixabay_page(&v, 1);
        let it = &page.items[0];
        assert_eq!(
            (it.id.as_str(), it.title.as_str(), it.url.as_str()),
            ("195893", "blossom, bloom, flower", "https://pixabay.com/get/l.jpg")
        );
        assert_eq!(it.author_url, "https://pixabay.com/users/Josch13-48777/");
        assert_eq!(page.next.as_deref(), Some("2"));
        assert_eq!(pixabay_page(&v, 17).next, None);
    }

    #[test]
    fn reads_wikimedia_results() {
        let info = |mime: &str, width: u32, thumb: &str| {
            json!([{
                "url": "https://upload.wikimedia.org/o.jpg", "thumburl": thumb, "width": width, "height": 1000, "mime": mime,
                "descriptionurl": "https://commons.wikimedia.org/wiki/File:X.jpg",
                "extmetadata": {
                    "Artist": { "value": "<a href=\"//commons.wikimedia.org/wiki/User:Ann\">Ann &amp; Bo</a>" },
                    "LicenseShortName": { "value": "CC BY-SA 4.0" }
                }
            }])
        };
        let v = json!({
            "continue": { "gsroffset": 30, "continue": "gsroffset||" },
            "query": { "pages": [
                { "pageid": 2, "index": 2, "title": "File:Small lake.png", "imageinfo": info("image/png", 800, "https://t/400px-Small.png") },
                { "pageid": 1, "index": 1, "title": "File:Big lake.jpg", "imageinfo": info("image/jpeg", 6000, "https://t/a/500px-Big.jpg?utm=x") },
                { "pageid": 3, "index": 3, "title": "File:Scan.tif", "imageinfo": info("image/tiff", 6000, "https://t/400px-Scan.jpg") }
            ] }
        });
        let page = wikimedia_page(&v);
        assert_eq!(page.items.len(), 2, "TIFF is skipped");
        let (big, small) = (&page.items[0], &page.items[1]);
        assert_eq!((big.id.as_str(), big.title.as_str()), ("1", "Big lake"));
        assert_eq!(big.url, "https://t/a/1920px-Big.jpg?utm=x");
        assert_eq!(small.url, "https://upload.wikimedia.org/o.jpg");
        assert_eq!((big.author.as_str(), big.license.as_str()), ("Ann & Bo", "CC BY-SA 4.0"));
        assert_eq!(page.next.as_deref(), Some("30"));
    }
}
