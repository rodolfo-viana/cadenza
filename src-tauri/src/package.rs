//! The `.cadenza` file format.
//!
//! A `.cadenza` file is a ZIP archive:
//!
//! ```text
//! deck.json        the Deck model (pretty-printed JSON)
//! meta.json        application/version info
//! assets/<name>    images, videos and other media, named by content hash
//! ```
//!
//! Saving is atomic: the archive is written to a temporary file next to the
//! target and renamed over it only once complete. Where a filesystem refuses
//! that rename, the old file is moved aside first and put back if the new
//! one can't take its place.

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::model::{Deck, FORMAT_VERSION};
use crate::util::{is_valid_asset_name, now_ms};

pub const EXTENSION: &str = "cadenza";
const MAX_DECK_JSON: u64 = 64 * 1024 * 1024;

/// Reads a package, extracting its assets into `assets_dir`.
pub fn read(path: &Path, assets_dir: &Path) -> anyhow::Result<Deck> {
    let file = File::open(path).with_context(|| format!("Cannot open {}", path.display()))?;
    let mut zip = ZipArchive::new(BufReader::new(file)).context("This file is not a Cadenza presentation")?;
    let mut deck: Deck = {
        let entry = zip.by_name("deck.json").context("The presentation is missing deck.json")?;
        if entry.size() > MAX_DECK_JSON {
            bail!("deck.json is unreasonably large");
        }
        let mut json = String::new();
        entry.take(MAX_DECK_JSON).read_to_string(&mut json)?;
        let value: serde_json::Value = serde_json::from_str(&json).context("deck.json is corrupted")?;
        migrate(value)?
    };
    fs::create_dir_all(assets_dir)?;
    // Only the media the deck uses: anything else in the archive would take
    // disk space (and time) for nothing.
    let wanted = deck.retained_assets();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name()?.to_string();
        let Some(asset) = name.strip_prefix("assets/") else { continue };
        if !is_valid_asset_name(asset) || !wanted.contains(asset) {
            continue;
        }
        let dest = assets_dir.join(asset);
        if dest.exists() {
            continue;
        }
        // No more than the archive says the file holds.
        let size = entry.size();
        let tmp = assets_dir.join(format!(".{asset}.part"));
        {
            let mut out = BufWriter::new(File::create(&tmp)?);
            if io::copy(&mut (&mut entry).take(size), &mut out)? != size {
                bail!("The presentation file is damaged");
            }
            out.flush()?;
        }
        fs::rename(&tmp, &dest)?;
    }
    deck.normalize();
    crate::sanitize::deck(&mut deck);
    Ok(deck)
}

/// Upgrades older documents. Version 1 is the first format.
fn migrate(value: serde_json::Value) -> anyhow::Result<Deck> {
    let version = value.get("formatVersion").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
    if version > FORMAT_VERSION {
        bail!("This presentation was created by a newer version of Cadenza (format {version}). Please update Cadenza.");
    }
    Ok(serde_json::from_value(value)?)
}

/// Media the deck uses that is missing from its assets folder. Saving and
/// exporting refuse rather than write a file that silently lacks it.
#[derive(Debug, PartialEq)]
pub struct MissingMedia {
    /// 1-based numbers of the slides that use missing media.
    pub slides: Vec<usize>,
    /// Embedded fonts whose files are missing.
    pub fonts: Vec<String>,
    /// Found by an export rather than a save; the message says so.
    pub export: bool,
}

impl MissingMedia {
    pub(crate) fn find(deck: &Deck, assets_dir: &Path) -> Option<Self> {
        let missing = |name: &str| !assets_dir.join(name).is_file();
        let slides: Vec<usize> = deck
            .slides
            .iter()
            .enumerate()
            .filter(|(_, s)| s.asset_refs().into_iter().any(missing))
            .map(|(i, _)| i + 1)
            .collect();
        let fonts: Vec<String> = deck
            .web_fonts
            .iter()
            .filter(|f| f.asset_refs().iter().any(|n| missing(n)))
            .map(|f| f.family.clone())
            .collect();
        (!slides.is_empty() || !fonts.is_empty()).then_some(Self { slides, fonts, export: false })
    }

