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

/// Subsets that are not a writing system: arrows, math and other symbols
/// many Latin fonts carry. Left out to keep fonts small (about a quarter of
/// Roboto); the system font draws those characters.
const SYMBOL_SUBSETS: &[&str] = &["math", "symbols"];
/// Most files one font may embed. Google serves Chinese, Japanese and Korean
/// fonts as about 120 slices per style, so regular and bold of one take up
/// to 250 files (variable fonts share them across weights).
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
        if ws.contains(&400) {
            Some(400)
        } else {
            ws.iter().copied().min_by_key(|w| (*w as i32 - 400).abs())
        }
    };
    let pick_bold = |ws: &[u32]| -> Option<u32> {
        if ws.contains(&700) {
            Some(700)
        } else {
            ws.iter().copied().filter(|w| *w >= 600).min()
        }
    };
    let name = family.trim().replace(' ', "+");
    let mut wanted: Vec<(bool, u32)> = Vec::new();
    for italic in [false, true] {
        let ws: Vec<u32> = available.iter().filter(|(i, _)| *i == italic).map(|(_, w)| *w).collect();
        if let Some(r) = pick_regular(&ws) {
            wanted.push((italic, r));
        }
        if let Some(b) = pick_bold(&ws)
            && !wanted.contains(&(italic, b))
        {
            wanted.push((italic, b));
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
/// that precedes each one (empty for the unnamed slices of CJK fonts).
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
                // A comment names only the rule right after it.
                let subset = std::mem::take(&mut subset);
                if let Some(u) = rule.find("url(") {
                    let after = &rule[u + 4..];
                    if let Some(e) = after.find(')') {
                        let url = after[..e].trim_matches(|c| c == '"' || c == '\'').to_string();
                        faces.push(Face {
                            subset,
                            rule: rule.to_string(),
                            url,
                        });
                    }
                }
                rest = &rest[r + close + 1..];
            }
            _ => break,
        }
    }
    faces
}

/// Chooses which faces to embed: every writing system the family covers, as
/// what slides will say is not known yet (an Arabic family keeps Arabic next
/// to Latin, a Japanese one all its slices, since a partial set would mix
/// fonts within a line). Symbol subsets are left out unless there is
/// nothing else; a font with more than `MAX_FILES` files is refused.
pub fn select_faces(faces: Vec<Face>) -> anyhow::Result<Vec<Face>> {
    let scripts: Vec<Face> = faces.iter().filter(|f| !SYMBOL_SUBSETS.contains(&f.subset.as_str())).cloned().collect();
    let chosen = if scripts.is_empty() { faces } else { scripts };
    if unique_urls(&chosen).len() > MAX_FILES {
        bail!("This font has too many files to embed");
    }
    Ok(chosen)
}

/// The files of `faces`, each once: variable fonts serve every weight from
/// the same file.
fn unique_urls(faces: &[Face]) -> Vec<&str> {
    let mut urls: Vec<&str> = faces.iter().map(|f| f.url.as_str()).collect();
    urls.sort_unstable();
    urls.dedup();
    urls
}

fn agent() -> ureq::Agent {
    crate::util::http_agent(ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).build())
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
        .or_else(|_| {
            fetch_css(&format!(
                "https://fonts.googleapis.com/css2?family={}&display=swap",
                family.trim().replace(' ', "+")
            ))
        })
        .context("Could not reach Google Fonts — check your internet connection")?;
    let faces = select_faces(parse_faces(&css))?;
    if faces.is_empty() {
        bail!("Google Fonts returned no files for “{family}”");
    }
    // Download in parallel (a family is typically 8–30 small files, a CJK
    // one 120–250).
    let urls = unique_urls(&faces);
    let results: Vec<anyhow::Result<String>> = std::thread::scope(|scope| {
        let chunks: Vec<&[&str]> = urls.chunks(urls.len().div_ceil(8)).collect();
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                let agent = agent.clone();
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|url| -> anyhow::Result<String> {
                            if !url.starts_with("https://fonts.gstatic.com/") {
                                bail!("unexpected font URL");
                            }
                            let mut resp = agent.get(*url).header("User-Agent", USER_AGENT).call()?;
                            let bytes = resp.body_mut().with_config().limit(20 * 1024 * 1024).read_to_vec()?;
                            store(&bytes)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });
    let mut names = std::collections::HashMap::new();
    for (url, name) in urls.iter().zip(results) {
        names.insert(*url, name.with_context(|| format!("Could not download {url}"))?);
    }
    let mut out = String::new();
    for face in &faces {
        let name = names.get(face.url.as_str()).with_context(|| format!("Could not download {}", face.url))?;
        out.push_str(&face.rule.replace(&face.url, &format!("{ASSET_PREFIX}{name}")));
        out.push('\n');
    }
    Ok(WebFont {
        family: family.trim().to_string(),
        source: "google".into(),
        css: out,
    })
}

