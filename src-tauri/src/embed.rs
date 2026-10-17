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
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use crate::util::{decode_percent, encode_url_path};

/// Connections served at once. Web engines open about six per server, and
/// the app has a few windows.
const MAX_CONNECTIONS: usize = 64;
/// Time for a client to send its request line and headers, all of them.
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);
/// Limits for a request's head; the webview sends a few short headers.
const MAX_LINE: u64 = 8 * 1024;
const MAX_HEADERS: usize = 64;
// Statuses for the webview, never shown to people.
const URI_TOO_LONG: &str = "414 URI Too Long";
const HEADERS_TOO_LARGE: &str = "431 Request Header Fields Too Large";

/// The server's address (`http://127.0.0.1:PORT`), started on first use;
/// `None` if it could not start (callers then fall back to direct URLs).
pub fn host() -> Option<&'static str> {
    static HOST: OnceLock<Option<String>> = OnceLock::new();
    HOST.get_or_init(|| {
        let listener = TcpListener::bind("127.0.0.1:0").ok()?;
        let port = listener.local_addr().ok()?.port();
        std::thread::spawn(move || run(listener, MAX_CONNECTIONS));
        Some(format!("http://127.0.0.1:{port}"))
    })
    .as_deref()
}

/// Serves each connection on a thread of its own, `max` at a time; others
/// are closed right away.
fn run(listener: TcpListener, max: usize) {
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming().flatten() {
        let Some(slot) = Slot::take(&active, max) else { continue };
        // On failure the closure, and so the slot, is dropped.
        let _ = std::thread::Builder::new().spawn(move || {
            let _slot = slot;
            let _ = serve(stream, HEAD_TIMEOUT);
        });
    }
}

/// A connection being served; frees its place when dropped.
struct Slot(Arc<AtomicUsize>);

impl Slot {
    /// A place among `max`, if one is free.
    fn take(active: &Arc<AtomicUsize>, max: usize) -> Option<Slot> {
        let slot = Slot(active.clone());
        (active.fetch_add(1, Ordering::SeqCst) < max).then_some(slot)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
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

/// Reads from a client until a deadline, however slowly it sends.
struct Deadline<'a> {
    stream: &'a TcpStream,
    at: Instant,
}

impl Read for Deadline<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.at.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        self.stream.set_read_timeout(Some(left))?;
        self.stream.read(buf)
    }
}

/// One line of a request's head; `None` if longer than `MAX_LINE`.
fn head_line(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut line = Vec::new();
    reader.take(MAX_LINE).read_until(b'\n', &mut line)?;
    if line.len() as u64 == MAX_LINE && !line.ends_with(b"\n") {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&line).into_owned()))
}

/// The request line and `Range` header of a request, or the status that
/// refuses a head too large.
fn read_head(reader: &mut impl BufRead) -> io::Result<Result<(String, Option<String>), &'static str>> {
    let Some(request) = head_line(reader)? else {
        return Ok(Err(URI_TOO_LONG));
    };
    let mut range = None;
    for _ in 0..=MAX_HEADERS {
        let Some(header) = head_line(reader)? else { break };
        if header.len() <= 2 {
            return Ok(Ok((request, range)));
        }
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("range")
        {
            range = Some(value.trim().to_string());
        }
    }
    Ok(Err(HEADERS_TOO_LARGE))
}

/// Writes have no time limit: a paused video's player stops reading for as
/// long as the pause lasts, and must find its connection when it goes on.
fn serve(mut stream: TcpStream, head_timeout: Duration) -> io::Result<()> {
    let mut reader = BufReader::new(Deadline {
        stream: &stream,
        at: Instant::now() + head_timeout,
    });
    // Read the headers too: closing with unread input can reset the
    // connection before the browser gets the reply.
    let (request, range) = match read_head(&mut reader)? {
        Ok(head) => head,
        Err(status) => {
            (&stream).write_all(format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes())?;
            // Same here: what is left of the head is read (within the
            // deadline) and dropped.
            stream.shutdown(Shutdown::Write)?;
            return io::copy(&mut reader.take(64 * 1024), &mut io::sink()).map(drop);
        }
    };
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
    fn refuses_heads_too_large() {
        let status = |path: &str, extra: &str| String::from_utf8(get(path, extra)).unwrap().lines().next().unwrap_or("").to_string();
        assert_eq!(status(&format!("/{}", "a".repeat(10_000)), ""), "HTTP/1.1 414 URI Too Long");
        let long = format!("X-A: {}\r\n", "a".repeat(10_000));
        assert_eq!(status("/youtube.com/embed/abc", &long), "HTTP/1.1 431 Request Header Fields Too Large");
        // `get` sends a Host header too.
        let headers = |n: usize| "X-A: 1\r\n".repeat(n - 1);
        assert_eq!(status("/youtube.com/embed/abc", &headers(MAX_HEADERS)), "HTTP/1.1 200 OK");
        assert_eq!(
            status("/youtube.com/embed/abc", &headers(MAX_HEADERS + 1)),
            "HTTP/1.1 431 Request Header Fields Too Large"
        );
    }

    #[test]
    fn gives_up_on_slow_senders() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (conn, _) = listener.accept().unwrap();
        let start = Instant::now();
        let server = std::thread::spawn(move || serve(conn, Duration::from_millis(300)));
        // A header that never ends, a byte at a time.
        client.write_all(b"GET /youtube.com/embed/abc HTTP/1.1\r\nX-A: ").unwrap();
        while !server.is_finished() && start.elapsed() < Duration::from_secs(5) {
            let _ = client.write_all(b"a");
            std::thread::sleep(Duration::from_millis(20));
        }
        let err = server.join().unwrap().unwrap_err();
        assert!(matches!(err.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock), "{err}");
        assert!(start.elapsed() < Duration::from_secs(2), "{:?}", start.elapsed());
    }

    #[test]
    fn serves_a_few_connections_at_a_time() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || run(listener, 2));
        let idle: Vec<TcpStream> = (0..2).map(|_| TcpStream::connect(addr).unwrap()).collect();
        // A third is closed at once, not left waiting for its request.
        let mut extra = TcpStream::connect(addr).unwrap();
        extra.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        assert_eq!(extra.read(&mut [0; 16]).unwrap(), 0);
        // Closed connections free their places.
        drop(idle);
        let start = Instant::now();
        loop {
            let mut s = TcpStream::connect(addr).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let _ = s.write_all(b"GET /youtube.com/embed/abc HTTP/1.1\r\n\r\n");
            let mut reply = Vec::new();
            let _ = s.read_to_end(&mut reply);
            if reply.starts_with(b"HTTP/1.1 200 OK") {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(5), "no place freed");
            std::thread::sleep(Duration::from_millis(20));
        }
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