    /// The explanation shown to the user, translated with `text`.
    pub fn message(&self, text: &dyn Fn(&str) -> String) -> String {
        let mut parts = Vec::new();
        if !self.slides.is_empty() {
            let list = self.slides.iter().map(usize::to_string).collect::<Vec<_>>().join(", ");
            let template = if self.export {
                crate::tr(
                    &text,
                    "Cannot export: media is missing on these slides: {slides}. Replace or delete it, then export again.",
                )
            } else {
                crate::tr(
                    &text,
                    "Cannot save: media is missing on these slides: {slides}. Replace or delete it, then save again.",
                )
            };
            parts.push(template.replace("{slides}", &list));
        }
        if !self.fonts.is_empty() {
            let list = self.fonts.join(", ");
            let template = if self.export {
                crate::tr(
                    &text,
                    "Cannot export: the files of these embedded fonts are missing: {fonts}. Remove them under Document > Typography, then export again.",
                )
            } else {
                crate::tr(
                    &text,
                    "Cannot save: the files of these embedded fonts are missing: {fonts}. Remove them under Document > Typography, then save again.",
                )
            };
            parts.push(template.replace("{fonts}", &list));
        }
        parts.join("\n\n")
    }
}

impl std::fmt::Display for MissingMedia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message(&|s: &str| s.to_string()))
    }
}

impl std::error::Error for MissingMedia {}

/// Writes a package atomically. Only assets referenced by the deck are
/// included, so files never accumulate unused media.
pub fn write(path: &Path, deck: &Deck, assets_dir: &Path) -> anyhow::Result<()> {
    if let Some(missing) = MissingMedia::find(deck, assets_dir) {
        return Err(missing.into());
    }
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "presentation".into());
    let tmp: PathBuf = dir.join(format!(".{file_name}.{}.tmp", now_ms()));
    let result = (|| -> anyhow::Result<()> {
        let file = File::create(&tmp).with_context(|| format!("Cannot write to {}", dir.display()))?;
        let mut zip = ZipWriter::new(BufWriter::new(file));
        let deflate = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated).large_file(false);
        let store = SimpleFileOptions::default().compression_method(CompressionMethod::Stored).large_file(true);

        zip.start_file("deck.json", deflate)?;
        zip.write_all(serde_json::to_string_pretty(deck)?.as_bytes())?;

        zip.start_file("meta.json", deflate)?;
        let meta = serde_json::json!({
            "app": "Cadenza",
            "appVersion": env!("CARGO_PKG_VERSION"),
            "formatVersion": FORMAT_VERSION,
            "savedAt": now_ms(),
        });
        zip.write_all(serde_json::to_string_pretty(&meta)?.as_bytes())?;

        let required = deck.referenced_assets();
        for name in deck.retained_assets() {
            let src = assets_dir.join(&name);
            if !required.contains(&name) && !src.is_file() {
                continue;
            }
            let mut f = File::open(src).with_context(|| format!("Cannot read {name}"))?;
            // Media is already compressed; text-like assets are deflated.
            let compressible = matches!(
                name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str(),
                "svg" | "json" | "txt" | "csv" | "bmp"
            );
            zip.start_file(format!("assets/{name}"), if compressible { deflate } else { store })?;
            io::copy(&mut f, &mut zip)?;
        }
        let mut inner = zip.finish()?;
        inner.flush()?;
        inner.get_ref().sync_all()?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = replace(&tmp, path, &dir.join(format!(".{file_name}.{}.bak", now_ms()))) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    // The rename itself is kept through a power cut once the folder is synced.
    #[cfg(unix)]
    let _ = File::open(dir).and_then(|d| d.sync_all());
    Ok(())
}

/// Renames `tmp` over `path`. Some filesystems (sshfs, some network shares)
/// refuse to rename over an existing file: then the old file moves to
/// `backup` first, and back if the new one can't take its place, so a
/// failure never leaves the file's last good version missing. A folder in
/// the way is never moved.
pub(crate) fn replace(tmp: &Path, path: &Path, backup: &Path) -> io::Result<()> {
    let Err(first) = fs::rename(tmp, path) else { return Ok(()) };
    if !path.is_file() || fs::rename(path, backup).is_err() {
        return Err(first);
    }
    match fs::rename(tmp, path) {
        Ok(()) => {
            let _ = fs::remove_file(backup);
            Ok(())
        }
        Err(e) => {
            let _ = fs::rename(backup, path);
            Err(e)
        }
    }
}

