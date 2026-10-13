//! An open document: the deck, its media and its editing session.
//!
//! Each open document gets a private working directory
//! (`<cache>/sessions/<id>/`) holding its extracted assets plus a recovery
//! snapshot that is refreshed while editing. The directory is deleted when
//! the document is closed normally, so any directory found at startup belongs
//! to a session that crashed and can be recovered.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::model::Deck;
use crate::ops::{self, History, Op, OpOutcome};
use crate::package;
use crate::render::{self, Ctx, Mode, SlideRender};
use crate::util::{encode_uri_component, now_ms};

const RECOVERY_FILE: &str = "recovery.json";
const RECOVERY_INTERVAL_MS: u64 = 4000;

pub struct Document {
    pub id: String,
    pub deck: Deck,
    pub path: Option<PathBuf>,
    pub work_dir: PathBuf,
    pub history: History,
    saved: Deck,
    /// Last rendering sent to the UI, used to send only what changed.
    rendered: HashMap<String, SlideRender>,
    /// Monotonic view order, including saves and other changes to UI metadata.
    view_revision: u64,
    last_recovery: u64,
    /// Restored from a crash: unsaved until the next save.
    recovered: bool,
}

/// What the UI receives after every change.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DocView {
    pub doc_id: String,
    pub revision: u64,
    pub deck: Deck,
    /// Renderings of slides that changed (all slides when `full`).
    pub slides: HashMap<String, SlideRender>,
    pub full: bool,
    pub dirty: bool,
    pub path: Option<String>,
    pub file_name: String,
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
    pub outcome: Option<OpOutcome>,
    /// Stylesheet of the deck's theme (UI-relative path; empty for custom
    /// themes, which are part of `deck_css`).
    pub theme_href: &'static str,
    /// Cadenza theme variables plus the deck's custom CSS.
    pub deck_css: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct RecoveryFile {
    path: Option<PathBuf>,
    saved_at: u64,
    deck: Deck,
}

/// A crashed session that can be restored.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryInfo {
    pub session_id: String,
    pub title: String,
    pub path: Option<String>,
    pub saved_at: u64,
    pub slides: usize,
}

/// Information about an imported asset.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssetInfo {
    pub name: String,
    /// "image", "video", "audio" or "other".
    pub kind: String,
    pub width: u32,
    pub height: u32,
    pub url: String,
}

/// URL prefix of Tauri's asset protocol on this platform (the equivalent of
/// `convertFileSrc` in JavaScript).
pub fn asset_protocol_base() -> &'static str {
    if cfg!(windows) { "http://asset.localhost/" } else { "asset://localhost/" }
}

pub fn file_url(path: &Path) -> String {
    format!("{}{}", asset_protocol_base(), encode_uri_component(&path.to_string_lossy()))
}

impl Document {
    fn with_deck(sessions_root: &Path, id: String, deck: Deck, path: Option<PathBuf>) -> anyhow::Result<Self> {
        let work_dir = sessions_root.join(&id);
        fs::create_dir_all(work_dir.join("assets")).context("Cannot create the session folder")?;
        Ok(Document {
            id,
            saved: deck.clone(),
            deck,
            path,
            work_dir,
            history: History::default(),
            rendered: HashMap::new(),
            view_revision: 0,
            last_recovery: 0,
            recovered: false,
        })
    }

    pub fn new(sessions_root: &Path, mut deck: Deck) -> anyhow::Result<Self> {
        deck.normalize();
        crate::sanitize::deck(&mut deck);
        Self::with_deck(sessions_root, uuid::Uuid::new_v4().to_string(), deck, None)
    }

    pub fn open(sessions_root: &Path, path: &Path) -> anyhow::Result<Self> {
        let id = uuid::Uuid::new_v4().to_string();
        let assets = sessions_root.join(&id).join("assets");
        let deck = match package::read(path, &assets) {
            Ok(d) => d,
            Err(e) => {
                let _ = fs::remove_dir_all(sessions_root.join(&id));
                return Err(e);
            }
        };
        Self::with_deck(sessions_root, id, deck, Some(path.to_path_buf()))
    }

