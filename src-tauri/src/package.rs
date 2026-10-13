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
//! target and renamed over it only once complete.

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
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name()?.to_string();
        let Some(asset) = name.strip_prefix("assets/") else { continue };
        if !is_valid_asset_name(asset) {
            continue;
        }
        let dest = assets_dir.join(asset);
        if dest.exists() {
            continue;
        }
        let tmp = assets_dir.join(format!(".{asset}.part"));
        {
            let mut out = BufWriter::new(File::create(&tmp)?);
            io::copy(&mut entry, &mut out)?;
            out.flush()?;
        }
        fs::rename(&tmp, &dest)?;
    }
    deck.normalize();
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

/// Writes a package atomically. Only assets referenced by the deck are
/// included, so files never accumulate unused media.
pub fn write(path: &Path, deck: &Deck, assets_dir: &Path) -> anyhow::Result<()> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let file_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "presentation".into());
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

        for name in deck.referenced_assets() {
            if !is_valid_asset_name(&name) {
                continue;
            }
            let src = assets_dir.join(&name);
            let Ok(mut f) = File::open(&src) else { continue };
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
    fs::rename(&tmp, path).or_else(|_| {
        // Some filesystems refuse to rename over an existing file.
        let _ = fs::remove_file(path);
        fs::rename(&tmp, path)
    })?;
    Ok(())
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
            content: Content::Image(ImageContent { src: "abc.png".into(), ..Default::default() }),
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
        let leftovers: Vec<_> = fs::read_dir(dir.path()).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).collect();
        assert!(leftovers.is_empty());
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
