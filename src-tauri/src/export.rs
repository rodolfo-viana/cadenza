//! Exporting presentations as standalone web pages.
//!
//! * **Folder**: `index.html` + `lib/` (reveal.js, theme, helpers) +
//!   `assets/`. Works offline and can be hosted on any static web server.
//! * **Single file**: one self-contained `.html` with every library and
//!   asset inlined — ideal for e-mailing a presentation.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use base64::Engine;

use crate::model::Deck;
use crate::render::{self, Ctx, LibLinker, Mode};
use crate::util::{esc, is_valid_asset_name};

/// Reads a file of the bundled UI (e.g. `vendor/reveal/reveal.js`).
pub type UiReader<'a> = &'a dyn Fn(&str) -> Option<Vec<u8>>;

struct FolderLinker {
    used: RefCell<BTreeSet<String>>,
}

impl LibLinker for FolderLinker {
    fn stylesheet(&self, path: &str) -> String {
        self.used.borrow_mut().insert(path.to_string());
        format!("<link rel=\"stylesheet\" href=\"lib/{}\">\n", esc(path))
    }
    fn script(&self, path: &str) -> String {
        self.used.borrow_mut().insert(path.to_string());
        format!("<script src=\"lib/{}\"></script>\n", esc(path))
    }
}

struct InlineLinker<'a> {
    read: UiReader<'a>,
}

impl LibLinker for InlineLinker<'_> {
    fn stylesheet(&self, path: &str) -> String {
        let Some(bytes) = (self.read)(path) else { return String::new() };
        let mut css = String::from_utf8_lossy(&bytes).to_string();
        css = inline_css_urls(&css, path, self.read);
        format!("<style>\n{}\n</style>\n", css.replace("</style", "<\\/style"))
    }
    fn script(&self, path: &str) -> String {
        let Some(bytes) = (self.read)(path) else { return String::new() };
        let js = String::from_utf8_lossy(&bytes).replace("</script", "<\\/script");
        format!("<script>\n{js}\n</script>\n")
    }
}

/// Relative `url(...)` references inside a stylesheet (fonts, images).
fn css_relative_urls(css: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = css;
    while let Some(i) = rest.find("url(") {
        rest = &rest[i + 4..];
        let Some(end) = rest.find(')') else { break };
        let raw = rest[..end].trim().trim_matches(|c| c == '"' || c == '\'');
        if !raw.is_empty() && !raw.starts_with("data:") && !raw.contains("://") && !raw.starts_with('#') && !raw.starts_with('/') {
            out.push(raw.to_string());
        }
        rest = &rest[end..];
    }
    out
}

