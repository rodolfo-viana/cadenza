//! Google Fonts support.
//!
//! Picking a Google font downloads its `@font-face` CSS and WOFF2 files once
//! and stores them as presentation assets, so the font travels with the
//! `.cadenza` file and works offline, while presenting and in every export.
//! The stored CSS references assets as `cz-asset:<name>`; renderers replace
//! that with a real URL (asset protocol, relative path or data URI).

use std::time::Duration;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

pub const ASSET_PREFIX: &str = "cz-asset:";

/// A font embedded in the presentation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct WebFont {
    pub family: String,
    /// "google"
    pub source: String,
    /// `@font-face` rules referencing assets as `cz-asset:<name>`.
    pub css: String,
}

/// Subsets worth embedding for presentations in Latin, Greek and Cyrillic
/// scripts (CJK fonts, which have no such subsets, keep everything).
const SUBSETS: &[&str] = &["latin", "latin-ext", "greek", "greek-ext", "cyrillic", "cyrillic-ext", "vietnamese"];
const MAX_FILES: usize = 400;
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

pub fn valid_family(family: &str) -> bool {
    let f = family.trim();
    !f.is_empty() && f.len() <= 80 && f.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
}

/// Builds the css2 API URL asking for regular, bold and their italics when
/// the family offers them. `variants` is the catalog's "400,400i,700,…".
pub fn request_url(family: &str, variants: &str) -> String {
    let available: Vec<(bool, u32)> = variants
        .split(',')
        .filter_map(|v| {
            let v = v.trim();
            let italic = v.ends_with('i');
            v.trim_end_matches('i').parse::<u32>().ok().map(|w| (italic, w))
        })
        .collect();
    let uprights: Vec<u32> = available.iter().filter(|(i, _)| !i).map(|(_, w)| *w).collect();
    let pick_regular = |ws: &[u32]| -> Option<u32> {
        if ws.contains(&400) { Some(400) } else { ws.iter().copied().min_by_key(|w| (*w as i32 - 400).abs()) }
    };
    let pick_bold = |ws: &[u32]| -> Option<u32> {
        if ws.contains(&700) { Some(700) } else { ws.iter().copied().filter(|w| *w >= 600).min() }
    };
    let name = family.trim().replace(' ', "+");
    let mut wanted: Vec<(bool, u32)> = Vec::new();
    for italic in [false, true] {
        let ws: Vec<u32> = available.iter().filter(|(i, _)| *i == italic).map(|(_, w)| *w).collect();
        if let Some(r) = pick_regular(&ws) {
            wanted.push((italic, r));
        }
        if let Some(b) = pick_bold(&ws) {
            if !wanted.contains(&(italic, b)) {
                wanted.push((italic, b));
            }
        }
    }
    if wanted.is_empty() || (uprights.is_empty() && available.is_empty()) {
        return format!("https://fonts.googleapis.com/css2?family={name}&display=swap");
    }
    wanted.sort();
    wanted.dedup();
    let has_italic = wanted.iter().any(|(i, _)| *i);
    let spec = if has_italic {
        let tuples: Vec<String> = wanted.iter().map(|(i, w)| format!("{},{}", u8::from(*i), w)).collect();
        format!("ital,wght@{}", tuples.join(";"))
    } else {
        let ws: Vec<String> = wanted.iter().map(|(_, w)| w.to_string()).collect();
        format!("wght@{}", ws.join(";"))
    };
    format!("https://fonts.googleapis.com/css2?family={name}:{spec}&display=swap")
}

/// One `@font-face` rule from Google's CSS.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    pub subset: String,
    pub rule: String,
    pub url: String,
}

/// Splits Google's CSS into rules, keeping the subset name from the comment
/// that precedes each one.
pub fn parse_faces(css: &str) -> Vec<Face> {
    let mut faces = Vec::new();
    let mut subset = String::new();
    let mut rest = css;
    loop {
        let next_comment = rest.find("/*");
        let next_rule = rest.find("@font-face");
        match (next_comment, next_rule) {
            (Some(c), Some(r)) if c < r => {
                let end = rest[c..].find("*/").map(|e| c + e + 2).unwrap_or(rest.len());
                subset = rest[c + 2..end.saturating_sub(2).max(c + 2)].trim().to_string();
                rest = &rest[end..];
            }
            (_, Some(r)) => {
                let Some(close) = rest[r..].find('}') else { break };
                let rule = &rest[r..r + close + 1];
                if let Some(u) = rule.find("url(") {
                    let after = &rule[u + 4..];
                    if let Some(e) = after.find(')') {
                        let url = after[..e].trim_matches(|c| c == '"' || c == '\'').to_string();
                        faces.push(Face { subset: subset.clone(), rule: rule.to_string(), url });
                    }
                }
                rest = &rest[r + close + 1..];
            }
            _ => break,
        }
    }
    faces
}

/// Chooses which faces to embed.
pub fn select_faces(faces: Vec<Face>) -> Vec<Face> {
    let preferred: Vec<Face> = faces.iter().filter(|f| SUBSETS.contains(&f.subset.as_str())).cloned().collect();
    let mut chosen = if preferred.is_empty() { faces } else { preferred };
    chosen.truncate(MAX_FILES);
    chosen
}

