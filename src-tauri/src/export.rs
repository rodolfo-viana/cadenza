//! Exporting presentations as standalone web pages.
//!
//! * **Folder**: `index.html` + `lib/` (reveal.js, theme, helpers) +
//!   `assets/`. Works offline and can be hosted on any static web server.
//! * **Single file**: one self-contained `.html` with every library and
//!   asset inlined — ideal for e-mailing a presentation.
//!
//! Like saving, exporting refuses when media is missing, and it writes next
//! to the destination first, so a failed export leaves an earlier one as it
//! was.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::Context;
use base64::Engine;

use crate::model::Deck;
use crate::package::MissingMedia;
use crate::render::{self, Ctx, LibLinker, Mode};
use crate::util::{esc, is_valid_asset_name, temp_path};

/// Reads a file of the bundled UI (e.g. `vendor/reveal/reveal.js`).
pub type UiReader<'a> = &'a dyn Fn(&str) -> Option<Vec<u8>>;

/// A file of the bundled UI that the export cannot do without.
fn bundled(read: UiReader, path: &str) -> anyhow::Result<Vec<u8>> {
    read(path)
        .ok_or_else(|| anyhow::anyhow!("Missing bundled file {path}"))
        .context("Cadenza is missing some of its own files. Reinstall Cadenza, then export again.")
}

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
    /// The first library that could not be read: links return markup only.
    failure: RefCell<Option<anyhow::Error>>,
}

impl InlineLinker<'_> {
    fn bytes(&self, path: &str) -> Option<Vec<u8>> {
        match bundled(self.read, path) {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                self.failure.borrow_mut().get_or_insert(e);
                None
            }
        }
    }
}

impl LibLinker for InlineLinker<'_> {
    fn stylesheet(&self, path: &str) -> String {
        let Some(bytes) = self.bytes(path) else { return String::new() };
        let mut css = String::from_utf8_lossy(&bytes).to_string();
        css = inline_css_urls(&css, path, self.read);
        format!("<style>\n{}\n</style>\n", css.replace("</style", "<\\/style"))
    }
    fn script(&self, path: &str) -> String {
        let Some(bytes) = self.bytes(path) else { return String::new() };
        let js = String::from_utf8_lossy(&bytes).replace("</script", "<\\/script");
        format!("<script>\n{js}\n</script>\n")
    }
}

/// Relative `url(...)` references inside a stylesheet (fonts, images). Some
/// are fallbacks that are not bundled (KaTeX lists .woff and .ttf after
/// .woff2), so exports skip those that are missing.
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

/// Where to write before renaming over `path`: a hidden, unique path next
/// to it, on the same file system.
fn staging_path(path: &Path) -> anyhow::Result<PathBuf> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).with_context(|| format!("Cannot create {}", parent.display()))?;
    }
    Ok(temp_path(path))
}

fn replace(from: &Path, to: &Path) -> io::Result<()> {
    crate::package::replace(from, to, &temp_path(to))
}

/// Moves a staged export into place. A folder that exists already may hold
/// more than an earlier export, so files are moved in one by one, replacing
/// those of the same name and keeping the others; `index.html` goes last,
/// once what it loads is there.
fn move_into(from: &Path, to: &Path) -> io::Result<()> {
    if !from.is_dir() || !to.is_dir() {
        return replace(from, to);
    }
    let mut entries = fs::read_dir(from)?.collect::<io::Result<Vec<_>>>()?;
    entries.sort_by_key(|e| e.file_name() == "index.html");
    for entry in entries {
        move_into(&entry.path(), &to.join(entry.file_name()))?;
    }
    Ok(())
}

/// Exports to a folder; returns the path of `index.html`.
pub fn export_folder(deck: &Deck, assets_dir: &Path, dir: &Path, read: UiReader) -> anyhow::Result<PathBuf> {
    if let Some(missing) = MissingMedia::find(deck, assets_dir) {
        return Err(MissingMedia { export: true, ..missing }.into());
    }
    let stage = staging_path(dir)?;
    let result = write_folder(deck, assets_dir, &stage, read).and_then(|()| move_into(&stage, dir).with_context(|| format!("Cannot write {}", dir.display())));
    let _ = fs::remove_dir_all(&stage);
    result.map(|()| dir.join("index.html"))
}