fn resolve_relative(base_file: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = base_file.split('/').collect();
    parts.pop();
    for seg in rel.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn inline_css_urls(css: &str, css_path: &str, read: UiReader) -> String {
    let mut out = css.to_string();
    for rel in css_relative_urls(css) {
        let clean = rel.split(['?', '#']).next().unwrap_or(&rel);
        if let Some(bytes) = read(&resolve_relative(css_path, clean)) {
            let mime = mime_guess::from_path(clean).first_or_octet_stream().to_string();
            let data = format!("data:{};base64,{}", mime, base64::engine::general_purpose::STANDARD.encode(bytes));
            out = out.replace(&rel, &data);
        }
    }
    out
}

/// Exports to a folder; returns the path of `index.html`.
pub fn export_folder(deck: &Deck, assets_dir: &Path, dir: &Path, read: UiReader) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(dir).with_context(|| format!("Cannot create {}", dir.display()))?;
    let url = |name: &str| format!("assets/{name}");
    let ctx = Ctx { mode: Mode::Show, asset_url: &url };
    let linker = FolderLinker { used: RefCell::new(BTreeSet::new()) };
    let html = render::document(deck, &ctx, &linker);

    for path in linker.used.borrow().iter() {
        let bytes = read(path).with_context(|| format!("Missing bundled file {path}"))?;
        let dest = dir.join("lib").join(path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        // Fonts and images referenced by stylesheets travel along.
        if path.ends_with(".css") {
            for rel in css_relative_urls(&String::from_utf8_lossy(&bytes)) {
                let clean = rel.split(['?', '#']).next().unwrap_or(&rel).to_string();
                let src = resolve_relative(path, &clean);
                if let Some(b) = read(&src) {
                    let d = dir.join("lib").join(&src);
                    if let Some(p) = d.parent() {
                        fs::create_dir_all(p)?;
                    }
                    fs::write(d, b)?;
                }
            }
        }
        fs::write(dest, bytes)?;
    }

    let assets = deck.referenced_assets();
    if !assets.is_empty() {
        fs::create_dir_all(dir.join("assets"))?;
    }
    for name in assets {
        if is_valid_asset_name(&name) {
            let src = assets_dir.join(&name);
            if src.exists() {
                fs::copy(&src, dir.join("assets").join(&name))?;
            }
        }
    }
    let index = dir.join("index.html");
    fs::write(&index, html)?;
    Ok(index)
}

/// Exports a single self-contained HTML file.
pub fn export_single(deck: &Deck, assets_dir: &Path, file: &Path, read: UiReader) -> anyhow::Result<()> {
    let url = |name: &str| -> String {
        if !is_valid_asset_name(name) {
            return String::new();
        }
        match fs::read(assets_dir.join(name)) {
            Ok(bytes) => {
                let mime = mime_guess::from_path(name).first_or_octet_stream().to_string();
                format!("data:{};base64,{}", mime, base64::engine::general_purpose::STANDARD.encode(bytes))
            }
            Err(_) => String::new(),
        }
    };
    let ctx = Ctx { mode: Mode::Show, asset_url: &url };
    let html = render::document(deck, &ctx, &InlineLinker { read });
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(file, html).with_context(|| format!("Cannot write {}", file.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn fake_ui(path: &str) -> Option<Vec<u8>> {
        match path {
            "vendor/katex/katex.min.css" => Some(b"@font-face{src:url(fonts/KaTeX_Main.woff2) format(\"woff2\")}".to_vec()),
            "vendor/katex/fonts/KaTeX_Main.woff2" => Some(b"FONT".to_vec()),
            p if p.ends_with(".css") || p.ends_with(".js") => Some(format!("/* {p} */").into_bytes()),
            _ => None,
        }
    }

    fn deck() -> Deck {
        let mut deck = crate::layouts::starter_deck("cadenza-midnight", 1280, 720);
        deck.slides[0].elements.push(Element {
            content: Content::Image(ImageContent { src: "abc.png".into(), ..Default::default() }),
            ..Default::default()
        });
        deck.slides[0].elements.push(Element { content: Content::Math(MathContent::default()), ..Default::default() });
        deck
    }

    #[test]
    fn folder_export_copies_libs_fonts_and_assets() {
        let dir = tempfile::tempdir().unwrap();
        let assets = dir.path().join("assets-src");
        fs::create_dir_all(&assets).unwrap();
        fs::write(assets.join("abc.png"), b"PNG").unwrap();
        let out = dir.path().join("site");
        let index = export_folder(&deck(), &assets, &out, &fake_ui).unwrap();
        let html = fs::read_to_string(index).unwrap();
        assert!(html.contains("lib/vendor/reveal/reveal.js"));
        assert!(html.contains("assets/abc.png"));
        assert!(out.join("lib/vendor/katex/fonts/KaTeX_Main.woff2").exists());
        assert!(out.join("lib/themes/cadenza-midnight.css").exists());
        assert_eq!(fs::read(out.join("assets/abc.png")).unwrap(), b"PNG");
    }

    #[test]
    fn single_file_inlines_everything() {
        let dir = tempfile::tempdir().unwrap();
        let assets = dir.path().join("a");
        fs::create_dir_all(&assets).unwrap();
        fs::write(assets.join("abc.png"), b"PNG").unwrap();
        let file = dir.path().join("talk.html");
        export_single(&deck(), &assets, &file, &fake_ui).unwrap();
        let html = fs::read_to_string(file).unwrap();
        assert!(!html.contains("src=\"lib/"));
        assert!(html.contains("data:image/png;base64,UE5H"));
        assert!(html.contains("data:font/woff2;base64,Rk9OVA=="));
        assert!(html.contains("/* vendor/reveal/reveal.js */"));
    }

    #[test]
    fn relative_paths_resolve() {
        assert_eq!(resolve_relative("vendor/katex/katex.min.css", "fonts/a.woff2"), "vendor/katex/fonts/a.woff2");
        assert_eq!(resolve_relative("a/b/c.css", "../x.png"), "a/x.png");
    }
}