    /// Restores a crashed session, reusing its working directory.
    pub fn recover(sessions_root: &Path, session_id: &str) -> anyhow::Result<Self> {
        if !session_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            anyhow::bail!("invalid session");
        }
        let dir = sessions_root.join(session_id);
        let rec: RecoveryFile = serde_json::from_slice(&fs::read(dir.join(RECOVERY_FILE))?)?;
        let mut deck = rec.deck;
        deck.normalize();
        crate::sanitize::deck(&mut deck);
        let mut doc = Self::with_deck(sessions_root, session_id.to_string(), deck, rec.path.clone())?;
        doc.recovered = true;
        Ok(doc)
    }

    /// Treats the current deck as the unmodified starting point (used right
    /// after creating a document).
    pub fn reset_baseline(&mut self) {
        self.saved = self.deck.clone();
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.work_dir.join("assets")
    }

    /// Rejects work requested for a document that has since been replaced.
    pub fn ensure_id(&self, expected: &str) -> anyhow::Result<()> {
        if self.id != expected {
            anyhow::bail!("The presentation changed while the operation was running.");
        }
        Ok(())
    }

    pub fn is_dirty(&self) -> bool {
        self.recovered || self.deck != self.saved
    }

    pub fn file_name(&self) -> String {
        match &self.path {
            Some(p) => p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
            None => {
                let t = self.deck.title.trim();
                if t.is_empty() { "Untitled".into() } else { t.to_string() }
            }
        }
    }

    pub fn asset_url(&self, name: &str) -> String {
        file_url(&self.assets_dir().join(name))
    }

    fn render_all(&self) -> HashMap<String, SlideRender> {
        let assets = self.assets_dir();
        let url = move |name: &str| file_url(&assets.join(name));
        let ctx = Ctx {
            mode: Mode::Editor,
            asset_url: &url,
        };
        self.deck
            .slides
            .iter()
            .map(|s| (s.id.clone(), render::slide_for_editor(&self.deck, s, &ctx)))
            .collect()
    }

    /// Builds the view for the UI. With `full`, every slide is included;
    /// otherwise only slides whose rendering changed since the last view.
    pub fn view(&mut self, full: bool, outcome: Option<OpOutcome>) -> DocView {
        self.view_revision += 1;
        let all = self.render_all();
        let slides = if full {
            all.clone()
        } else {
            all.iter()
                .filter(|(id, r)| self.rendered.get(*id) != Some(*r))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        self.rendered = all;
        DocView {
            doc_id: self.id.clone(),
            revision: self.view_revision,
            deck: self.deck.clone(),
            slides,
            full,
            dirty: self.is_dirty(),
            path: self.path.as_ref().map(|p| p.to_string_lossy().to_string()),
            file_name: self.file_name(),
            can_undo: self.history.undo_label().is_some(),
            can_redo: self.history.redo_label().is_some(),
            undo_label: self.history.undo_label().map(str::to_string),
            redo_label: self.history.redo_label().map(str::to_string),
            outcome,
            theme_href: self.deck.theme_href().unwrap_or(""),
            deck_css: {
                let assets = self.assets_dir();
                let url = move |name: &str| file_url(&assets.join(name));
                render::deck_css(
                    &self.deck,
                    &Ctx {
                        mode: Mode::Editor,
                        asset_url: &url,
                    },
                )
            },
        }
    }

    pub fn apply(&mut self, op: Op, coalesce: Option<&str>) -> anyhow::Result<OpOutcome> {
        let before = self.deck.clone();
        let out = ops::apply(&mut self.deck, op)?;
        if self.deck != before {
            self.history.record(&before, &out, coalesce);
            self.maybe_write_recovery(false);
        }
        Ok(out)
    }

    /// Applies a change that is not an editing step of its own (e.g. caching a
    /// diagram rendering): no undo entry, redo history kept.
    pub fn apply_untracked(&mut self, op: Op) -> anyhow::Result<OpOutcome> {
        let out = ops::apply(&mut self.deck, op)?;
        self.maybe_write_recovery(false);
        Ok(out)
    }

    pub fn undo(&mut self) -> Option<OpOutcome> {
        let out = self.history.undo(&mut self.deck);
        self.maybe_write_recovery(false);
        out
    }

    pub fn redo(&mut self) -> Option<OpOutcome> {
        let out = self.history.redo(&mut self.deck);
        self.maybe_write_recovery(false);
        out
    }

    pub fn save(&mut self, path: &Path) -> anyhow::Result<()> {
        package::write(path, &self.deck, &self.assets_dir())?;
        self.path = Some(path.to_path_buf());
        self.saved = self.deck.clone();
        self.recovered = false;
        self.history.seal();
        let _ = fs::remove_file(self.work_dir.join(RECOVERY_FILE));
        Ok(())
    }

    /// Persists a recovery snapshot (throttled unless `force`).
    pub fn maybe_write_recovery(&mut self, force: bool) {
        if !self.is_dirty() {
            let _ = fs::remove_file(self.work_dir.join(RECOVERY_FILE));
            return;
        }
        let now = now_ms();
        if !force && now.saturating_sub(self.last_recovery) < RECOVERY_INTERVAL_MS {
            return;
        }
        self.last_recovery = now;
        let rec = RecoveryFile {
            path: self.path.clone(),
            saved_at: now,
            deck: self.deck.clone(),
        };
        if let Ok(json) = serde_json::to_vec(&rec) {
            let tmp = self.work_dir.join(".recovery.tmp");
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, self.work_dir.join(RECOVERY_FILE));
            }
        }
    }

    /// Removes the working directory. Called when the document is closed.
    pub fn dispose(self) {
        let _ = fs::remove_dir_all(&self.work_dir);
    }

    // -- assets -------------------------------------------------------------

    pub fn import_file(&self, src: &Path) -> anyhow::Result<AssetInfo> {
        let mut f = fs::File::open(src).with_context(|| format!("Cannot read {}", src.display()))?;
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes)?;
        let ext = src.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        self.import_bytes(&bytes, &ext)
    }

    pub fn import_bytes(&self, bytes: &[u8], ext: &str) -> anyhow::Result<AssetInfo> {
        let name = store_asset(&self.assets_dir(), bytes, ext)?;
        let ext = name.rsplit('.').next().unwrap_or("").to_string();
        let kind = asset_kind(&ext);
        let (width, height) = if kind == "image" {
            imagesize::blob_size(bytes).map(|s| (s.width as u32, s.height as u32)).unwrap_or((0, 0))
        } else {
            (0, 0)
        };
        Ok(AssetInfo {
            url: self.asset_url(&name),
            name,
            kind: kind.into(),
            width,
            height,
        })
    }
}

