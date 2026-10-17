//! Small helpers shared across the crate: ids, escaping, colors, numbers.

use std::fmt::Write as _;

/// Generates a short, random, URL/HTML-safe identifier (base36, 10 chars),
/// optionally with a prefix such as `s` (slide) or `e` (element).
pub fn new_id(prefix: &str) -> String {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut n = uuid::Uuid::new_v4().as_u128();
    let mut out = String::with_capacity(prefix.len() + 11);
    out.push_str(prefix);
    out.push('-');
    for _ in 0..10 {
        out.push(ALPHABET[(n % 36) as usize] as char);
        n /= 36;
    }
    out
}

/// Escapes text for safe inclusion in HTML text nodes and attribute values.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Percent-encodes a string the way JavaScript's `encodeURIComponent` does.
pub fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => out.push(b as char),
            _ => {
                let _ = write!(out, "%{:02X}", b);
            }
        }
    }
    out
}

/// Percent-encodes a file path for use in a URL path. `/` and the other
/// characters a URL path may hold literally are kept, so that
/// `encodeURI(decodeURI(url))` (as reveal.js does for backgrounds) gives the
/// same URL back.
pub fn encode_url_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => out.push(b as char),
            b':' | b'@' | b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'=' => out.push(b as char),
            _ => {
                let _ = write!(out, "%{:02X}", b);
            }
        }
    }
    out
}

/// Decodes `%XX` escapes; `None` if an escape is malformed or the result
/// isn't UTF-8.
pub fn decode_percent(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// An HTTP client that tries a host's IPv4 addresses before its IPv6 ones.
/// Some hosts publish an IPv6 address that never answers, and ureq tries
/// addresses one after another, giving the first most of the time allowed:
/// a download would hang for many seconds before trying IPv4 (curl and web
/// engines race the two instead). Networks without IPv4 resolve no IPv4
/// addresses, so they lose nothing.
pub fn http_agent(config: ureq::config::Config) -> ureq::Agent {
    ureq::Agent::with_parts(config, ureq::unversioned::transport::DefaultConnector::new(), Ipv4First::default())
}

#[derive(Debug, Default)]
struct Ipv4First(ureq::unversioned::resolver::DefaultResolver);

impl ureq::unversioned::resolver::Resolver for Ipv4First {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
        let mut addrs = self.0.resolve(uri, config, timeout)?;
        addrs.sort_by_key(|a| a.is_ipv6());
        Ok(addrs)
    }
}

/// Formats a number compactly for CSS/SVG output (no trailing zeros).
pub fn num(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let rounded = (v * 100.0).round() / 100.0;
    if rounded == rounded.trunc() {
        format!("{}", rounded as i64)
    } else {
        let s = format!("{:.2}", rounded);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// An RGBA color parsed from a CSS color string.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f64,
}

/// Parses the CSS color forms Cadenza produces: `#rgb`, `#rgba`, `#rrggbb`,
/// `#rrggbbaa`, `rgb(...)` and `rgba(...)`. Returns `None` for anything else.
pub fn parse_color(s: &str) -> Option<Rgba> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let expand = |c: u8| -> Option<u8> {
            let v = (c as char).to_digit(16)? as u8;
            Some(v * 16 + v)
        };
        let pair = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        let bytes = hex.as_bytes();
        return match hex.len() {
            3 | 4 => Some(Rgba {
                r: expand(bytes[0])?,
                g: expand(bytes[1])?,
                b: expand(bytes[2])?,
                a: if hex.len() == 4 { expand(bytes[3])? as f64 / 255.0 } else { 1.0 },
            }),
            6 | 8 => Some(Rgba {
                r: pair(0)?,
                g: pair(2)?,
                b: pair(4)?,
                a: if hex.len() == 8 { pair(6)? as f64 / 255.0 } else { 1.0 },
            }),
            _ => None,
        };
    }
    let lower = s.to_ascii_lowercase();
    let inner = lower.strip_prefix("rgba(").or_else(|| lower.strip_prefix("rgb("))?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split([',', ' ', '/']).filter(|p| !p.is_empty()).collect();
    if parts.len() < 3 {
        return None;
    }
    let channel = |p: &str| p.trim().parse::<f64>().ok().map(|v| v.clamp(0.0, 255.0) as u8);
    let alpha = match parts.get(3) {
        Some(p) if p.ends_with('%') => p.trim_end_matches('%').parse::<f64>().ok()? / 100.0,
        Some(p) => p.parse::<f64>().ok()?,
        None => 1.0,
    };
    Some(Rgba {
        r: channel(parts[0])?,
        g: channel(parts[1])?,
        b: channel(parts[2])?,
        a: alpha.clamp(0.0, 1.0),
    })
}

/// Perceived brightness (0-255) using the same formula reveal.js uses to
/// decide between `has-dark-background` and `has-light-background`.
pub fn brightness(c: Rgba) -> f64 {
    (c.r as f64 * 299.0 + c.g as f64 * 587.0 + c.b as f64 * 114.0) / 1000.0
}

/// Accepts only values that are safe to embed in a CSS declaration without
/// breaking out of it (colors, font names, lengths). Anything containing
/// characters that could terminate the declaration is rejected.
pub fn css_safe(s: &str) -> bool {
    !s.chars().any(|c| matches!(c, ';' | '{' | '}' | '<' | '>' | '\\' | '\n' | '\r'))
        && !s.to_ascii_lowercase().contains("url(")
        && !s.to_ascii_lowercase().contains("expression")
}