fn agent() -> ureq::Agent {
    ureq::Agent::new_with_config(ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).build())
}

/// Downloads a Google font. `store` saves a WOFF2 file as an asset and
/// returns its name.
pub fn install(family: &str, variants: &str, store: &(dyn Fn(&[u8]) -> anyhow::Result<String> + Sync)) -> anyhow::Result<WebFont> {
    if !valid_family(family) {
        bail!("“{family}” is not a valid font name");
    }
    let agent = agent();
    let fetch_css = |url: &str| -> anyhow::Result<String> {
        let mut resp = agent.get(url).header("User-Agent", USER_AGENT).call()?;
        Ok(resp.body_mut().read_to_string()?)
    };
    let css = fetch_css(&request_url(family, variants))
        .or_else(|_| fetch_css(&format!("https://fonts.googleapis.com/css2?family={}&display=swap", family.trim().replace(' ', "+"))))
        .context("Could not reach Google Fonts — check your internet connection")?;
    let faces = select_faces(parse_faces(&css));
    if faces.is_empty() {
        bail!("Google Fonts returned no files for “{family}”");
    }
    // Download in parallel (a family is typically 8–30 small files).
    let results: Vec<anyhow::Result<String>> = std::thread::scope(|scope| {
        let chunks: Vec<&[Face]> = faces.chunks(faces.len().div_ceil(8)).collect();
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                let agent = agent.clone();
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|f| -> anyhow::Result<String> {
                            if !f.url.starts_with("https://fonts.gstatic.com/") {
                                bail!("unexpected font URL");
                            }
                            let mut resp = agent.get(&f.url).header("User-Agent", USER_AGENT).call()?;
                            let bytes = resp.body_mut().with_config().limit(20 * 1024 * 1024).read_to_vec()?;
                            store(&bytes)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });
    let mut out = String::new();
    for (face, name) in faces.iter().zip(results) {
        let name = name.with_context(|| format!("Could not download {}", face.url))?;
        out.push_str(&face.rule.replace(&face.url, &format!("{ASSET_PREFIX}{name}")));
        out.push('\n');
    }
    Ok(WebFont { family: family.trim().to_string(), source: "google".into(), css: out })
}

/// Replaces `cz-asset:<name>` references with real URLs.
pub fn resolve_css(css: &str, url: &dyn Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = rest.find(ASSET_PREFIX) {
        out.push_str(&rest[..i]);
        let tail = &rest[i + ASSET_PREFIX.len()..];
        let end = tail.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))).unwrap_or(tail.len());
        let name = &tail[..end];
        out.push_str(&url(name));
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// Asset names referenced by font CSS.
pub fn asset_names(css: &str) -> Vec<String> {
    let names = std::cell::RefCell::new(Vec::new());
    let _ = resolve_css(css, &|n: &str| {
        names.borrow_mut().push(n.to_string());
        String::new()
    });
    names.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "/* cyrillic */\n@font-face {\n  font-family: 'Lora';\n  src: url(https://fonts.gstatic.com/s/lora/a.woff2) format('woff2');\n}\n/* latin */\n@font-face {\n  font-family: 'Lora';\n  src: url(https://fonts.gstatic.com/s/lora/b.woff2) format('woff2');\n}\n/* [0] */\n@font-face {\n  font-family: 'Lora';\n  src: url(https://fonts.gstatic.com/s/lora/c.woff2) format('woff2');\n}\n";

    #[test]
    fn builds_requests_for_available_styles() {
        assert_eq!(
            request_url("Open Sans", "300,400,700,400i,700i"),
            "https://fonts.googleapis.com/css2?family=Open+Sans:ital,wght@0,400;0,700;1,400;1,700&display=swap"
        );
        assert_eq!(request_url("Lobster", "400"), "https://fonts.googleapis.com/css2?family=Lobster:wght@400&display=swap");
        assert_eq!(request_url("Thin Only", "100,900"), "https://fonts.googleapis.com/css2?family=Thin+Only:wght@100;900&display=swap");
    }

    #[test]
    fn parses_and_selects_faces() {
        let faces = parse_faces(SAMPLE);
        assert_eq!(faces.len(), 3);
        assert_eq!(faces[0].subset, "cyrillic");
        assert_eq!(faces[1].url, "https://fonts.gstatic.com/s/lora/b.woff2");
        let chosen = select_faces(faces);
        assert_eq!(chosen.len(), 2);
        assert!(chosen.iter().all(|f| f.subset != "[0]"));
    }

    #[test]
    fn resolves_asset_references() {
        let css = "src: url(cz-asset:abc.woff2) format('woff2'); src: url(cz-asset:def.woff2)";
        assert_eq!(asset_names(css), vec!["abc.woff2", "def.woff2"]);
        assert_eq!(resolve_css(css, &|n| format!("assets/{n}")), "src: url(assets/abc.woff2) format('woff2'); src: url(assets/def.woff2)");
    }

    #[test]
    fn validates_family_names() {
        assert!(valid_family("Source Sans 3"));
        assert!(!valid_family("x&y=1"));
        assert!(!valid_family(""));
    }
}