/// Stores bytes in an assets folder under a content-hash name (deduplicated)
/// and returns that name.
pub fn store_asset(dir: &Path, bytes: &[u8], ext: &str) -> anyhow::Result<String> {
    if bytes.is_empty() {
        anyhow::bail!("The file is empty");
    }
    let ext: String = ext
        .trim_start_matches('.')
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase();
    let ext = if ext.is_empty() { sniff_extension(bytes).to_string() } else { ext };
    let hash = hex::encode(Sha256::digest(bytes));
    let name = format!("{}.{}", &hash[..20], ext);
    let dest = dir.join(&name);
    if !dest.exists() {
        fs::create_dir_all(dir)?;
        let tmp = dir.join(format!(".{name}.{}.part", now_ms()));
        fs::write(&tmp, bytes)?;
        fs::rename(&tmp, &dest)?;
    }
    Ok(name)
}

pub fn asset_kind(ext: &str) -> &'static str {
    match ext {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "avif" | "bmp" | "ico" => "image",
        "mp4" | "webm" | "ogv" | "mov" | "m4v" | "mkv" => "video",
        "mp3" | "wav" | "ogg" | "oga" | "m4a" | "flac" | "aac" => "audio",
        _ => "other",
    }
}

fn sniff_extension(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "png",
        [0xFF, 0xD8, 0xFF, ..] => "jpg",
        [b'G', b'I', b'F', b'8', ..] => "gif",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "webp",
        [_, _, _, _, b'f', b't', b'y', b'p', ..] => "mp4",
        [0x1A, 0x45, 0xDF, 0xA3, ..] => "webm",
        _ if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") => "svg",
        _ => "bin",
    }
}

