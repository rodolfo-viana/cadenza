//! A tiny web server on 127.0.0.1 for what the webview can't load by itself.
//!
//! - YouTube's player refuses to play (error 153) unless the request names
//!   the embedding page in an HTTP `Referer`, and pages from Cadenza's own
//!   `tauri://` scheme never send one. In the app, slides therefore embed a
//!   small wrapper page from `http://127.0.0.1:PORT/youtube.com/embed/ID`; it
//!   embeds the real player (which then sees a localhost `Referer`) and passes
//!   reveal.js's play/pause messages through. The path keeps the
//!   `youtube.com/embed/` that reveal.js looks for.
//! - WebKitGTK's media player can't read custom schemes like `asset://`, so on
//!   Linux the videos of open documents are streamed from
//!   `/files/TOKEN/PATH`. Only files inside the folder given to `allow_dir`
//!   are served, and only with this run's random token.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use crate::util::{decode_percent, encode_url_path};

/// The server's address (`http://127.0.0.1:PORT`), started on first use;
/// `None` if it could not start (callers then fall back to direct URLs).
pub fn host() -> Option<&'static str> {
    static HOST: OnceLock<Option<String>> = OnceLock::new();
    HOST.get_or_init(|| {
        let listener = TcpListener::bind("127.0.0.1:0").ok()?;
        let port = listener.local_addr().ok()?.port();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                std::thread::spawn(move || {
                    let _ = serve(stream);
                });
            }
        });
        Some(format!("http://127.0.0.1:{port}"))
    })
    .as_deref()
}

static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Sets the only folder whose files may be served.
pub fn allow_dir(dir: &Path) {
    let _ = ROOT.set(dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()));
}

fn token() -> &'static str {
    static TOKEN: OnceLock<String> = OnceLock::new();
    TOKEN.get_or_init(|| uuid::Uuid::new_v4().simple().to_string())
}

/// URL that streams `path` (inside the allowed folder) from the server.
pub fn file_url(path: &Path) -> Option<String> {
    Some(format!("{}/files/{}/{}", host()?, token(), encode_url_path(&path.to_string_lossy())))
}

fn serve(mut stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(&stream);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    // Read the headers too: closing with unread input can reset the
    // connection before the browser gets the reply.
    let mut range = None;
    let mut header = String::new();
    while reader.read_line(&mut header)? > 2 {
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("range")
        {
            range = Some(value.trim().to_string());
        }
        header.clear();
    }
    let mut parts = request.split(' ');
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if method != "GET" && method != "HEAD" {
        return stream.write_all(b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    }
    if let Some(rest) = target.strip_prefix("/files/") {
        return serve_file(&mut stream, rest, range.as_deref(), method == "HEAD");
    }
    let reply = match page(target) {
        Some(html) => format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nReferrer-Policy: strict-origin-when-cross-origin\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{html}",
            html.len()
        ),
        None => not_found(),
    };
    stream.write_all(reply.as_bytes())
}

fn not_found() -> String {
    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
}

/// The file a `/files/TOKEN/PATH` request asks for, if it may be served.
fn file_path(rest: &str) -> Option<PathBuf> {
    let (tok, path) = rest.split_once('/')?;
    if tok != token() {
        return None;
    }
    let path = Path::new(&decode_percent(path)?).canonicalize().ok()?;
    path.starts_with(ROOT.get()?).then_some(path)
}

/// Streams a file, honoring a `Range` request so that video can seek.
fn serve_file(stream: &mut TcpStream, rest: &str, range: Option<&str>, head: bool) -> std::io::Result<()> {
    let Some((mut file, path)) = file_path(rest).and_then(|p| Some((File::open(&p).ok()?, p))) else {
        return stream.write_all(not_found().as_bytes());
    };
    let len = file.metadata()?.len();
    let (status, start, count) = match range {
        None => ("200 OK", 0, len),
        Some(spec) => match byte_range(spec, len) {
            Some((start, end)) => ("206 Partial Content", start, end - start + 1),
            None => {
                let reply = format!("HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{len}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                return stream.write_all(reply.as_bytes());
            }
        },
    };
    let mut headers = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {}\r\nContent-Length: {count}\r\nAccept-Ranges: bytes\r\nCache-Control: no-store\r\nConnection: close\r\n",
        mime_guess::from_path(&path).first_or_octet_stream()
    );
    if range.is_some() {
        headers.push_str(&format!("Content-Range: bytes {start}-{}/{len}\r\n", start + count - 1));
    }
    headers.push_str("\r\n");
    stream.write_all(headers.as_bytes())?;
    if !head && count > 0 {
        file.seek(SeekFrom::Start(start))?;
        std::io::copy(&mut file.take(count), stream)?;
    }
    Ok(())
}