/// Lists the assets stored in a package without extracting it.
#[cfg(test)]
fn asset_names(path: &Path) -> anyhow::Result<Vec<String>> {
    let mut zip = ZipArchive::new(BufReader::new(File::open(path)?))?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let e = zip.by_index(i)?;
        if let Some(a) = e.name()?.strip_prefix("assets/") {
            out.push(a.to_string());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    #[test]
    fn round_trip_with_assets() {
        let dir = tempfile::tempdir().unwrap();
        let assets = dir.path().join("assets");
        fs::create_dir_all(&assets).unwrap();
        fs::write(assets.join("abc.png"), b"png-bytes").unwrap();
        fs::write(assets.join("unused.png"), b"nope").unwrap();

        let mut deck = crate::layouts::starter_deck("black", 1280, 720);
        deck.slides[0].elements.push(Element {
            content: Content::Image(ImageContent {
                src: "abc.png".into(),
                ..Default::default()
            }),
            ..Default::default()
        });
        let file = dir.path().join("talk.cadenza");
        write(&file, &deck, &assets).unwrap();
        assert_eq!(asset_names(&file).unwrap(), vec!["abc.png".to_string()]);

        let out_assets = dir.path().join("opened");
        let back = read(&file, &out_assets).unwrap();
        assert_eq!(back, deck);
        assert_eq!(fs::read(out_assets.join("abc.png")).unwrap(), b"png-bytes");

        // Overwriting an existing file works and leaves no temp files.
        write(&file, &back, &out_assets).unwrap();
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn replacing_falls_back_without_losing_the_old_file() {
        let dir = tempfile::tempdir().unwrap();
        let (tmp, path, backup) = (dir.path().join("new.tmp"), dir.path().join("talk.cadenza"), dir.path().join("talk.bak"));
        fs::write(&path, b"old").unwrap();
        fs::write(&tmp, b"new").unwrap();
        replace(&tmp, &path, &backup).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert!(!tmp.exists() && !backup.exists());
        // The new file can't take the old one's place, even once the old
        // one is moved aside (here it is missing): the old one is put back.
        assert!(replace(&tmp, &path, &backup).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert!(!backup.exists());
    }

    #[test]
    fn only_media_the_deck_uses_is_extracted() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("extra.cadenza");
        {
            let mut zip = ZipWriter::new(File::create(&file).unwrap());
            let o = SimpleFileOptions::default();
            zip.start_file("deck.json", o).unwrap();
            zip.write_all(br#"{"slides": [{"elements": [{"content": {"kind": "image", "src": "used.png"}}]}]}"#)
                .unwrap();
            for name in ["used.png", "unused.png"] {
                zip.start_file(format!("assets/{name}"), o).unwrap();
                zip.write_all(name.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        let assets = dir.path().join("assets");
        read(&file, &assets).unwrap();
        assert_eq!(fs::read(assets.join("used.png")).unwrap(), b"used.png");
        assert!(!assets.join("unused.png").exists());
    }

    #[test]
    fn rejects_non_packages_and_future_versions() {
        let dir = tempfile::tempdir().unwrap();
        let junk = dir.path().join("junk.cadenza");
        fs::write(&junk, b"not a zip").unwrap();
        assert!(read(&junk, &dir.path().join("a")).is_err());
        assert!(migrate(serde_json::json!({"formatVersion": FORMAT_VERSION + 1})).is_err());
    }

    #[test]
    fn ignores_traversal_entries() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("evil.cadenza");
        {
            let mut zip = ZipWriter::new(File::create(&file).unwrap());
            let o = SimpleFileOptions::default();
            zip.start_file("deck.json", o).unwrap();
            zip.write_all(b"{}").unwrap();
            zip.start_file("assets/../../escape.txt", o).unwrap();
            zip.write_all(b"x").unwrap();
            zip.finish().unwrap();
        }
        let assets = dir.path().join("x/assets");
        read(&file, &assets).unwrap();
        assert!(!dir.path().join("escape.txt").exists());
        assert!(!dir.path().join("x/escape.txt").exists());
    }
}