/// Returns `s` when it is CSS-safe, otherwise an empty string.
pub fn css_value(s: &str) -> &str {
    if css_safe(s) { s.trim() } else { "" }
}

/// Validates the names Cadenza gives to stored assets (`<hash>.<ext>`), which
/// also guarantees they cannot be used for path traversal.
pub fn is_valid_asset_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && !name.starts_with('.') && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// True when the reference points at the web rather than a bundled asset.
pub fn is_remote(src: &str) -> bool {
    let l = src.trim_start().to_ascii_lowercase();
    l.starts_with("http://") || l.starts_with("https://") || l.starts_with("data:")
}

/// Whether a media reference names a bundled asset. Anything else (a web
/// address, or a video link typed without "https://") is not a file to save.
pub fn is_asset_ref(src: &str) -> bool {
    !is_remote(src) && is_valid_asset_name(src)
}

/// Current time in milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Text from a file of unknown encoding: UTF-8 or UTF-16 with a byte order
/// mark, UTF-8 without one, and else Windows-1252 (what spreadsheets on
/// Windows export).
pub fn decode_text(bytes: &[u8]) -> String {
    let utf16 = |rest: &[u8], le: bool| {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| if le { u16::from_le_bytes(c) } else { u16::from_be_bytes(c) })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        _ => match std::str::from_utf8(bytes) {
            Ok(s) => s.to_string(),
            Err(_) => bytes.iter().map(|&b| windows_1252(b)).collect(),
        },
    }
}

/// A Windows-1252 byte as a character (Latin-1, but for 0x80–0x9F).
fn windows_1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™',
        'š', '›', 'œ', '\u{9D}', 'ž', 'Ÿ',
    ];
    if (0x80..0xA0).contains(&b) { HIGH[(b - 0x80) as usize] } else { b as char }
}

/// RGBA pixels as a PNG file.
pub fn encode_png(rgba: &[u8], width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    writer.finish()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_prefixed() {
        let a = new_id("s");
        let b = new_id("s");
        assert!(a.starts_with("s-"));
        assert_eq!(a.len(), 12);
        assert_ne!(a, b);
    }

    #[test]
    fn escapes_html() {
        assert_eq!(esc("<a href=\"x\">&'"), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;");
    }

    #[test]
    fn encodes_like_js() {
        assert_eq!(encode_uri_component("/home/a b/ç.png"), "%2Fhome%2Fa%20b%2F%C3%A7.png");
    }

    #[test]
    fn encodes_paths_keeping_slashes() {
        let encoded = encode_url_path("/home/a b/ç (1).png");
        assert_eq!(encoded, "/home/a%20b/%C3%A7%20(1).png");
        assert_eq!(decode_percent(&encoded).as_deref(), Some("/home/a b/ç (1).png"));
        assert_eq!(encode_url_path("C:/Users/x#y?z"), "C:/Users/x%23y%3Fz");
        assert!(decode_percent("%zz").is_none());
        assert!(decode_percent("%4").is_none());
    }

    #[test]
    fn formats_numbers() {
        assert_eq!(num(10.0), "10");
        assert_eq!(num(10.5), "10.5");
        assert_eq!(num(1.234), "1.23");
        assert_eq!(num(-0.0), "0");
    }

    #[test]
    fn parses_colors() {
        assert_eq!(parse_color("#fff").map(|c| (c.r, c.g, c.b)), Some((255, 255, 255)));
        assert_eq!(parse_color("#102030").map(|c| (c.r, c.g, c.b)), Some((16, 32, 48)));
        let c = parse_color("rgba(10, 20, 30, 0.5)").unwrap();
        assert_eq!((c.r, c.g, c.b), (10, 20, 30));
        assert!((c.a - 0.5).abs() < 1e-9);
        assert!(parse_color("red").is_none());
        assert!(brightness(parse_color("#000").unwrap()) < 1.0);
    }

    #[test]
    fn rejects_css_injection() {
        assert!(css_safe("#ff0000"));
        assert!(css_safe("'Inter', sans-serif"));
        assert!(!css_safe("red; background:url(x)"));
        assert!(!css_safe("red}</style>"));
    }

    #[test]
    fn decodes_text_files_from_spreadsheets() {
        assert_eq!(decode_text("a;é\n".as_bytes()), "a;é\n");
        assert_eq!(decode_text(b"\xEF\xBB\xBFx,y"), "x,y");
        assert_eq!(decode_text(b"\xFF\xFEa\x00\xE9\x00"), "aé");
        assert_eq!(decode_text(b"\xFE\xFF\x00a\x00\xE9"), "aé");
        assert_eq!(decode_text(b"Ano;Pre\xE7o \x80\n"), "Ano;Preço €\n", "Windows-1252");
    }

    #[test]
    fn validates_asset_names() {
        assert!(is_valid_asset_name("a1b2c3.png"));
        assert!(!is_valid_asset_name("../etc/passwd"));
        assert!(!is_valid_asset_name("a/b.png"));
        assert!(!is_valid_asset_name(".hidden"));
    }

    #[test]
    fn clipboard_pixels_become_a_png() {
        let rgba = [255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 255, 0];
        let png = encode_png(&rgba, 2, 2).unwrap();
        assert_eq!(imagesize::blob_size(&png).map(|s| (s.width, s.height)).unwrap(), (2, 2));
        assert!(encode_png(&rgba, 3, 3).is_err());
    }
}
