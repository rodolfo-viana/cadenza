//! Portable clipboard snapshots. Copied media and fonts wait in an app-wide
//! folder, so a paste still works after the source presentation closes, while
//! the system clipboard only carries the content's JSON.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

use crate::document::{Document, store_asset};
use crate::model::{Content, Deck, Element, Slide};
use crate::ops::{self, Op, OpOutcome};
use crate::webfonts::{self, WebFont};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Selection {
    Elements { elements: Vec<Element> },
    Slides { slides: Vec<Slide> },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Clipboard {
    #[serde(flatten)]
    pub selection: Selection,
    /// The snapshot in the `Store` holding the copied media files.
    #[serde(default)]
    pub media: String,
    /// Only the source document may reuse its own files after a snapshot expires.
    #[serde(default)]
    pub source_doc_id: String,
    #[serde(default)]
    pub web_fonts: Vec<WebFont>,
}

/// Where copied media waits to be pasted. Every window shares it, and only
/// the latest successfully written copy is kept.
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The folder of snapshot `id`, if `id` has the form of one.
    fn dir(&self, id: &str) -> Option<PathBuf> {
        (id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit())).then(|| self.root.join(id))
    }

    /// Removes every snapshot except `id`.
    fn keep_only(&self, id: &str) {
        for entry in fs::read_dir(&self.root).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name != id && self.dir(&name).is_some() {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
}

impl Selection {
    fn deck(&self, fonts: Vec<WebFont>) -> Deck {
        Deck {
            slides: match self {
                Self::Elements { elements } => vec![Slide {
                    elements: elements.clone(),
                    ..Default::default()
                }],
                Self::Slides { slides } => slides.clone(),
            },
            web_fonts: fonts,
            ..Default::default()
        }
    }
}

impl Clipboard {
    /// Captures and publishes one copy while the caller holds the document
    /// lock shared by all windows. Failed writes preserve the previous copy.
    pub fn copy(doc: &Document, selection: Selection, store: &Store, write: impl FnOnce(String) -> anyhow::Result<()>) -> anyhow::Result<Self> {
        let clip = Self::capture(doc, selection, store)?;
        let publish = (|| {
            let text = format!("application/x-cadenza+json:{}", serde_json::to_string(&clip)?);
            write(text)
        })();
        if let Err(e) = publish {
            if let Some(dir) = store.dir(&clip.media) {
                let _ = fs::remove_dir_all(dir);
            }
            return Err(e);
        }
        store.keep_only(&clip.media);
        Ok(clip)
    }

    /// Snapshots the selection's media and the presentation's fonts.
    pub fn capture(doc: &Document, selection: Selection, store: &Store) -> anyhow::Result<Self> {
        let assets = doc.assets_dir();
        let present = |name: &String| assets.join(name).is_file();
        let copied = selection.deck(Vec::new());
        if !copied.referenced_assets().iter().all(present) {
            bail!("Some media in the selection is missing. Replace or delete it, then copy again.");
        }
        let mut names = copied.retained_assets();
        names.retain(present);
        // A font whose files are gone is already broken here; leave it behind.
        let web_fonts: Vec<WebFont> = doc.deck.web_fonts.iter().filter(|f| f.asset_refs().iter().all(present)).cloned().collect();
        names.extend(web_fonts.iter().flat_map(WebFont::asset_refs));

        let id = uuid::Uuid::new_v4().simple().to_string();
        let dir = store.root.join(&id);
        let saved = (|| -> anyhow::Result<()> {
            fs::create_dir_all(&dir)?;
            for name in &names {
                let (src, dest) = (assets.join(name), dir.join(name));
                // A hard link is instant and keeps the bytes once the source closes.
                fs::hard_link(&src, &dest).or_else(|_| fs::copy(&src, &dest).map(drop))?;
            }
            Ok(())
        })();
        if let Err(e) = saved {
            let _ = fs::remove_dir_all(&dir);
            return Err(e);
        }
        Ok(Self {
            selection,
            media: id,
            source_doc_id: doc.id.clone(),
            web_fonts,
        })
    }

    /// Imports the media, then inserts the content and fonts as one undo
    /// step. A clipboard whose media is gone never changes the document.
    pub fn paste(&self, doc: &mut Document, target: Option<String>, offset: f64, store: &Store) -> anyhow::Result<OpOutcome> {
        let mut copied = self.selection.deck(self.web_fonts.clone());
        let snapshot = store.dir(&self.media);
        let assets = doc.assets_dir();
        let mut names = BTreeMap::new();
        let required = copied.referenced_assets();
        for name in copied.retained_assets() {
            let imported = match snapshot.as_ref().map(|dir| dir.join(&name)).filter(|f| f.is_file()) {
                // Stored by content, so it cannot clash with another file of the same name.
                Some(file) => store_asset(&assets, &fs::read(&file)?, extension(&name))?,
                // Content copied within this presentation, or before media travelled with it.
                None if (self.media.is_empty() || self.source_doc_id == doc.id) && assets.join(&name).is_file() => name.clone(),
                // Missing inactive backgrounds are optional; clear the old name
                // so it cannot accidentally point at unrelated destination media.
                None if !required.contains(&name) => String::new(),
                None => bail!("The copied media is no longer available. Copy the content again."),
            };
            names.insert(name, imported);
        }
        let remap = |src: &mut String| {
            if let Some(name) = names.get(src) {
                *src = name.clone();
            }
        };
        for slide in &mut copied.slides {
            remap(&mut slide.background.image);
            remap(&mut slide.background.video);
            for el in &mut slide.elements {
                match &mut el.content {
                    Content::Image(image) => remap(&mut image.src),
                    Content::Video(video) => {
                        remap(&mut video.src);
                        remap(&mut video.poster);
                    }
                    _ => {}
                }
            }
        }
        for font in &mut copied.web_fonts {
            font.css = webfonts::resolve_css(&font.css, &|name| {
                format!("{}{}", webfonts::ASSET_PREFIX, names.get(name).map(String::as_str).unwrap_or(name))
            });
        }
        let op = match &self.selection {
            Selection::Elements { .. } => Op::InsertElements {
                slide: target.context("No slide is selected")?,
                elements: copied.slides.remove(0).elements,
                offset,
            },
            Selection::Slides { .. } => Op::InsertSlides {
                after: target,
                slides: copied.slides,
            },
        };
        let mut deck = doc.deck.clone();
        let out = ops::apply(&mut deck, op)?;
        for font in copied.web_fonts {
            if !deck.web_fonts.iter().any(|existing| existing.family == font.family) {
                deck.web_fonts.push(font);
            }
        }
        doc.apply(
            Op::ReplaceDeck {
                deck: Box::new(deck),
                label: Some(out.label.clone()),
            },
            None,
        )?;
        Ok(out)
    }
}

fn extension(name: &str) -> &str {
    Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn content(name: &str) -> Vec<u8> {
        format!("bytes of {name}").into_bytes()
    }

    fn copies_survive_source_close(copy_slides: bool) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("clipboard"));
        let mut source = Document::new(
            dir.path(),
            serde_json::from_value(json!({
                "webFonts": [{"family": "TestFont", "css": "@font-face{font-family:TestFont;src:url(cz-asset:font.woff2)}"}],
                "slides": [{
                    "background": {"kind": "image", "image": "background.png"},
                    "elements": [
                        {"content": {"kind": "image", "src": "photo.png"}},
                        {"content": {"kind": "video", "src": "movie.mp4", "poster": "poster.png"}},
                        {"content": {"kind": "image", "src": "https://example.com/remote.png"}}
                    ]
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        for name in source.deck.referenced_assets() {
            fs::write(source.assets_dir().join(&name), content(&name)).unwrap();
        }
        let selection = if copy_slides {
            Selection::Slides {
                slides: source.deck.slides.clone(),
            }
        } else {
            Selection::Elements {
                elements: source.deck.slides[0].elements.clone(),
            }
        };
        let copied = Clipboard::capture(&source, selection, &store).unwrap();
        let mut expected: std::collections::BTreeSet<Vec<u8>> = ["photo.png", "movie.mp4", "poster.png", "font.woff2"].map(content).into();
        if copy_slides {
            expected.insert(content("background.png"));
        }
        // The system clipboard gets the content without the media bytes.
        let wire = serde_json::to_string(&copied).unwrap();
        assert!(!wire.contains("bytes of"));
        let copied: Clipboard = serde_json::from_str(&wire).unwrap();
        // A cut followed by closing the source.
        source.deck.slides.clear();
        source.dispose();

        let mut dest = Document::new(dir.path(), crate::layouts::starter_deck("black", 1280, 720)).unwrap();
        // A legacy file name in the destination must not shadow the copied bytes.
        fs::write(dest.assets_dir().join("photo.png"), b"different destination image").unwrap();
        let before = dest.deck.clone();
        let target = Some(dest.deck.slides[0].id.clone());
        let out = copied.paste(&mut dest, target, 24.0, &store).unwrap();
        assert!(!out.created.is_empty());
        assert_eq!(dest.deck.web_fonts.len(), 1);
        assert_eq!(fs::read(dest.assets_dir().join("photo.png")).unwrap(), b"different destination image");
        let after = dest.deck.clone();
        dest.undo();
        assert_eq!(dest.deck, before);
        dest.redo();
        assert_eq!(dest.deck, after);

        let path = dir.path().join("destination.cadenza");
        dest.save(&path).unwrap();
        let reopened = Document::open(dir.path(), &path).unwrap();
        let actual: std::collections::BTreeSet<Vec<u8>> = reopened
            .deck
            .referenced_assets()
            .iter()
            .map(|name| fs::read(reopened.assets_dir().join(name)).unwrap())
            .collect();
        assert_eq!(actual, expected);
        assert!(!reopened.is_dirty());
        assert!(serde_json::to_string(&reopened.deck).unwrap().contains("https://example.com/remote.png"));
    }

    #[test]
    fn copied_elements_keep_media_and_fonts_after_source_closes() {
        copies_survive_source_close(false);
    }

    #[test]
    fn copied_slides_keep_backgrounds_media_and_fonts_after_source_closes() {
        copies_survive_source_close(true);
    }

    #[test]
    fn only_the_latest_copy_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("clipboard"));
        let mut doc = Document::new(dir.path(), crate::layouts::starter_deck("black", 1280, 720)).unwrap();
        let asset = doc.import_bytes(b"image", "png").unwrap();
        let elements: Vec<Element> = vec![serde_json::from_value(json!({"content": {"kind": "image", "src": asset.name}})).unwrap()];
        let first = Clipboard::copy(&doc, Selection::Elements { elements: elements.clone() }, &store, |_| Ok(())).unwrap();
        let second = Clipboard::copy(&doc, Selection::Elements { elements }, &store, |_| Ok(())).unwrap();
        assert!(!store.dir(&first.media).unwrap().exists());
        assert!(store.dir(&second.media).unwrap().join(&asset.name).is_file());
        // Within the same presentation even the replaced copy still pastes.
        let target = Some(doc.deck.slides[0].id.clone());
        first.paste(&mut doc, target, 0.0, &store).unwrap();
    }

    #[test]
    fn failed_native_write_preserves_the_previous_copy_after_source_close() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("clipboard"));
        let source = Document::new(dir.path(), crate::layouts::starter_deck("black", 1280, 720)).unwrap();
        fs::write(source.assets_dir().join("photo.png"), b"original image").unwrap();
        let image = serde_json::from_value(json!({"content": {"kind": "image", "src": "photo.png"}})).unwrap();
        let mut native_text = String::new();
        Clipboard::copy(&source, Selection::Elements { elements: vec![image] }, &store, |text| {
            native_text = text;
            Ok(())
        })
        .unwrap();
        source.dispose();
        let previous: Clipboard = serde_json::from_str(native_text.strip_prefix("application/x-cadenza+json:").unwrap()).unwrap();
        let mut destination = Document::new(dir.path(), crate::layouts::starter_deck("black", 1280, 720)).unwrap();
        let failed = Clipboard::copy(&destination, Selection::Elements { elements: vec![] }, &store, |_| {
            anyhow::bail!("Simulated native clipboard failure")
        });
        assert!(failed.is_err());
        assert_eq!(fs::read_dir(&store.root).unwrap().count(), 1);
        let target = Some(destination.deck.slides[0].id.clone());
        previous.paste(&mut destination, target, 0.0, &store).unwrap();
        let Content::Image(pasted) = &destination.deck.slides[0].elements.last().unwrap().content else {
            panic!("expected image")
        };
        assert_eq!(fs::read(destination.assets_dir().join(&pasted.src)).unwrap(), b"original image");
    }

    #[test]
    fn expired_snapshot_never_substitutes_another_documents_media() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("clipboard"));
        let source = Document::new(dir.path(), crate::layouts::starter_deck("black", 1280, 720)).unwrap();
        fs::write(source.assets_dir().join("photo.png"), b"source image").unwrap();
        let image = serde_json::from_value(json!({"content": {"kind": "image", "src": "photo.png"}})).unwrap();
        let previous = Clipboard::copy(&source, Selection::Elements { elements: vec![image] }, &store, |_| Ok(())).unwrap();
        source.dispose();
        let mut destination = Document::new(dir.path(), crate::layouts::starter_deck("black", 1280, 720)).unwrap();
        fs::write(destination.assets_dir().join("photo.png"), b"different image").unwrap();
        Clipboard::copy(&destination, Selection::Elements { elements: vec![] }, &store, |_| Ok(())).unwrap();
        let before = destination.deck.clone();
        let target = Some(destination.deck.slides[0].id.clone());
        assert!(previous.paste(&mut destination, target, 0.0, &store).is_err());
        assert_eq!(destination.deck, before);
        assert!(destination.history.undo_label().is_none());
    }

    #[test]
    fn inactive_backgrounds_preserve_available_media_and_allow_missing_media() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("clipboard"));
        let mut source = Document::new(dir.path(), crate::layouts::starter_deck("black", 1280, 720)).unwrap();
        let background = &mut source.deck.slides[0].background;
        background.kind = "color".into();
        background.image = "photo.png".into();
        background.video = "missing.mp4".into();
        fs::write(source.assets_dir().join("photo.png"), b"source image").unwrap();
        let selection = Selection::Slides {
            slides: vec![source.deck.slides[0].clone()],
        };
        let clip = Clipboard::copy(&source, selection, &store, |_| Ok(())).unwrap();
        source.dispose();
        let mut destination = Document::new(dir.path(), crate::layouts::starter_deck("black", 1280, 720)).unwrap();
        fs::write(destination.assets_dir().join("missing.mp4"), b"unrelated video").unwrap();
        clip.paste(&mut destination, None, 0.0, &store).unwrap();
        let pasted = destination.deck.slides.last().unwrap();
        assert_eq!(pasted.background.kind, "color");
        assert_eq!(fs::read(destination.assets_dir().join(&pasted.background.image)).unwrap(), b"source image");
        assert!(pasted.background.video.is_empty());
        destination.save(&dir.path().join("pasted.cadenza")).unwrap();
    }

    #[test]
    fn copying_missing_media_fails_without_leaving_a_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("clipboard"));
        let doc = Document::new(dir.path(), crate::layouts::starter_deck("black", 1280, 720)).unwrap();
        let elements = vec![serde_json::from_value(json!({"content": {"kind": "image", "src": "missing.png"}})).unwrap()];
        let err = Clipboard::capture(&doc, Selection::Elements { elements }, &store).unwrap_err();
        assert!(err.to_string().contains("missing"));
        assert_eq!(fs::read_dir(dir.path().join("clipboard")).map(|d| d.count()).unwrap_or(0), 0);
    }

    #[test]
    fn unavailable_media_never_changes_the_deck() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("clipboard"));
        let mut doc = Document::new(dir.path(), crate::layouts::starter_deck("black", 1280, 720)).unwrap();
        let target = Some(doc.deck.slides[0].id.clone());
        let before = doc.deck.clone();
        // No snapshot, a snapshot that is gone, and an id that is not one.
        for media in ["", "0123456789abcdef0123456789abcdef", "../../etc"] {
            let clipboard: Clipboard = serde_json::from_value(json!({
                "type": "elements", "media": media, "elements": [{"content": {"kind": "image", "src": "photo.png"}}]
            }))
            .unwrap();
            assert!(clipboard.paste(&mut doc, target.clone(), 0.0, &store).is_err());
            assert_eq!(doc.deck, before);
            assert!(doc.history.undo_label().is_none());
        }
    }

    #[test]
    fn legacy_clipboard_can_reuse_media_in_the_same_document() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("clipboard"));
        let mut doc = Document::new(dir.path(), crate::layouts::starter_deck("black", 1280, 720)).unwrap();
        let asset = doc.import_bytes(b"image", "png").unwrap();
        let clipboard: Clipboard = serde_json::from_value(json!({
            "type": "elements", "elements": [{"content": {"kind": "image", "src": asset.name}}]
        }))
        .unwrap();
        let target = Some(doc.deck.slides[0].id.clone());
        clipboard.paste(&mut doc, target, 0.0, &store).unwrap();
        doc.save(&dir.path().join("saved.cadenza")).unwrap();
    }
}