/// Replaces `cz-asset:<name>` references with real URLs.
pub fn resolve_css(css: &str, url: &dyn Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = rest.find(ASSET_PREFIX) {
        out.push_str(&rest[..i]);
        let tail = &rest[i + ASSET_PREFIX.len()..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')))
            .unwrap_or(tail.len());
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

impl WebFont {
    /// Names of the bundled font files this font uses.
    pub fn asset_refs(&self) -> Vec<String> {
        asset_names(&self.css).into_iter().filter(|s| crate::util::is_asset_ref(s)).collect()
    }
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
        assert_eq!(
            request_url("Lobster", "400"),
            "https://fonts.googleapis.com/css2?family=Lobster:wght@400&display=swap"
        );
        assert_eq!(
            request_url("Thin Only", "100,900"),
            "https://fonts.googleapis.com/css2?family=Thin+Only:wght@100;900&display=swap"
        );
    }

    /// Google's CSS for `styles` (weight, file prefix) of a family: per style,
    /// `slices` unnamed CJK slices, then the named subsets.
    fn css(styles: &[(u32, &str)], slices: usize, subsets: &[&str]) -> String {
        let face = |w: u32, url: String| format!("@font-face {{\n  font-weight: {w};\n  src: url({url}) format('woff2');\n}}\n");
        let mut out = String::new();
        for (w, file) in styles {
            for i in 0..slices {
                out.push_str(&face(*w, format!("https://fonts.gstatic.com/s/f/{file}.{i}.woff2")));
            }
            for s in subsets {
                out.push_str(&format!("/* {s} */\n{}", face(*w, format!("https://fonts.gstatic.com/s/f/{file}-{s}.woff2"))));
            }
        }
        out
    }

    fn subsets(faces: &[Face]) -> Vec<&str> {
        faces.iter().map(|f| f.subset.as_str()).collect()
    }

    #[test]
    fn parses_faces() {
        let faces = parse_faces(SAMPLE);
        assert_eq!(subsets(&faces), ["cyrillic", "latin", "[0]"]);
        assert_eq!(faces[1].url, "https://fonts.gstatic.com/s/lora/b.woff2");
        // A comment names only the next rule: the slices of the bold weight
        // after the regular one's "latin" are not Latin.
        let faces = parse_faces(&css(&[(400, "r"), (700, "b")], 2, &["latin"]));
        assert_eq!(subsets(&faces), ["", "", "latin", "", "", "latin"]);
    }

    #[test]
    fn latin_families_keep_the_same_subsets() {
        let all = [
            "cyrillic-ext",
            "cyrillic",
            "greek-ext",
            "greek",
            "math",
            "symbols",
            "vietnamese",
            "latin-ext",
            "latin",
        ];
        let chosen = select_faces(parse_faces(&css(&[(400, "r"), (700, "b")], 0, &all))).unwrap();
        let latin = ["cyrillic-ext", "cyrillic", "greek-ext", "greek", "vietnamese", "latin-ext", "latin"];
        assert_eq!(subsets(&chosen), [latin, latin].concat());
    }

    #[test]
    fn other_scripts_are_kept_next_to_latin() {
        let chosen = select_faces(parse_faces(&css(
            &[(400, "r"), (700, "b")],
            0,
            &["arabic", "math", "symbols", "latin-ext", "latin"],
        )))
        .unwrap();
        assert_eq!(subsets(&chosen), ["arabic", "latin-ext", "latin", "arabic", "latin-ext", "latin"]);
        let chosen = select_faces(parse_faces(&css(&[(400, "r")], 0, &["devanagari", "latin-ext", "latin"]))).unwrap();
        assert_eq!(subsets(&chosen), ["devanagari", "latin-ext", "latin"]);
        // A family of symbols only keeps them.
        let chosen = select_faces(parse_faces(&css(&[(400, "r")], 0, &["symbols"]))).unwrap();
        assert_eq!(subsets(&chosen), ["symbols"]);
    }

    #[test]
    fn cjk_fonts_keep_every_slice_within_a_limit() {
        let named = ["cyrillic", "vietnamese", "latin-ext", "latin"];
        // A variable font: both weights use the same 124 files.
        let chosen = select_faces(parse_faces(&css(&[(400, "v"), (700, "v")], 120, &named))).unwrap();
        assert_eq!(chosen.len(), 248);
        assert_eq!(unique_urls(&chosen).len(), 124);
        // A static one: 248 files.
        let chosen = select_faces(parse_faces(&css(&[(400, "r"), (700, "b")], 120, &named))).unwrap();
        assert_eq!(unique_urls(&chosen).len(), 248);
        // Beyond the limit, the font is refused rather than cut short.
        let styles = [(400, "r"), (700, "b"), (400, "ri"), (700, "bi")];
        let err = select_faces(parse_faces(&css(&styles, 120, &named))).unwrap_err();
        assert_eq!(err.to_string(), "This font has too many files to embed");
    }

    #[test]
    fn resolves_asset_references() {
        let css = "src: url(cz-asset:abc.woff2) format('woff2'); src: url(cz-asset:def.woff2)";
        assert_eq!(asset_names(css), vec!["abc.woff2", "def.woff2"]);
        assert_eq!(
            resolve_css(css, &|n| format!("assets/{n}")),
            "src: url(assets/abc.woff2) format('woff2'); src: url(assets/def.woff2)"
        );
    }

    #[test]
    fn validates_family_names() {
        assert!(valid_family("Source Sans 3"));
        assert!(!valid_family("x&y=1"));
        assert!(!valid_family(""));
    }
}