fn write_folder(deck: &Deck, assets_dir: &Path, dir: &Path, read: UiReader) -> anyhow::Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("Cannot create {}", dir.display()))?;
    let url = |name: &str| format!("assets/{name}");
    let ctx = Ctx {
        mode: Mode::Export,
        asset_url: &url,
        youtube_host: None,
    };
    let linker = FolderLinker {
        used: RefCell::new(BTreeSet::new()),
    };
    let html = render::document(deck, &ctx, &linker);

    for path in linker.used.borrow().iter() {
        let bytes = bundled(read, path)?;
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
            fs::copy(assets_dir.join(&name), dir.join("assets").join(&name)).with_context(|| format!("Cannot read {name}"))?;
        }
    }
    fs::write(dir.join("index.html"), html)?;
    Ok(())
}

/// Exports a single self-contained HTML file.
pub fn export_single(deck: &Deck, assets_dir: &Path, file: &Path, read: UiReader) -> anyhow::Result<()> {
    if let Some(missing) = MissingMedia::find(deck, assets_dir) {
        return Err(MissingMedia { export: true, ..missing }.into());
    }
    let unreadable = RefCell::new(None);
    let url = |name: &str| -> String {
        if !is_valid_asset_name(name) {
            return String::new();
        }
        match fs::read(assets_dir.join(name)) {
            Ok(bytes) => {
                let mime = mime_guess::from_path(name).first_or_octet_stream().to_string();
                format!("data:{};base64,{}", mime, base64::engine::general_purpose::STANDARD.encode(bytes))
            }
            Err(e) => {
                unreadable
                    .borrow_mut()
                    .get_or_insert(anyhow::Error::from(e).context(format!("Cannot read {name}")));
                String::new()
            }
        }
    };
    let ctx = Ctx {
        mode: Mode::Export,
        asset_url: &url,
        youtube_host: None,
    };
    let linker = InlineLinker {
        read,
        failure: RefCell::new(None),
    };
    let html = render::document(deck, &ctx, &linker);
    if let Some(e) = linker.failure.take().or_else(|| unreadable.take()) {
        return Err(e);
    }
    let tmp = staging_path(file)?;
    let result = fs::write(&tmp, html).and_then(|()| replace(&tmp, file));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.with_context(|| format!("Cannot write {}", file.display()))
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
            content: Content::Image(ImageContent {
                src: "abc.png".into(),
                ..Default::default()
            }),
            ..Default::default()
        });
        deck.slides[0].elements.push(Element {
            content: Content::Math(MathContent::default()),
            ..Default::default()
        });
        deck
    }

    fn with_font(mut deck: Deck) -> Deck {
        deck.web_fonts.push(crate::webfonts::WebFont {
            family: "Lora".into(),
            source: "google".into(),
            css: "@font-face{font-family:'Lora';src:url(cz-asset:lora.woff2) format('woff2')}".into(),
        });
        deck
    }

    /// Everything in `dir`, with contents, to tell whether an export changed it.
    fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(snapshot(&path));
                out.push((path, Vec::new()));
            } else {
                out.push((path.clone(), fs::read(&path).unwrap()));
            }
        }
        out.sort();
        out
    }

    #[test]
    fn folder_export_copies_libs_fonts_and_assets() {
        let dir = tempfile::tempdir().unwrap();
        let assets = dir.path().join("assets-src");
        fs::create_dir_all(&assets).unwrap();
        fs::write(assets.join("abc.png"), b"PNG").unwrap();
        fs::write(assets.join("lora.woff2"), b"LORA").unwrap();
        let out = dir.path().join("site");
        let index = export_folder(&with_font(deck()), &assets, &out, &fake_ui).unwrap();
        let html = fs::read_to_string(index).unwrap();
        assert!(html.contains("lib/vendor/reveal/reveal.js"));
        assert!(html.contains("assets/abc.png"));
        assert!(out.join("lib/vendor/katex/fonts/KaTeX_Main.woff2").exists());
        assert!(out.join("lib/themes/cadenza-midnight.css").exists());
        assert_eq!(fs::read(out.join("assets/abc.png")).unwrap(), b"PNG");
        // Every library and asset the page loads is there.
        for quote in html.split('"').skip(1).step_by(2) {
            if quote.starts_with("lib/") || quote.starts_with("assets/") {
                assert!(out.join(quote).is_file(), "{quote}");
            }
        }
        assert!(html.contains("assets/lora.woff2") && out.join("assets/lora.woff2").is_file());
    }

    #[test]
    fn missing_media_fails_exports_and_keeps_earlier_ones() {
        let dir = tempfile::tempdir().unwrap();
        let assets = dir.path().join("a");
        fs::create_dir_all(&assets).unwrap();
        fs::write(assets.join("abc.png"), b"PNG").unwrap();
        fs::write(assets.join("lora.woff2"), b"LORA").unwrap();
        let deck = with_font(deck());
        let site = dir.path().join("site");
        let file = dir.path().join("talk.html");
        export_folder(&deck, &assets, &site, &fake_ui).unwrap();
        export_single(&deck, &assets, &file, &fake_ui).unwrap();
        let before = snapshot(dir.path());

        for (gone, slides, fonts) in [("abc.png", vec![1], vec![]), ("lora.woff2", vec![], vec!["Lora".to_string()])] {
            fs::rename(assets.join(gone), dir.path().join(gone)).unwrap();
            for result in [
                export_folder(&deck, &assets, &site, &fake_ui).map(|_| ()),
                export_single(&deck, &assets, &file, &fake_ui),
            ] {
                let e = result.unwrap_err();
                let missing = e.downcast_ref::<MissingMedia>().expect("missing media");
                assert_eq!((&missing.slides, &missing.fonts, missing.export), (&slides, &fonts, true));
                assert!(e.to_string().starts_with("Cannot export:"), "{e}");
            }
            fs::rename(dir.path().join(gone), assets.join(gone)).unwrap();
            assert_eq!(snapshot(dir.path()), before);
        }
    }

    #[test]
    fn missing_libraries_fail_exports_and_keep_earlier_ones() {
        let dir = tempfile::tempdir().unwrap();
        let assets = dir.path().join("a");
        fs::create_dir_all(&assets).unwrap();
        fs::write(assets.join("abc.png"), b"PNG").unwrap();
        let site = dir.path().join("site");
        let file = dir.path().join("talk.html");
        export_folder(&deck(), &assets, &site, &fake_ui).unwrap();
        export_single(&deck(), &assets, &file, &fake_ui).unwrap();
        let before = snapshot(dir.path());
        for lib in ["js/runtime.js", "vendor/katex/katex.min.css"] {
            let broken = |p: &str| if p == lib { None } else { fake_ui(p) };
            for result in [
                export_folder(&deck(), &assets, &site, &broken).map(|_| ()),
                export_single(&deck(), &assets, &file, &broken),
            ] {
                let e = result.unwrap_err();
                assert!(e.to_string().contains("Reinstall Cadenza"), "{e}");
                assert!(format!("{e:#}").contains(lib));
            }
            assert_eq!(snapshot(dir.path()), before);
        }
    }

    #[test]
    fn exporting_again_replaces_the_export_and_keeps_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let assets = dir.path().join("a");
        fs::create_dir_all(&assets).unwrap();
        fs::write(assets.join("abc.png"), b"PNG").unwrap();
        let site = dir.path().join("site");
        let file = dir.path().join("talk.html");
        export_folder(&deck(), &assets, &site, &fake_ui).unwrap();
        export_single(&deck(), &assets, &file, &fake_ui).unwrap();
        fs::write(site.join("README.txt"), b"mine").unwrap();
        fs::write(assets.join("abc.png"), b"NEW").unwrap();
        let mut deck = deck();
        deck.title = "Second".into();
        export_folder(&deck, &assets, &site, &fake_ui).unwrap();
        export_single(&deck, &assets, &file, &fake_ui).unwrap();
        assert!(fs::read_to_string(site.join("index.html")).unwrap().contains("<title>Second</title>"));
        assert_eq!(fs::read(site.join("assets/abc.png")).unwrap(), b"NEW");
        assert_eq!(fs::read(site.join("README.txt")).unwrap(), b"mine");
        assert!(fs::read_to_string(&file).unwrap().contains("<title>Second</title>"));
        let names: Vec<_> = fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(names.len(), 3, "{names:?}");
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
    fn a_folder_in_the_way_is_never_removed() {
        let dir = tempfile::tempdir().unwrap();
        let (file, folder) = (dir.path().join("new.html"), dir.path().join("index.html"));
        fs::write(&file, b"new").unwrap();
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("keep.txt"), b"mine").unwrap();
        assert!(replace(&file, &folder).is_err());
        assert_eq!(fs::read(folder.join("keep.txt")).unwrap(), b"mine");
        assert_eq!(fs::read(&file).unwrap(), b"new");
    }

    #[test]
    fn relative_paths_resolve() {
        assert_eq!(resolve_relative("vendor/katex/katex.min.css", "fonts/a.woff2"), "vendor/katex/fonts/a.woff2");
        assert_eq!(resolve_relative("a/b/c.css", "../x.png"), "a/x.png");
    }
}
