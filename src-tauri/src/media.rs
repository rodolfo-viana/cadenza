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
    crate::util::http_agent(
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
    let mut tag: Option<String> = None;
    for c in html.chars() {
        match (c, tag.as_mut()) {
            ('<', None) => tag = Some(String::new()),
            ('>', Some(t)) => {
                // Lines and blocks end in a space ("Ann<br>Bo" is "Ann Bo").
                let name = t.trim_start_matches('/').split(|c: char| !c.is_ascii_alphanumeric()).next().unwrap_or("");
                if matches!(name.to_ascii_lowercase().as_str(), "br" | "p" | "div" | "li" | "tr" | "td" | "th") {
                    out.push(' ');
                }
                tag = None;
            }
            (_, Some(t)) => t.push(c),
            (_, None) => out.push(c),
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
            if !web_image(&mime) {
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
            Some(MediaItem {
                thumb,
                url,
                ..wikimedia_file(r)
            })
        })
        .collect();
    MediaPage {
        items,
        next: v["continue"]["gsroffset"].as_u64().map(|o| o.to_string()),
    }
}

fn web_image(mime: &str) -> bool {
    matches!(mime, "image/jpeg" | "image/png" | "image/gif" | "image/webp")
}

/// A Wikimedia file's name, author and license (`r` is a page of an
/// imageinfo query).
fn wikimedia_file(r: &Value) -> MediaItem {
    let info = &r["imageinfo"][0];
    let meta = &info["extmetadata"];
    let file = s(r, &["title"]);
    let name = file.split_once(':').map_or(file.as_str(), |(_, n)| n);
    let name = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    let title = plain(&s(meta, &["ObjectName", "value"]));
    MediaItem {
        id: id(&r["pageid"]),
        provider: "wikimedia".into(),
        kind: if s(info, &["mime"]) == "image/gif" { "gif".into() } else { "image".into() },
        title: if title.is_empty() { name.to_string() } else { title },
        width: n(info, &["width"]),
        height: n(info, &["height"]),
        // Credits continue after the name ("…, CC BY-SA 4.0").
        author: plain(&s(meta, &["Artist", "value"])).trim_end_matches('.').to_string(),
        page_url: s(info, &["descriptionurl"]),
        license: plain(&s(meta, &["LicenseShortName", "value"])),
        ..Default::default()
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

/// What an image link shows, before inserting it: a Wikimedia file (with its
/// author and license), an image, or a web page's main image.
pub fn link(link: &str) -> anyhow::Result<MediaItem> {
    let link = link.trim();
    let lower = link.to_ascii_lowercase();
    if !lower.starts_with("https://") && !lower.starts_with("http://") {
        bail!("Invalid address");
    }
    if let Some((api, title)) = wikimedia_title(link) {
        let direct = !lower.contains("/wiki/") && !lower.contains("title=");
        if let Some(item) = wikimedia_link(&api, &title, direct.then_some(link)) {
            return Ok(item);
        }
    }
    image_at(link, true)
}

/// The API and file title of a link to a file on Wikimedia Commons or
/// Wikipedia: an upload (`upload.wikimedia.org/wikipedia/commons/…`), a file
/// page (`…/wiki/File:…`) or a picture opened from an article (`#/media/…`).
fn wikimedia_title(link: &str) -> Option<(String, String)> {
    let rest = link.split_once("://")?.1;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase().replace(".m.", ".");
    let (path, fragment) = path.split_once('#').unwrap_or((path, ""));
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    let decode = |s: &str| crate::util::decode_percent(s).map(|s| s.replace('_', " "));
    if host == "upload.wikimedia.org" || host == "thumb.wikimedia.org" {
        // wikipedia/<wiki>/[thumb/]a/ab/<name>[/<size>px-<name>.png]
        let parts: Vec<&str> = path.split('/').collect();
        let (family, wiki) = (*parts.first()?, *parts.get(1)?);
        let skip = if parts.get(2) == Some(&"thumb") { 3 } else { 2 };
        let name = parts.get(skip + 2)?;
        if family != "wikipedia" || name.is_empty() {
            return None;
        }
        let api = if wiki == "commons" {
            "commons.wikimedia.org".to_string()
        } else {
            format!("{wiki}.wikipedia.org")
        };
        return Some((format!("https://{api}/w/api.php"), format!("File:{}", decode(name)?)));
    }
    if host != "commons.wikimedia.org" && !host.ends_with(".wikipedia.org") {
        return None;
    }
    let title = if let Some(media) = fragment.strip_prefix("/media/") {
        media
    } else if let Some(t) = path.strip_prefix("wiki/") {
        t
    } else {
        query.split('&').find_map(|kv| kv.strip_prefix("title="))?
    };
    // A file, in any language ("File:", "Ficheiro:", "Datei:"…).
    let title = decode(title)?;
    title.contains(':').then(|| (format!("https://{host}/w/api.php"), title))
}

/// A Wikimedia file as a media item: the link itself when it is the image,
/// else the file (or a picture of it for SVG, TIFF, PDF…), 1920px at most.
fn wikimedia_link(api: &str, title: &str, direct: Option<&str>) -> Option<MediaItem> {
    let url = format!(
        "{api}?action=query&format=json&formatversion=2&titles={}&prop=imageinfo&iiprop=url%7Csize%7Cmime%7Cextmetadata&iiurlwidth=1920&iiextmetadatafilter=Artist%7CLicenseShortName%7CObjectName",
        enc(title)
    );
    let v = get_json(&url, &[]).ok()?;
    let r = &v["query"]["pages"][0];
    let info = &r["imageinfo"][0];
    let (original, thumb) = (s(info, &["url"]), s(info, &["thumburl"]));
    if original.is_empty() {
        return None;
    }
    let mime = s(info, &["mime"]);
    let keep = web_image(&mime) && (n(info, &["width"]) <= 1920 || mime == "image/gif");
    let url = match direct {
        Some(link) => link.to_string(),
        None if keep || thumb.is_empty() => original,
        None => thumb.clone(),
    };
    let mut item = wikimedia_file(r);
    if direct.is_some_and(|l| !l.to_ascii_lowercase().ends_with(".gif")) || !keep {
        item.kind = "image".into();
    }
    // A thumbnail link (…/1280px-Name.png) is that wide.
    let file = direct.unwrap_or(&url).split(['?', '#']).next().unwrap_or("").rsplit('/').next().unwrap_or("");
    let px = file.split_once("px-").and_then(|(w, _)| w.rsplit('-').next()?.parse::<u32>().ok());
    if let Some(w) = px.filter(|w| *w > 0 && item.width > 0) {
        item.height = (f64::from(item.height) * f64::from(w) / f64::from(item.width)).round() as u32;
        item.width = w;
    }
    Some(MediaItem {
        thumb: if thumb.is_empty() { url.clone() } else { thumb },
        url,
        ..item
    })
}

/// The image at `link`; for a web page, the image it shows in link
/// previews (Open Graph), when `page` allows.
fn image_at(link: &str, page: bool) -> anyhow::Result<MediaItem> {
    use std::io::Read;
    use ureq::ResponseExt;
    let mut resp = agent()
        .get(link)
        .header("User-Agent", UA)
        .header("Accept", "image/webp,image/png,image/jpeg,image/*;q=0.9,text/html;q=0.8,*/*;q=0.5")
        .call()
        .context("Could not reach the address — check the link and your internet connection")?;
    match resp.status().as_u16() {
        200..=299 => {}
        429 => bail!("Too many requests — please wait a moment and try again."),
        _ => bail!("The link did not open — check that it is complete and public"),
    }
    // Where the link led, after redirects: what the page's links are relative to.
    let address = resp.get_uri().to_string();
    let ctype = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    // Enough to read an image's size, or a page's head.
    let mut head = Vec::new();
    resp.body_mut()
        .as_reader()
        .take(1 << 20)
        .read_to_end(&mut head)
        .context("Download failed — check your internet connection")?;
    if let Some((width, height)) = image_size(&head) {
        let file = link.split(['?', '#']).next().unwrap_or(link).rsplit('/').next().unwrap_or("");
        let file = crate::util::decode_percent(file).unwrap_or_else(|| file.to_string());
        let gif = head.starts_with(b"GIF8");
        return Ok(MediaItem {
            id: link.to_string(),
            provider: "link".into(),
            kind: if gif { "gif".into() } else { "image".into() },
            title: file.rsplit_once('.').map_or(file.as_str(), |(stem, _)| stem).replace('_', " "),
            thumb: link.to_string(),
            url: link.to_string(),
            width,
            height,
            page_url: link.to_string(),
            ..Default::default()
        });
    }
    if page && (ctype.contains("html") || ctype.is_empty()) {
        let html = String::from_utf8_lossy(&head);
        if let Some((src, title)) = page_image(&html)
            && let Some(src) = join_url(&address, page_base(&html).as_deref(), &src)
        {
            let mut item = image_at(&src, false)?;
            item.page_url = link.to_string();
            if !title.is_empty() {
                item.title = title;
            }
            return Ok(item);
        }
    }
    bail!("This link does not show an image. Right-click the image and copy its address, then paste that.")
}

/// Size of an image in a format every web engine shows (0 × 0 for an SVG
/// without one), from its first bytes.
fn image_size(head: &[u8]) -> Option<(u32, u32)> {
    use imagesize::ImageType::*;
    if let Ok(Png | Jpeg | Gif | Webp | Bmp | Ico) = imagesize::image_type(head) {
        return imagesize::blob_size(head).ok().map(|s| (s.width as u32, s.height as u32));
    }
    let text = String::from_utf8_lossy(&head[..head.len().min(4096)]);
    let start = text.find("<svg")?;
    let tag = &text[start + 4..start + text[start..].find('>')?];
    let attrs = tag_attrs(tag);
    let get = |k: &str| attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.trim());
    let px = |v: Option<&str>| v.and_then(|v| v.trim_end_matches("px").parse::<f64>().ok()).filter(|v| *v > 0.0);
    let view: Vec<f64> = get("viewbox").unwrap_or("").split([' ', ',']).filter_map(|v| v.parse().ok()).collect();
    let (w, h) = match (px(get("width")), px(get("height"))) {
        (Some(w), Some(h)) => (w, h),
        _ if view.len() == 4 => (view[2], view[3]),
        _ => (0.0, 0.0),
    };
    Some((w.round() as u32, h.round() as u32))
}

/// The image a web page names for link previews (Open Graph or Twitter
/// cards) and the page's title there.
fn page_image(html: &str) -> Option<(String, String)> {
    let (mut image, mut title) = (None, String::new());
    for attrs in tags(html, "meta") {
        let get = |k: &str| attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        let key = get("property").or(get("name")).unwrap_or("").to_ascii_lowercase();
        let value = html_unescape(get("content").unwrap_or("").trim());
        match key.as_str() {
            "og:image" | "og:image:url" | "og:image:secure_url" | "twitter:image" | "twitter:image:src" if image.is_none() && !value.is_empty() => {
                image = Some(value)
            }
            "og:title" | "twitter:title" if title.is_empty() => title = value,
            _ => {}
        }
    }
    image.map(|i| (i, title))
}

/// Where a page's `<base href>` moves its relative links, if it does.
fn page_base(html: &str) -> Option<String> {
    tags(html, "base").find_map(|attrs| attrs.into_iter().find(|(n, _)| n == "href").map(|(_, v)| html_unescape(v.trim())))
}

/// The attributes of each `<name …>` tag of a page, in order.
fn tags<'a>(html: &'a str, name: &str) -> impl Iterator<Item = Vec<(String, String)>> + 'a {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{name}");
    let mut at = 0;
    std::iter::from_fn(move || {
        let start = at + lower.get(at..)?.find(&open)?;
        let end = lower[start..].find('>').map_or(lower.len(), |e| start + e);
        at = end + 1;
        Some(tag_attrs(&html[start + open.len()..end]))
    })
}