/// First range of a `bytes=…` header as inclusive `(start, end)` offsets.
fn byte_range(spec: &str, len: u64) -> Option<(u64, u64)> {
    let first = spec.trim().strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = first.split_once('-')?;
    let last = len.checked_sub(1)?;
    let (start, end) = if a.is_empty() {
        (len.saturating_sub(b.parse().ok()?), last)
    } else {
        let start: u64 = a.parse().ok()?;
        let end = if b.is_empty() { last } else { b.parse::<u64>().ok()?.min(last) };
        (start, end)
    };
    (start <= end).then_some((start, end))
}

/// The wrapper page for `/youtube.com/embed/ID?QUERY`. The video id and the
/// player options (built by `render`) may only use URL-safe characters.
fn page(target: &str) -> Option<String> {
    let rest = target.strip_prefix("/youtube.com/embed/")?;
    let (id, query) = rest.split_once('?').unwrap_or((rest, ""));
    let safe = |s: &str, extra: &str| s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || extra.contains(c));
    if id.is_empty() || !safe(id, "") || !safe(query, "=&") {
        return None;
    }
    Some(format!(
        r#"<!doctype html>
<meta charset="utf-8">
<style>html, body {{ margin: 0; height: 100%; overflow: hidden; background: #000; }} iframe {{ display: block; width: 100%; height: 100%; border: 0; }}</style>
<iframe id="player" src="https://www.youtube.com/embed/{id}?{query}" allow="autoplay; encrypted-media; picture-in-picture; fullscreen" allowfullscreen></iframe>
<script>
  var player = document.getElementById('player');
  addEventListener('message', function (e) {{
    if (e.source === parent) player.contentWindow.postMessage(e.data, 'https://www.youtube.com');
    else if (e.source === player.contentWindow) parent.postMessage(e.data, '*');
  }});
</script>
"#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(path: &str, extra: &str) -> Vec<u8> {
        let addr = host().expect("server starts").strip_prefix("http://").unwrap();
        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n{extra}\r\n").as_bytes()).unwrap();
        let mut reply = Vec::new();
        s.read_to_end(&mut reply).unwrap();
        reply
    }

    #[test]
    fn wraps_only_youtube_embeds() {
        let html = page("/youtube.com/embed/aqz-KE-bpKQ?rel=0&enablejsapi=1&mute=1").unwrap();
        assert!(html.contains(r#"src="https://www.youtube.com/embed/aqz-KE-bpKQ?rel=0&enablejsapi=1&mute=1""#));
        assert!(page("/youtube.com/embed/").is_none());
        assert!(page("/other/aqz-KE-bpKQ").is_none());
        assert!(page("/youtube.com/embed/x\"><script>").is_none());
        assert!(page("/youtube.com/embed/x?a=%22").is_none());
        let reply = String::from_utf8(get("/youtube.com/embed/abc_123?rel=0", "")).unwrap();
        assert!(reply.starts_with("HTTP/1.1 200 OK"));
        assert!(reply.contains("https://www.youtube.com/embed/abc_123?rel=0"));
    }

    #[test]
    fn parses_byte_ranges() {
        assert_eq!(byte_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(byte_range("bytes=900-", 1000), Some((900, 999)));
        assert_eq!(byte_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(byte_range("bytes=500-5000", 1000), Some((500, 999)));
        assert_eq!(byte_range("bytes=1000-", 1000), None);
        assert_eq!(byte_range("bytes=0-0", 0), None);
        assert_eq!(byte_range("items=0-1", 1000), None);
    }

    #[test]
    fn streams_only_allowed_files() {
        let root = std::env::temp_dir().join(format!("cz-embed-{}", token()));
        std::fs::create_dir_all(root.join("a b")).unwrap();
        std::fs::write(root.join("a b/v.mp4"), b"0123456789").unwrap();
        allow_dir(&root);
        let url = file_url(&root.canonicalize().unwrap().join("a b/v.mp4")).unwrap();
        let path = &url[url.find("/files/").unwrap()..];

        let full = String::from_utf8(get(path, "")).unwrap();
        assert!(
            full.starts_with("HTTP/1.1 200 OK") && full.contains("Content-Type: video/mp4") && full.ends_with("0123456789"),
            "{full}"
        );
        let part = String::from_utf8(get(path, "Range: bytes=2-4\r\n")).unwrap();
        assert!(
            part.starts_with("HTTP/1.1 206") && part.contains("Content-Range: bytes 2-4/10") && part.ends_with("234"),
            "{part}"
        );

        let outside = format!("/files/{}/{}", token(), encode_url_path(&std::env::temp_dir().to_string_lossy()));
        assert!(String::from_utf8(get(&outside, "")).unwrap().starts_with("HTTP/1.1 404"));
        let wrong_token = path.replacen(token(), "0000", 1);
        assert!(String::from_utf8(get(&wrong_token, "")).unwrap().starts_with("HTTP/1.1 404"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