/// Lists sessions left behind by a crash that have unsaved work.
pub fn list_recoverable(sessions_root: &Path) -> Vec<RecoveryInfo> {
    let Ok(entries) = fs::read_dir(sessions_root) else { return vec![] };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        let file = dir.join(RECOVERY_FILE);
        let Some(id) = dir.file_name().map(|n| n.to_string_lossy().to_string()) else {
            continue;
        };
        match fs::read(&file).ok().and_then(|b| serde_json::from_slice::<RecoveryFile>(&b).ok()) {
            Some(rec) => out.push(RecoveryInfo {
                session_id: id,
                title: rec.deck.title.clone(),
                path: rec.path.map(|p| p.to_string_lossy().to_string()),
                saved_at: rec.saved_at,
                slides: rec.deck.slides.len(),
            }),
            // Nothing to recover: clean up.
            None => {
                let _ = fs::remove_dir_all(&dir);
            }
        }
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.saved_at));
    out
}

pub fn discard_recovery(sessions_root: &Path, session_id: &str) {
    if session_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') && !session_id.is_empty() {
        let _ = fs::remove_dir_all(sessions_root.join(session_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use serde_json::json;

    fn new_doc(root: &Path) -> Document {
        Document::new(root, crate::layouts::starter_deck("black", 1280, 720)).unwrap()
    }

    #[test]
    fn view_sends_only_changed_slides() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = new_doc(dir.path());
        doc.apply(
            Op::AddSlide {
                after: None,
                layout: "blank".into(),
            },
            None,
        )
        .unwrap();
        let full = doc.view(true, None);
        assert_eq!(full.slides.len(), 2);
        let sid = doc.deck.slides[0].id.clone();
        let eid = doc.deck.slides[0].elements[0].id.clone();
        doc.apply(
            Op::UpdateElements {
                slide: sid.clone(),
                updates: vec![ops::ElementPatch {
                    id: eid,
                    patch: json!({"x": 5}),
                }],
            },
            None,
        )
        .unwrap();
        let v = doc.view(false, None);
        assert_eq!(v.slides.keys().collect::<Vec<_>>(), vec![&sid]);
        assert!(v.dirty && v.can_undo);
    }

    #[test]
    fn dirty_tracks_saved_state() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = new_doc(dir.path());
        assert!(!doc.is_dirty());
        doc.apply(
            Op::UpdateDeck {
                patch: json!({"title": "Hello"}),
            },
            None,
        )
        .unwrap();
        assert!(doc.is_dirty());
        doc.undo();
        assert!(!doc.is_dirty());
        doc.redo();
        let file = dir.path().join("x.cadenza");
        doc.save(&file).unwrap();
        assert!(!doc.is_dirty());
        let reopened = Document::open(dir.path(), &file).unwrap();
        assert_eq!(reopened.deck.title, "Hello");
    }

    #[test]
    fn assets_are_deduplicated_and_measured() {
        let dir = tempfile::tempdir().unwrap();
        let doc = new_doc(dir.path());
        // 1x1 transparent PNG
        let png = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=",
        )
        .unwrap();
        let a = doc.import_bytes(&png, "").unwrap();
        let b = doc.import_bytes(&png, "png").unwrap();
        assert_eq!(a.name, b.name);
        assert!(a.name.ends_with(".png"));
        assert_eq!((a.width, a.height), (1, 1));
        assert_eq!(a.kind, "image");
    }

    #[test]
    fn crashed_sessions_can_be_recovered() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = new_doc(dir.path());
        doc.apply(
            Op::UpdateDeck {
                patch: json!({"title": "Unsaved work"}),
            },
            None,
        )
        .unwrap();
        doc.maybe_write_recovery(true);
        let id = doc.id.clone();
        drop(doc); // simulated crash: no dispose()
        let list = list_recoverable(dir.path());
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Unsaved work");
        let rec = Document::recover(dir.path(), &id).unwrap();
        assert!(rec.is_dirty());
        assert_eq!(rec.deck.title, "Unsaved work");
        rec.dispose();
        assert!(list_recoverable(dir.path()).is_empty());
    }

    #[test]
    fn clean_sessions_are_not_offered() {
        let dir = tempfile::tempdir().unwrap();
        let _doc = new_doc(dir.path());
        assert!(list_recoverable(dir.path()).is_empty());
        let _ = Content::default();
    }

    #[test]
    fn opened_and_recovered_documents_sanitize_all_rich_text() {
        let dir = tempfile::tempdir().unwrap();
        let markup = "<p><em>Keep</em> $x$<img src=x onerror=alert(1)><script>alert(1)</script></p>";
        let deck: Deck = serde_json::from_value(json!({"slides": [{"elements": [
            {"content": {"kind": "text", "html": markup}},
            {"content": {"kind": "shape", "text": {"html": markup}}},
            {"content": {"kind": "table", "rows": [[markup]]}}
        ]}]}))
        .unwrap();
        let path = dir.path().join("untrusted.cadenza");
        package::write(&path, &deck, dir.path()).unwrap();
        let opened = Document::open(dir.path(), &path).unwrap();
        assert!(!opened.is_dirty());
        let mut recovery = new_doc(dir.path());
        recovery.deck = deck;
        recovery.maybe_write_recovery(true);
        let recovered = Document::recover(dir.path(), &recovery.id).unwrap();
        assert!(recovered.is_dirty());
        for mut doc in [opened, recovered] {
            for el in &doc.deck.slides[0].elements {
                let html = match &el.content {
                    Content::Table(t) => &t.rows[0][0],
                    _ => &el.content.text().unwrap().html,
                };
                assert_eq!(html, "<p><em>Keep</em> $x$</p>");
            }
            let view = doc.view(true, None);
            let html = &view.slides.values().next().unwrap().html;
            assert!(!html.contains("onerror") && !html.contains("<script"));
            assert!(html.contains("cz-math-inline"));
        }
    }

    #[test]
    fn missing_media_does_not_overwrite_saved_file_or_clear_dirty_state() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = new_doc(dir.path());
        let file = dir.path().join("saved.cadenza");
        doc.save(&file).unwrap();
        let previous = fs::read(&file).unwrap();
        doc.apply(
            Op::AddElement {
                slide: doc.deck.slides[0].id.clone(),
                kind: "image".into(),
                variant: String::new(),
                props: Some(json!({"content": {"src": "missing.png"}})),
            },
            None,
        )
        .unwrap();
        let failure = doc.save(&file).unwrap_err();
        let missing = failure.downcast_ref::<package::MissingMedia>().unwrap();
        assert_eq!((missing.slides.as_slice(), missing.fonts.len()), (&[1][..], 0));
        assert!(failure.to_string().contains("slides: 1."));
        // The message is built from translated templates.
        assert_eq!(missing.message(&|_| "<{slides}>".to_string()), "<1>");
        assert!(doc.is_dirty());
        assert_eq!(fs::read(&file).unwrap(), previous);
        assert!(
            !fs::read_dir(dir.path())
                .unwrap()
                .flatten()
                .any(|e| e.path().extension().is_some_and(|x| x == "tmp"))
        );
    }

    #[test]
    fn links_that_are_not_asset_names_do_not_block_saving() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = new_doc(dir.path());
        let slide = doc.deck.slides[0].id.clone();
        for src in ["youtu.be/dQw4w9WgXcQ", "www.youtube.com/watch?v=dQw4w9WgXcQ", "vimeo.com/76979871"] {
            let props = Some(json!({"content": {"src": src}}));
            doc.apply(
                Op::AddElement {
                    slide: slide.clone(),
                    kind: "video".into(),
                    variant: String::new(),
                    props,
                },
                None,
            )
            .unwrap();
        }
        doc.save(&dir.path().join("links.cadenza")).unwrap();
        assert!(!doc.is_dirty());
    }

    #[test]
    fn replacing_missing_background_media_allows_saving_and_undo_restores_the_warning() {
        let dir = tempfile::tempdir().unwrap();
        for kind in ["image", "video"] {
            let mut doc = new_doc(dir.path());
            let id = doc.deck.slides[0].id.clone();
            doc.apply(
                Op::UpdateSlides {
                    ids: vec![id.clone()],
                    patch: json!({"background": {"kind": kind, "image": "missing.png", "video": "missing.mp4"}}),
                },
                None,
            )
            .unwrap();
            let path = dir.path().join(format!("{kind}.cadenza"));
            assert!(doc.save(&path).is_err());
            doc.apply(
                Op::UpdateSlides {
                    ids: vec![id],
                    patch: json!({"background": {"kind": "color", "color": "#ffffff"}}),
                },
                None,
            )
            .unwrap();
            doc.save(&path).unwrap();
            let reopened = Document::open(dir.path(), &path).unwrap();
            assert_eq!(reopened.deck.slides[0].background.kind, "color");
            assert!(reopened.deck.referenced_assets().is_empty());
            doc.undo();
            assert!(doc.save(&path).is_err());
        }
    }

    #[test]
    fn views_are_ordered_even_when_the_deck_does_not_change() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = new_doc(dir.path());
        let first = doc.view(true, None);
        let second = doc.view(false, None);
        doc.save(&dir.path().join("saved.cadenza")).unwrap();
        let saved = doc.view(false, None);
        assert!(first.revision < second.revision && second.revision < saved.revision);
        assert_eq!(first.deck, second.deck);
        assert!(second.slides.is_empty());
    }

    #[test]
    fn saving_preserves_available_inactive_backgrounds() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = new_doc(dir.path());
        for name in ["photo.png", "clip.mp4"] {
            fs::write(doc.assets_dir().join(name), name.as_bytes()).unwrap();
        }
        doc.apply(
            Op::UpdateSlides {
                ids: vec![doc.deck.slides[0].id.clone()],
                patch: json!({"background": {"kind": "color", "image": "photo.png", "video": "clip.mp4"}}),
            },
            None,
        )
        .unwrap();
        let path = dir.path().join("backgrounds.cadenza");
        doc.save(&path).unwrap();
        doc.dispose();
        let mut reopened = Document::open(dir.path(), &path).unwrap();
        for (kind, name) in [("image", "photo.png"), ("video", "clip.mp4")] {
            assert_eq!(fs::read(reopened.assets_dir().join(name)).unwrap(), name.as_bytes());
            reopened
                .apply(
                    Op::UpdateSlides {
                        ids: vec![reopened.deck.slides[0].id.clone()],
                        patch: json!({"background": {"kind": kind}}),
                    },
                    None,
                )
                .unwrap();
            reopened.save(&path).unwrap();
        }
    }
}