/// Attributes of a tag (`name="value"`, `name='value'` or `name=value`),
/// names in lowercase.
fn tag_attrs(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag;
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq].split_whitespace().last().unwrap_or("").to_ascii_lowercase();
        let after = rest[eq + 1..].trim_start();
        let (value, next) = match after.chars().next() {
            Some(q @ ('"' | '\'')) => {
                let body = &after[1..];
                let close = body.find(q).unwrap_or(body.len());
                (&body[..close], &body[(close + 1).min(body.len())..])
            }
            _ => {
                let close = after.find(char::is_whitespace).unwrap_or(after.len());
                (&after[..close], &after[close..])
            }
        };
        out.push((name, value.to_string()));
        rest = next;
    }
    out
}

/// `href` read from the page at `address`, or from the `<base href>` it
/// names; only web addresses.
fn join_url(address: &str, base: Option<&str>, href: &str) -> Option<String> {
    let address = tauri::Url::parse(address).ok()?;
    let base = base.and_then(|b| address.join(b).ok()).unwrap_or(address);
    let url = base.join(href.trim()).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.into())
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

    #[test]
    fn finds_wikimedia_files_in_links() {
        let commons = "https://commons.wikimedia.org/w/api.php".to_string();
        let file = |t: &str| Some((commons.clone(), t.to_string()));
        assert_eq!(
            wikimedia_title("https://thumb.wikimedia.org/wikipedia/commons/thumb/0/0a/Logo_Unesp.svg/1280px-Logo_Unesp.svg.png"),
            file("File:Logo Unesp.svg")
        );
        assert_eq!(
            wikimedia_title("https://upload.wikimedia.org/wikipedia/commons/a/ab/S%C3%A3o_Paulo.jpg"),
            file("File:São Paulo.jpg")
        );
        assert_eq!(
            wikimedia_title("https://commons.wikimedia.org/wiki/File:Logo_Unesp.svg?uselang=pt"),
            file("File:Logo Unesp.svg")
        );
        assert_eq!(wikimedia_title("https://commons.m.wikimedia.org/wiki/File:A.jpg"), file("File:A.jpg"));
        assert_eq!(
            wikimedia_title("https://pt.wikipedia.org/wiki/Unesp#/media/Ficheiro:Logo_Unesp.svg"),
            Some(("https://pt.wikipedia.org/w/api.php".into(), "Ficheiro:Logo Unesp.svg".into()))
        );
        assert_eq!(
            wikimedia_title("https://upload.wikimedia.org/wikipedia/en/thumb/1/1a/X.png/200px-X.png"),
            Some(("https://en.wikipedia.org/w/api.php".into(), "File:X.png".into()))
        );
        // Articles have no file: their page shows its own picture.
        assert_eq!(wikimedia_title("https://en.wikipedia.org/wiki/Brazil"), None);
        assert_eq!(wikimedia_title("https://example.com/wiki/File:A.jpg"), None);
    }

    #[test]
    fn plain_text_keeps_lines_apart() {
        assert_eq!(
            plain("Giro720<br>Original design by <a href=\"x\">Israel Pedrosa</a>."),
            "Giro720 Original design by Israel Pedrosa."
        );
        assert_eq!(plain("<p>Ann &amp; Bo</p><p>Cy</p>"), "Ann & Bo Cy");
    }

    #[test]
    fn reads_the_image_of_a_web_page() {
        let page = r#"<head><meta charset="utf-8"><meta content="A &amp; B" property="og:title">
            <meta name='twitter:image' content='/t.jpg'><META PROPERTY="og:image" CONTENT="https://x.org/i.jpg?a=1&amp;b=2"></head>"#;
        assert_eq!(page_image(page), Some(("/t.jpg".into(), "A & B".into())));
        assert_eq!(
            page_image("<meta property=og:image content=//cdn.x.org/a.png>"),
            Some(("//cdn.x.org/a.png".into(), String::new()))
        );
        assert_eq!(page_image("<title>None</title>"), None);
    }

    #[test]
    fn page_images_resolve_like_links() {
        let join = |address: &str, base: Option<&str>, href: &str| join_url(address, base, href);
        let page = "https://x.org/a/b.html?q";
        assert_eq!(join(page, None, "/t.jpg").as_deref(), Some("https://x.org/t.jpg"));
        assert_eq!(join(page, None, " c/t.jpg ").as_deref(), Some("https://x.org/a/c/t.jpg"));
        assert_eq!(join(page, None, "../t.jpg").as_deref(), Some("https://x.org/t.jpg"));
        assert_eq!(join(page, None, "?v=2").as_deref(), Some("https://x.org/a/b.html?v=2"));
        assert_eq!(join("http://x.org", None, "//cdn.x.org/a.png").as_deref(), Some("http://cdn.x.org/a.png"));
        assert_eq!(join(page, None, "https://y.org/i.png").as_deref(), Some("https://y.org/i.png"));
        assert_eq!(join(page, None, "javascript:alert(1)"), None);
        assert_eq!(join(page, None, "data:image/png;base64,AAAA"), None);
        // A <base href>, itself relative to the page or not.
        assert_eq!(join(page, Some("/static/"), "t.jpg").as_deref(), Some("https://x.org/static/t.jpg"));
        assert_eq!(join(page, Some("https://cdn.x.org/s/"), "t.jpg").as_deref(), Some("https://cdn.x.org/s/t.jpg"));
        let head = r#"<head><base target="_blank"><BASE HREF="https://cdn.x.org/s/"><base href="/other/">"#;
        assert_eq!(page_base(head).as_deref(), Some("https://cdn.x.org/s/"));
        assert_eq!(page_base("<meta property=og:image content=a.png>"), None);
    }

    #[test]
    fn page_images_are_read_from_where_the_link_leads() {
        use std::io::{BufRead, BufReader, Write};
        // A short link redirects to an article whose image is relative to it.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let host = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten().take(3) {
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let mut header = String::new();
                while reader.read_line(&mut header).unwrap_or(0) > 2 {
                    header.clear();
                }
                let reply = |head: &str, body: &str| format!("HTTP/1.1 {head}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let reply = match line.split(' ').nth(1).unwrap_or("") {
                    "/s" => reply("302 Found\r\nLocation: /news/story.html", ""),
                    "/news/story.html" => reply("200 OK\r\nContent-Type: text/html", r#"<meta property="og:image" content="img/p.svg">"#),
                    "/news/img/p.svg" => reply("200 OK\r\nContent-Type: image/svg+xml", r#"<svg width="64" height="32"></svg>"#),
                    _ => reply("404 Not Found", ""),
                };
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        let item = image_at(&format!("{host}/s"), true).unwrap();
        assert_eq!(item.url, format!("{host}/news/img/p.svg"));
        assert_eq!((item.width, item.height), (64, 32));
        assert_eq!(item.page_url, format!("{host}/s"));
    }

    #[test]
    fn reads_image_sizes_from_their_first_bytes() {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
        png.extend_from_slice(&[0, 0, 2, 0, 0, 0, 1, 0, 8, 6, 0, 0, 0]);
        assert_eq!(image_size(&png), Some((512, 256)));
        assert_eq!(
            image_size(br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 150">"#),
            Some((300, 150))
        );
        assert_eq!(image_size(br#"<svg width="64px" height='32' viewBox="0 0 1 1">"#), Some((64, 32)));
        assert_eq!(image_size(b"<!doctype html><html><body>no</body></html>"), None);
    }
}
