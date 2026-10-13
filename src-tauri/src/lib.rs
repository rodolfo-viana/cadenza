//! Cadenza — a presentation studio built on reveal.js.
//!
//! The Rust side owns the document model, editing operations and undo
//! history, the `.cadenza` file format, the asset pipeline, slide rendering,
//! exports and window management. The UI (in `ui/`) runs in the system
//! webview and talks to it through the commands defined here.

pub mod ai;
pub mod browse;
pub mod chart;
pub mod clipboard;
pub mod custom_theme;
pub mod document;
pub mod export;
pub mod fonts;
pub mod layouts;
pub mod markdown;
pub mod mathtext;
pub mod media;
pub mod model;
pub mod ops;
pub mod package;
pub mod render;
pub mod sanitize;
pub mod secrets;
pub mod settings;
pub mod shapes;
pub mod themes;
pub mod util;
pub mod webfonts;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent};
use tauri_plugin_clipboard_manager::ClipboardExt;

use document::{AssetInfo, DocView, Document, RecoveryInfo};
use ops::Op;
use render::{Ctx, Mode};
use settings::Settings;

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Shared application state.
pub struct AppState {
    /// Open documents keyed by editor window label.
    docs: Mutex<HashMap<String, Document>>,
    sessions_root: PathBuf,
    config_dir: PathBuf,
    settings: Mutex<Settings>,
    settings_path: PathBuf,
    /// Crashed sessions found at startup.
    recoverable: Mutex<Vec<RecoveryInfo>>,
    /// Files to open in freshly created editor windows.
    pending_open: Mutex<HashMap<String, PathBuf>>,
    window_counter: AtomicU32,
    secrets: secrets::Secrets,
    /// Media browser helper processes, per editor window.
    browsers: Mutex<HashMap<String, (u64, std::process::Child)>>,
    /// Cancel flags of running AI requests, per editor window.
    ai_runs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Media of the latest copy, shared by every window.
    clipboard: clipboard::Store,
}

impl AppState {
    fn with_doc<T>(&self, label: &str, f: impl FnOnce(&mut Document) -> anyhow::Result<T>) -> CmdResult<T> {
        let mut docs = self.docs.lock().map_err(err)?;
        let doc = docs.get_mut(label).ok_or("No presentation is open in this window")?;
        f(doc).map_err(err)
    }

    fn with_doc_id<T>(&self, label: &str, doc_id: &str, f: impl FnOnce(&mut Document) -> anyhow::Result<T>) -> CmdResult<T> {
        self.with_doc(label, |doc| {
            doc.ensure_id(doc_id)?;
            f(doc)
        })
    }

    fn cancel_ai(&self, label: &str) {
        if let Ok(mut runs) = self.ai_runs.lock() {
            if let Some(cancel) = runs.remove(label) {
                cancel.store(true, Ordering::SeqCst);
            }
        }
    }

    fn remember_recent(&self, path: &Path) {
        if let Ok(mut s) = self.settings.lock() {
            s.add_recent(path);
            let _ = s.save(&self.settings_path);
        }
    }
}

/// The editor window that owns a presentation/presenter/print window.
fn owner_label(label: &str) -> &str {
    label
        .strip_prefix("present-")
        .or_else(|| label.strip_prefix("presenter-"))
        .or_else(|| label.strip_prefix("print-"))
        .or_else(|| label.strip_prefix("browse-"))
        .unwrap_or(label)
}

// ---------------------------------------------------------------------------
// App info & settings
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: &'static str,
    platform: &'static str,
    themes: &'static [themes::ThemeInfo],
    layouts: &'static [layouts::LayoutInfo],
    shapes: Vec<Value>,
    charts: Vec<Value>,
    settings: Settings,
    recent: Vec<settings::RecentFile>,
    recoverable: Vec<RecoveryInfo>,
    asset_base: &'static str,
}

#[tauri::command]
fn app_info(state: State<AppState>) -> CmdResult<AppInfo> {
    let settings = state.settings.lock().map_err(err)?.clone();
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        platform: if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "linux"
        },
        themes: themes::THEMES,
        layouts: layouts::LAYOUTS,
        shapes: shapes::SHAPES.iter().map(|(id, name)| json!({"id": id, "name": name})).collect(),
        charts: chart::CHART_TYPES.iter().map(|(id, name)| json!({"id": id, "name": name})).collect(),
        recent: settings.existing_recent(),
        settings,
        recoverable: state.recoverable.lock().map_err(err)?.clone(),
        asset_base: document::asset_protocol_base(),
    })
}

#[tauri::command]
fn settings_update(state: State<AppState>, patch: Value) -> CmdResult<Settings> {
    let mut s = state.settings.lock().map_err(err)?;
    let mut v = serde_json::to_value(&*s).map_err(err)?;
    ops::merge_patch(&mut v, &patch);
    *s = serde_json::from_value(v).map_err(err)?;
    s.save(&state.settings_path).map_err(err)?;
    Ok(s.clone())
}

#[tauri::command]
fn recent_list(state: State<AppState>) -> CmdResult<Vec<settings::RecentFile>> {
    Ok(state.settings.lock().map_err(err)?.existing_recent())
}

#[tauri::command]
fn recent_remove(state: State<AppState>, path: String) -> CmdResult<()> {
    let mut s = state.settings.lock().map_err(err)?;
    s.remove_recent(&path);
    s.save(&state.settings_path).map_err(err)
}

#[tauri::command]
async fn fonts_list() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(|| fonts::families().to_vec()).await.unwrap_or_default()
}

/// Theme metadata plus the extra CSS Cadenza injects for it.
#[tauri::command]
fn theme_css(theme: String) -> String {
    themes::vars_css(&theme)
}

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

fn install_doc(state: &AppState, label: &str, mut doc: Document) -> CmdResult<DocView> {
    let view = doc.view(true, None);
    let mut docs = state.docs.lock().map_err(err)?;
    state.cancel_ai(label);
    if let Some(old) = docs.insert(label.to_string(), doc) {
        old.dispose();
    }
    Ok(view)
}

#[tauri::command]
fn doc_new(window: Window, state: State<AppState>, theme: Option<String>, aspect: Option<String>) -> CmdResult<DocView> {
    let s = state.settings.lock().map_err(err)?.clone();
    let theme = theme.unwrap_or(s.default_theme.clone());
    let (w, h) = settings::aspect_size(aspect.as_deref().unwrap_or(&s.default_aspect));
    let deck = layouts::starter_deck(&theme, w, h);
    let mut doc = Document::new(&state.sessions_root, deck).map_err(err)?;
    if let Some(def) = custom_theme::find(&state.config_dir, &theme) {
        apply_theme_to(&mut doc, def).map_err(err)?;
        doc.reset_baseline();
    }
    install_doc(&state, window.label(), doc)
}

// ---------------------------------------------------------------------------
// Custom themes (Theme Builder)
// ---------------------------------------------------------------------------

/// Library themes without their (large) embedded font data.
fn lite(mut def: custom_theme::ThemeDef) -> custom_theme::ThemeDef {
    for f in &mut def.fonts {
        f.css.clear();
    }
    def
}

#[tauri::command]
fn themes_user(state: State<AppState>) -> Vec<custom_theme::ThemeDef> {
    custom_theme::list(&state.config_dir).into_iter().map(lite).collect()
}

#[tauri::command]
fn theme_from_builtin(id: String) -> custom_theme::ThemeDef {
    custom_theme::from_builtin(&id)
}

/// A theme with its fonts: from the open presentation (fonts the user just
/// picked) and from the library copy.
fn full_theme(state: &AppState, label: &str, mut def: custom_theme::ThemeDef) -> custom_theme::ThemeDef {
    def.normalize();
    let mut fonts = custom_theme::find(&state.config_dir, &def.id).map(|t| t.fonts).unwrap_or_default();
    if let Ok(docs) = state.docs.lock() {
        if let Some(doc) = docs.get(label) {
            let mut from_deck = def.clone();
            custom_theme::embed_fonts(&mut from_deck, &doc.deck.web_fonts, &doc.assets_dir());
            for f in from_deck.fonts {
                fonts.retain(|x| x.family != f.family);
                fonts.push(f);
            }
        }
    }
    let used: Vec<String> = [&def.heading_font, &def.body_font]
        .iter()
        .map(|f| f.split(',').next().unwrap_or("").trim().trim_matches(|c| c == '\'' || c == '"').to_string())
        .collect();
    fonts.retain(|f| used.contains(&f.family));
    def.fonts = fonts;
    def
}

#[tauri::command]
fn theme_save(window: Window, state: State<AppState>, def: custom_theme::ThemeDef) -> CmdResult<custom_theme::ThemeDef> {
    let def = full_theme(&state, window.label(), def);
    custom_theme::save(&state.config_dir, &def).map_err(err)?;
    Ok(lite(def))
}

#[tauri::command]
fn theme_delete(state: State<AppState>, id: String) -> CmdResult<()> {
    custom_theme::delete(&state.config_dir, &id).map_err(err)
}

fn apply_theme_to(doc: &mut Document, def: custom_theme::ThemeDef) -> anyhow::Result<ops::OpOutcome> {
    let dir = doc.assets_dir();
    let fonts = custom_theme::extract_fonts(&def, &|bytes: &[u8]| document::store_asset(&dir, bytes, "woff2"))?;
    let mut web_fonts = doc.deck.web_fonts.clone();
    for f in fonts {
        web_fonts.retain(|x| x.family != f.family);
        web_fonts.push(f);
    }
    let mut def = def;
    def.fonts.clear();
    let id = def.id.clone();
    // The patch carries the complete definition, so it replaces any previous
    // custom theme (arrays and fields alike) in a single undo step.
    doc.apply(
        Op::UpdateDeck {
            patch: json!({ "customTheme": def, "theme": id, "webFonts": web_fonts }),
        },
        None,
    )
}

#[tauri::command]
fn theme_apply(window: Window, state: State<AppState>, def: custom_theme::ThemeDef) -> CmdResult<DocView> {
    let def = full_theme(&state, window.label(), def);
    state.with_doc(window.label(), |doc| {
        let out = apply_theme_to(doc, def)?;
        Ok(doc.view(false, Some(out)))
    })
}

#[tauri::command]
fn theme_export(window: Window, state: State<AppState>, def: custom_theme::ThemeDef, path: String, css: bool) -> CmdResult<String> {
    let def = full_theme(&state, window.label(), def);
    let mut target = PathBuf::from(path);
    if css {
        if target.extension().is_none() {
            target.set_extension("css");
        }
        std::fs::write(&target, custom_theme::reveal_css(&def)).map_err(err)?;
    } else {
        if target.extension().is_none() {
            target.set_extension(custom_theme::EXTENSION);
        }
        custom_theme::write_file(&target, &def).map_err(err)?;
    }
    Ok(target.to_string_lossy().to_string())
}

#[tauri::command]
fn theme_import(state: State<AppState>, path: String) -> CmdResult<custom_theme::ThemeDef> {
    let mut def = custom_theme::read_file(Path::new(&path)).map_err(err)?;
    if custom_theme::find(&state.config_dir, &def.id).is_some() {
        def.id = util::new_id("custom");
    }
    custom_theme::save(&state.config_dir, &def).map_err(err)?;
    Ok(lite(def))
}

/// CSS and sample slides for the Theme Builder's live preview.
#[tauri::command]
fn theme_preview(window: Window, state: State<AppState>, def: custom_theme::ThemeDef) -> CmdResult<Value> {
    // With its fonts (as data URIs), so library themes preview correctly
    // even when the open presentation doesn't use their fonts.
    let mut def = full_theme(&state, window.label(), def);
    let theme_fonts = std::mem::take(&mut def.fonts);
    let docs = state.docs.lock().map_err(err)?;
    let doc = docs.get(window.label());
    let mut deck = model::Deck {
        theme: def.id.clone(),
        custom_theme: Some(def),
        ..Default::default()
    };
    if let Some(d) = doc {
        deck.web_fonts = d.deck.web_fonts.clone();
        deck.width = d.deck.width;
        deck.height = d.deck.height;
    }
    for f in theme_fonts {
        if !deck.web_fonts.iter().any(|x| x.family == f.family) {
            deck.web_fonts.push(f);
        }
    }
    deck.slides = layouts::sample_slides(deck.width, deck.height);
    let assets = doc.map(|d| d.assets_dir()).unwrap_or_default();
    let url = move |name: &str| document::file_url(&assets.join(name));
    let ctx = Ctx {
        mode: Mode::Editor,
        asset_url: &url,
    };
    let slides: Vec<render::SlideRender> = deck.slides.iter().map(|s| render::slide_for_editor(&deck, s, &ctx)).collect();
    Ok(json!({ "css": render::deck_css(&deck, &ctx), "slides": slides, "width": deck.width, "height": deck.height }))
}

#[tauri::command]
async fn doc_open(window: Window, state: State<'_, AppState>, path: String) -> CmdResult<DocView> {
    let p = PathBuf::from(&path);
    let root = state.sessions_root.clone();
    let doc = tauri::async_runtime::spawn_blocking(move || Document::open(&root, &p))
        .await
        .map_err(err)?
        .map_err(err)?;
    state.remember_recent(Path::new(&path));
    install_doc(&state, window.label(), doc)
}

/// The window's current document (full view), or a file waiting to be opened.
#[tauri::command]
fn doc_state(window: Window, state: State<AppState>) -> CmdResult<Value> {
    if let Some(path) = state.pending_open.lock().map_err(err)?.remove(window.label()) {
        return Ok(json!({ "pendingOpen": path }));
    }
    let mut docs = state.docs.lock().map_err(err)?;
    match docs.get_mut(window.label()) {
        Some(doc) => Ok(json!({ "view": doc.view(true, None) })),
        None => Ok(json!({})),
    }
}

/// Resynchronizes one presentation after responses arrive out of order.
#[tauri::command]
fn doc_refresh(window: Window, state: State<AppState>, doc_id: String) -> CmdResult<DocView> {
    state.with_doc_id(window.label(), &doc_id, |doc| Ok(doc.view(true, None)))
}

/// Applies an editing operation. `track: false` skips undo history (used
/// for derived data such as cached diagram renderings).
#[tauri::command]
fn doc_apply(window: Window, state: State<AppState>, op: Op, coalesce: Option<String>, track: Option<bool>, doc_id: Option<String>) -> CmdResult<DocView> {
    state.with_doc(window.label(), |doc| {
        if let Some(id) = doc_id {
            doc.ensure_id(&id)?;
        }
        let out = if track == Some(false) {
            doc.apply_untracked(op)?
        } else {
            doc.apply(op, coalesce.as_deref())?
        };
        Ok(doc.view(false, Some(out)))
    })
}

#[tauri::command]
fn doc_undo(window: Window, state: State<AppState>) -> CmdResult<DocView> {
    state.with_doc(window.label(), |doc| {
        let out = doc.undo();
        Ok(doc.view(false, out))
    })
}

#[tauri::command]
fn doc_redo(window: Window, state: State<AppState>) -> CmdResult<DocView> {
    state.with_doc(window.label(), |doc| {
        let out = doc.redo();
        Ok(doc.view(false, out))
    })
}

/// Ends the current coalescing group (e.g. when a text box loses focus).
#[tauri::command]
fn doc_seal(window: Window, state: State<AppState>) -> CmdResult<()> {
    state.with_doc(window.label(), |doc| {
        doc.history.seal();
        Ok(())
    })
}

#[tauri::command]
async fn doc_save(window: Window, state: State<'_, AppState>, path: Option<String>) -> CmdResult<DocView> {
    let label = window.label().to_string();
    let target = {
        let docs = state.docs.lock().map_err(err)?;
        let doc = docs.get(&label).ok_or("No presentation is open")?;
        match path.map(PathBuf::from).or_else(|| doc.path.clone()) {
            Some(p) => p,
            None => return Err("NEEDS_PATH".into()),
        }
    };
    let target = if target.extension().is_none() {
        target.with_extension(package::EXTENSION)
    } else {
        target
    };
    let view = state.with_doc(&label, |doc| {
        doc.save(&target).map_err(|e| match e.downcast_ref::<package::MissingMedia>() {
            // It names slides and fonts, so it is translated here.
            Some(missing) => {
                let lang = state.settings.lock().map(|s| s.language.clone()).unwrap_or_default();
                anyhow::anyhow!(missing.message(&translator(window.app_handle(), &lang)))
            }
            None => e,
        })?;
        Ok(doc.view(false, None))
    })?;
    state.remember_recent(&target);
    Ok(view)
}

/// Writes a crash-recovery snapshot if there are unsaved changes.
#[tauri::command]
fn doc_checkpoint(window: Window, state: State<AppState>) -> CmdResult<()> {
    state.with_doc(window.label(), |doc| {
        doc.maybe_write_recovery(true);
        Ok(())
    })
}

#[tauri::command]
fn doc_close(window: Window, state: State<AppState>) -> CmdResult<()> {
    let mut docs = state.docs.lock().map_err(err)?;
    state.cancel_ai(window.label());
    if let Some(doc) = docs.remove(window.label()) {
        doc.dispose();
    }
    Ok(())
}

#[tauri::command]
fn doc_recover(window: Window, state: State<AppState>, session_id: String) -> CmdResult<DocView> {
    let doc = Document::recover(&state.sessions_root, &session_id).map_err(err)?;
    state.recoverable.lock().map_err(err)?.retain(|r| r.session_id != session_id);
    install_doc(&state, window.label(), doc)
}

#[tauri::command]
fn doc_discard_recovery(state: State<AppState>, session_id: String) -> CmdResult<()> {
    document::discard_recovery(&state.sessions_root, &session_id);
    state.recoverable.lock().map_err(err)?.retain(|r| r.session_id != session_id);
    Ok(())
}

// ---------------------------------------------------------------------------
// Assets & import
// ---------------------------------------------------------------------------

#[tauri::command]
fn clipboard_copy(window: Window, state: State<AppState>, doc_id: String, selection: clipboard::Selection) -> CmdResult<clipboard::Clipboard> {
    state.with_doc_id(window.label(), &doc_id, |doc| {
        clipboard::Clipboard::copy(doc, selection, &state.clipboard, |text| {
            window.clipboard().write_text(text)?;
            Ok(())
        })
    })
}

#[tauri::command]
fn clipboard_paste(
    window: Window,
    state: State<AppState>,
    doc_id: String,
    clip: clipboard::Clipboard,
    target: Option<String>,
    offset: f64,
) -> CmdResult<DocView> {
    state.with_doc_id(window.label(), &doc_id, |doc| {
        let out = clip.paste(doc, target, offset, &state.clipboard)?;
        Ok(doc.view(false, Some(out)))
    })
}

#[tauri::command]
async fn asset_import_file(window: Window, state: State<'_, AppState>, path: String) -> CmdResult<AssetInfo> {
    state.with_doc(window.label(), |doc| doc.import_file(Path::new(&path)))
}

/// Imports raw bytes (clipboard images) for the session named by `x-doc-id`.
/// The extension travels in `x-ext` and the body is the raw file.
#[tauri::command]
fn asset_import_bytes(window: Window, state: State<AppState>, request: tauri::ipc::Request<'_>) -> CmdResult<AssetInfo> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Expected binary data".into());
    };
    let ext = request.headers().get("x-ext").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let doc_id = request
        .headers()
        .get("x-doc-id")
        .and_then(|v| v.to_str().ok())
        .ok_or("No presentation is open")?;
    state.with_doc_id(window.label(), doc_id, |doc| doc.import_bytes(bytes, &ext))
}

/// Imports a Markdown file, either replacing the deck or appending slides.
#[tauri::command]
async fn import_markdown(window: Window, state: State<'_, AppState>, path: String, replace: bool) -> CmdResult<DocView> {
    let text = std::fs::read_to_string(&path).map_err(err)?;
    let base = Path::new(&path).parent().map(Path::to_path_buf).unwrap_or_default();
    state.with_doc(window.label(), |doc| {
        let parsed = markdown::parse(&text);
        if parsed.is_empty() {
            anyhow::bail!("The Markdown file has no content");
        }
        let (w, h) = (doc.deck.width, doc.deck.height);
        let mut resolve = |src: &str| -> Option<(String, u32, u32)> {
            if util::is_remote(src) {
                return Some((src.to_string(), 0, 0));
            }
            let p = base.join(src);
            doc.import_file(&p).ok().map(|a| (a.name, a.width, a.height))
        };
        let slides = markdown::to_slides(&parsed, w, h, &mut resolve);
        let op = if replace {
            let mut deck = doc.deck.clone();
            deck.slides = slides;
            if let Some(title) = parsed.iter().find_map(|s| s.title.clone()) {
                deck.title = title;
            }
            Op::ReplaceDeck {
                deck: Box::new(deck),
                label: None,
            }
        } else {
            Op::InsertSlides {
                after: doc.deck.slides.last().map(|s| s.id.clone()),
                slides,
            }
        };
        let out = doc.apply(op, None)?;
        Ok(doc.view(false, Some(out)))
    })
}

/// Downloads a Google font into the presentation (embedded for offline use).
#[tauri::command]
async fn font_install(window: Window, state: State<'_, AppState>, family: String, variants: String) -> CmdResult<DocView> {
    let label = window.label().to_string();
    let assets = {
        let docs = state.docs.lock().map_err(err)?;
        let doc = docs.get(&label).ok_or("No presentation is open")?;
        if doc.deck.web_fonts.iter().any(|f| f.family == family) {
            None
        } else {
            Some(doc.assets_dir())
        }
    };
    let font = match assets {
        Some(dir) => {
            let fam = family.clone();
            let font =
                tauri::async_runtime::spawn_blocking(move || webfonts::install(&fam, &variants, &|bytes: &[u8]| document::store_asset(&dir, bytes, "woff2")))
                    .await
                    .map_err(err)?
                    .map_err(err)?;
            Some(font)
        }
        None => None,
    };
    state.with_doc(&label, |doc| {
        let Some(font) = font else { return Ok(doc.view(false, None)) };
        let mut fonts = doc.deck.web_fonts.clone();
        fonts.retain(|f| f.family != font.family);
        fonts.push(font);
        let out = doc.apply(
            Op::UpdateDeck {
                patch: json!({ "webFonts": fonts }),
            },
            None,
        )?;
        Ok(doc.view(false, Some(out)))
    })
}

// ---------------------------------------------------------------------------
// API keys & online media
// ---------------------------------------------------------------------------

/// Which online services have an API key (never the keys themselves).
#[tauri::command]
fn secret_status(state: State<AppState>) -> std::collections::BTreeMap<&'static str, bool> {
    state.secrets.status()
}

#[tauri::command]
fn secret_set(state: State<AppState>, name: String, value: String) -> CmdResult<&'static str> {
    state.secrets.set(&name, &value).map_err(err)
}

#[tauri::command]
fn secret_delete(state: State<AppState>, name: String) -> CmdResult<()> {
    state.secrets.delete(&name).map_err(err)
}

fn provider_key(state: &AppState, provider: &str) -> Option<String> {
    match provider {
        "unsplash" | "giphy" | "youtube" => state.secrets.get(provider),
        _ => None,
    }
}

#[tauri::command]
async fn media_search(state: State<'_, AppState>, provider: String, query: String, page: Option<String>) -> CmdResult<media::MediaPage> {
    let key = provider_key(&state, &provider);
    tauri::async_runtime::spawn_blocking(move || media::search(&provider, &query, page.as_deref(), key.as_deref()))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
async fn media_oembed(url: String) -> CmdResult<media::MediaItem> {
    tauri::async_runtime::spawn_blocking(move || media::oembed(&url))
        .await
        .map_err(err)?
        .map_err(err)
}

/// Opens a provider's website (YouTube, Vimeo, GIPHY, Unsplash) with an
/// "Insert" bar. The site runs in a separate helper process (this same
/// executable with `--media-browser`), so a crash in the web engine can never
/// take the editor — and unsaved work — down with it. The pick comes back as
/// the `cz-media-picked` event.
#[tauri::command]
fn media_browse(app: AppHandle, window: Window, state: State<AppState>, provider: String, safe: Option<bool>) -> CmdResult<()> {
    use std::io::{BufRead, BufReader};
    browse::start_url(&provider).ok_or("Unknown provider")?;
    let owner = window.label().to_string();
    if let Some((_, mut old)) = state.browsers.lock().map_err(err)?.remove(&owner) {
        let _ = old.kill();
        let _ = old.wait();
    }
    let exe = std::env::current_exe().map_err(err)?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--media-browser").arg(&provider);
    let lang = state.settings.lock().map(|s| s.language.clone()).unwrap_or_default();
    cmd.arg("--lang").arg(lang);
    if safe == Some(true) {
        cmd.arg("--safe");
    }
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(err)?;
    let stdout = child.stdout.take();
    static NEXT: AtomicU32 = AtomicU32::new(1);
    let id = NEXT.fetch_add(1, Ordering::SeqCst) as u64;
    state.browsers.lock().map_err(err)?.insert(owner.clone(), (id, child));
    std::thread::spawn(move || {
        let mut picked: Option<Value> = None;
        if let Some(out) = stdout {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    picked = Some(v);
                    break;
                }
            }
        }
        let state = app.state::<AppState>();
        let child = state.browsers.lock().ok().and_then(|mut b| match b.get(&owner) {
            Some((cid, _)) if *cid == id => b.remove(&owner).map(|(_, c)| c),
            _ => None,
        });
        let crashed = child.map(|mut c| c.wait().map(|s| !s.success()).unwrap_or(false)).unwrap_or(false);
        if let Some(p) = picked {
            let _ = app.emit_to(owner.as_str(), "cz-media-picked", p);
        } else if crashed {
            let _ = app.emit_to(owner.as_str(), "cz-media-browser-closed", json!({ "provider": provider, "crashed": true }));
        }
        if let Some(w) = app.get_webview_window(&owner) {
            let _ = w.set_focus();
        }
    });
    Ok(())
}

fn stop_browser(app: &AppHandle, owner: &str) {
    let state = app.state::<AppState>();
    let child = state.browsers.lock().ok().and_then(|mut b| b.remove(owner));
    if let Some((_, mut c)) = child {
        let _ = c.kill();
        let _ = c.wait();
    }
}

/// Entry point of the `--media-browser` helper process.
/// Interface text in `lang` from the UI's catalogs (ui/locales), for the few
/// strings Rust shows itself.
fn translator(app: &AppHandle, lang: &str) -> impl Fn(&str) -> String + use<> {
    let catalog: HashMap<String, Value> = if lang.is_empty() || lang == "en" || !lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        HashMap::new()
    } else {
        app.asset_resolver()
            .get(format!("locales/{lang}.json"))
            .and_then(|a| serde_json::from_slice(&a.bytes).ok())
            .unwrap_or_default()
    };
    move |key: &str| catalog.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or(key).to_string()
}

/// Translates `key` with `text`; also marks it for the catalogs (tools/i18n.py).
fn tr(text: &dyn Fn(&str) -> String, key: &str) -> String {
    text(key)
}

fn run_media_browser(context: tauri::Context<tauri::Wry>, provider: String, lang: String, safe: bool) {
    let Some(start) = browse::start_url(&provider) else { std::process::exit(2) };
    let name = match provider.as_str() {
        "youtube" => "YouTube",
        "vimeo" => "Vimeo",
        "giphy" => "GIPHY",
        _ => "Unsplash",
    };
    tauri::Builder::default()
        .setup(move |app| {
            let text = translator(app.handle(), &lang);
            let prov = provider.clone();
            let w = WebviewWindowBuilder::new(app, "media-browser", WebviewUrl::External(start.parse()?))
                .title(tr(&text, "{name} — pick something to insert into Cadenza").replace("{name}", name))
                .inner_size(1200.0, 840.0)
                .initialization_script(browse::bar_script(&provider, &text))
                .on_navigation(move |url| {
                    if url.host_str() != Some(browse::PICK_HOST) {
                        return true;
                    }
                    use std::io::Write;
                    if let Some(picked) = browse::resolve(&prov, url) {
                        if let Ok(json) = serde_json::to_string(&picked) {
                            println!("{json}");
                        }
                    }
                    let _ = std::io::stdout().flush();
                    std::process::exit(0);
                })
                .build()?;
            harden_browser(&w, safe);
            Ok(())
        })
        .run(context)
        .expect("error while running the media browser");
}

/// The media browser only needs to show pages: no autoplay, no DRM, camera
/// or WebRTC. Safe mode also turns media off entirely, which sidesteps
/// WebKitGTK media crashes seen on some systems.
#[cfg(target_os = "linux")]
fn harden_browser(w: &tauri::WebviewWindow, safe: bool) {
    let _ = w.with_webview(move |wv| {
        use webkit2gtk::{SettingsExt, WebViewExt};
        if let Some(settings) = WebViewExt::settings(&wv.inner()) {
            if safe {
                settings.set_enable_media(false);
                settings.set_enable_webgl(false);
                settings.set_hardware_acceleration_policy(webkit2gtk::HardwareAccelerationPolicy::Never);
            }
            settings.set_media_playback_requires_user_gesture(true);
            settings.set_enable_encrypted_media(false);
            settings.set_enable_webrtc(false);
            settings.set_enable_media_stream(false);
        }
    });
}

#[cfg(not(target_os = "linux"))]
fn harden_browser(_w: &tauri::WebviewWindow, _safe: bool) {}

/// Downloads a picture/GIF into the presentation's assets.
#[tauri::command]
async fn media_import(window: Window, state: State<'_, AppState>, url: String, provider: String, track_url: Option<String>) -> CmdResult<AssetInfo> {
    if !state.docs.lock().map_err(err)?.contains_key(window.label()) {
        return Err("No presentation is open".into());
    }
    let unsplash_key = if provider == "unsplash" { state.secrets.get("unsplash") } else { None };
    let (bytes, ext) = tauri::async_runtime::spawn_blocking(move || {
        if let (Some(t), Some(k)) = (track_url.as_deref(), unsplash_key.as_deref()) {
            media::track_unsplash(t, k);
        }
        media::download(&url)
    })
    .await
    .map_err(err)?
    .map_err(err)?;
    state.with_doc(window.label(), |doc| doc.import_bytes(&bytes, &ext))
}

// ---------------------------------------------------------------------------
// AI assistant (opt-in)
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiRequest {
    /// "new" (create a presentation) or "edit" (change the open one).
    mode: String,
    /// Session identity of the presentation the user asked to edit.
    #[serde(default)]
    doc_id: Option<String>,
    prompt: String,
    #[serde(default)]
    history: Vec<ai::Message>,
    #[serde(default)]
    current_slide: Option<String>,
    #[serde(default)]
    selected: Vec<String>,
    /// New presentations: wanted number of slides (0 = let the model decide).
    #[serde(default)]
    slides: u32,
    #[serde(default)]
    audience: String,
    #[serde(default)]
    language: String,
    /// Notes or text to base a new presentation on.
    #[serde(default)]
    source: String,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    aspect: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AiResponse {
    reply: String,
    summary: ai::Summary,
    view: Option<DocView>,
}

/// The configured provider, or why the assistant can't run.
fn ai_provider(state: &AppState) -> CmdResult<(ai::Provider, bool)> {
    let s = state.settings.lock().map_err(err)?.clone();
    if !s.ai_enabled {
        return Err("The AI assistant is turned off. You can turn it on in Settings ▸ AI Assistant.".into());
    }
    let kind = s.ai_provider.clone();
    let model = s
        .ai_models
        .get(&kind)
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .or_else(|| (kind == "anthropic").then(|| ai::DEFAULT_ANTHROPIC_MODEL.to_string()))
        .ok_or("Choose a model in Settings ▸ AI Assistant")?;
    let key = state.secrets.get(&kind);
    if kind != "compatible" && key.is_none() {
        let name = if kind == "openai" { "OpenAI" } else { "Anthropic" };
        return Err(format!("Add your {name} API key in Settings ▸ AI Assistant"));
    }
    Ok((
        ai::Provider {
            kind,
            model,
            base_url: s.ai_base_url.clone(),
            key,
        },
        s.ai_images,
    ))
}

#[tauri::command]
async fn ai_models(state: State<'_, AppState>, provider: String, base_url: Option<String>) -> CmdResult<Vec<ai::ModelInfo>> {
    let p = ai::Provider {
        key: state.secrets.get(&provider),
        kind: provider,
        model: String::new(),
        base_url: base_url.unwrap_or_default(),
    };
    tauri::async_runtime::spawn_blocking(move || ai::list_models(&p))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
fn ai_cancel(window: Window, state: State<AppState>) {
    if let Ok(runs) = state.ai_runs.lock() {
        if let Some(flag) = runs.get(window.label()) {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

fn ai_new_deck_prompt(req: &AiRequest) -> String {
    let mut p = format!("Create a complete presentation.\n\nTopic and instructions:\n{}", req.prompt.trim());
    match req.slides {
        0 => p.push_str("\n\nLength: choose a suitable number of slides (usually 8 to 12)."),
        n => p.push_str(&format!("\n\nLength: about {n} slides.")),
    }
    if !req.audience.trim().is_empty() {
        p.push_str(&format!("\nAudience and tone: {}", req.audience.trim()));
    }
    if !req.language.trim().is_empty() {
        p.push_str(&format!("\nWrite the presentation in {}.", req.language.trim()));
    }
    if !req.source.trim().is_empty() {
        let source: String = req.source.trim().chars().take(60_000).collect();
        p.push_str(&format!("\n\nBase the content on this source material:\n<source>\n{source}\n</source>"));
    }
    p.push_str(
        "\n\nAnswer with a \"set_title\" action and one \"insert\" action (slide_id null) containing every slide. \
         Start with a \"title\" slide, separate the parts of longer talks with \"section\" slides, and end with a closing slide \
         (key takeaways, a call to action or questions). Write speaker notes for every slide.",
    );
    p
}

fn ai_theme_list(custom: Option<&custom_theme::ThemeDef>) -> String {
    let mut out = String::from("Themes you can switch to (id: name, look):");
    if let Some(c) = custom {
        out.push_str(&format!("\n- {}: {} (custom, {})", c.id, c.name, if c.dark { "dark" } else { "light" }));
    }
    for t in themes::THEMES {
        out.push_str(&format!("\n- {}: {} ({}, {})", t.id, t.name, t.group, if t.dark { "dark" } else { "light" }));
    }
    out
}

/// Keeps the conversation valid: starts with the user, alternates roles.
fn ai_history(history: &[ai::Message]) -> Vec<ai::Message> {
    let mut out: Vec<ai::Message> = Vec::new();
    for m in history.iter().rev().take(10).rev() {
        if !(m.role == "user" || m.role == "assistant") || m.text.trim().is_empty() || (out.is_empty() && m.role != "user") {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.role == m.role => {
                last.text.push_str("\n\n");
                last.text.push_str(&m.text);
            }
            _ => out.push(m.clone()),
        }
    }
    if out.last().is_some_and(|m| m.role == "user") {
        out.pop();
    }
    out
}

/// Credit line stored as the image description (as the Media Library does).
fn ai_credit(it: &media::MediaItem) -> String {
    match it.provider.as_str() {
        "unsplash" => format!("Photo by {} on Unsplash", it.author),
        _ => {
            let title = if it.title.is_empty() {
                "Image".to_string()
            } else {
                format!("“{}”", it.title)
            };
            let by = if it.author.is_empty() { String::new() } else { format!(" by {}", it.author) };
            let lic = if it.license.is_empty() { String::new() } else { format!(", {}", it.license) };
            format!("{title}{by}{lic}")
        }
    }
}

/// Finds and downloads one photo per query (in parallel) into `dir`.
fn ai_find_images(queries: &[String], dir: &Path, unsplash: Option<String>, cancel: &AtomicBool) -> HashMap<String, ai::FoundImage> {
    let provider = if unsplash.is_some() { "unsplash" } else { "openverse" };
    std::thread::scope(|scope| {
        let jobs: Vec<_> = queries
            .iter()
            .take(12)
            .map(|q| {
                let key = unsplash.clone();
                scope.spawn(move || -> Option<(String, ai::FoundImage)> {
                    if cancel.load(Ordering::SeqCst) {
                        return None;
                    }
                    let page = media::search(provider, q, None, key.as_deref()).ok()?;
                    let item = page.items.into_iter().find(|x| x.kind == "image")?;
                    let (bytes, ext) = media::download(&item.url).ok()?;
                    if let Some(k) = key.as_deref().filter(|_| !item.track_url.is_empty()) {
                        media::track_unsplash(&item.track_url, k);
                    }
                    let src = document::store_asset(dir, &bytes, &ext).ok()?;
                    let (width, height) = imagesize::blob_size(&bytes)
                        .map(|s| (s.width as u32, s.height as u32))
                        .unwrap_or((item.width, item.height));
                    Some((
                        q.clone(),
                        ai::FoundImage {
                            src,
                            width,
                            height,
                            alt: ai_credit(&item),
                        },
                    ))
                })
            })
            .collect();
        jobs.into_iter().filter_map(|j| j.join().ok().flatten()).collect()
    })
}

#[tauri::command]
async fn ai_run(app: AppHandle, window: Window, state: State<'_, AppState>, req: AiRequest) -> CmdResult<AiResponse> {
    let (provider, want_images) = ai_provider(&state)?;
    let label = window.label().to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    {
        // Register under the document lock so closing/replacing it cannot
        // miss cancelling a request that has just started.
        let docs = state.docs.lock().map_err(err)?;
        if req.mode == "new" {
            if docs.contains_key(&label) {
                return Err("Create the presentation in a new window.".into());
            }
        } else {
            let doc = docs.get(&label).ok_or("No presentation is open in this window")?;
            doc.ensure_id(req.doc_id.as_deref().ok_or(ai::CANCELLED)?).map_err(err)?;
        }
        if let Some(old) = state.ai_runs.lock().map_err(err)?.insert(label.clone(), cancel.clone()) {
            old.store(true, Ordering::SeqCst);
        }
    }
    let result = ai_run_inner(&app, &state, &label, req, provider, want_images, &cancel).await;
    if let Ok(mut runs) = state.ai_runs.lock() {
        if runs.get(&label).is_some_and(|c| Arc::ptr_eq(c, &cancel)) {
            runs.remove(&label);
        }
    }
    match result {
        // A change that was already made is reported even if Stop came too
        // late to prevent it, so the window shows what the document holds.
        Err(_) if cancel.load(Ordering::SeqCst) => Err(ai::CANCELLED.into()),
        result => result,
    }
}

async fn ai_run_inner(
    app: &AppHandle,
    state: &AppState,
    label: &str,
    req: AiRequest,
    provider: ai::Provider,
    want_images: bool,
    cancel: &Arc<AtomicBool>,
) -> CmdResult<AiResponse> {
    let emit = {
        let app = app.clone();
        let label = label.to_string();
        move |v: Value| {
            let _ = app.emit_to(label.as_str(), "cz-ai-progress", v);
        }
    };
    let new = req.mode == "new";
    let (request, custom) = if new {
        (ai_new_deck_prompt(&req), None)
    } else {
        let docs = state.docs.lock().map_err(err)?;
        let doc = docs.get(label).ok_or("No presentation is open in this window")?;
        if cancel.load(Ordering::SeqCst) || req.doc_id.as_deref() != Some(doc.id.as_str()) {
            return Err(ai::CANCELLED.into());
        }
        let ctx = ai::deck_context(&doc.deck, req.current_slide.as_deref(), &req.selected);
        (
            format!("The presentation (JSON):\n{ctx}\n\nRequest: {}", req.prompt.trim()),
            doc.deck.active_custom_theme().cloned(),
        )
    };
    let mut messages = ai_history(&req.history);
    messages.push(ai::Message {
        role: "user".into(),
        text: format!("{}\n\n{request}", ai_theme_list(custom.as_ref())),
    });

    emit(json!({ "phase": "thinking" }));
    let text = {
        let cancel = cancel.clone();
        let emit = emit.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let mut last: Option<std::time::Instant> = None;
            ai::complete(&provider, ai::SYSTEM_PROMPT, &messages, &ai::plan_schema(), &cancel, &mut |t: &str| {
                if last.is_none_or(|l| l.elapsed() >= std::time::Duration::from_millis(200)) {
                    last = Some(std::time::Instant::now());
                    let (slides, title) = ai::progress(t);
                    emit(json!({ "phase": "writing", "slides": slides, "title": title, "chars": t.len() }));
                }
            })
        })
        .await
        .map_err(err)?
        .map_err(err)?
    };
    if cancel.load(Ordering::SeqCst) {
        return Err(ai::CANCELLED.into());
    }
    let plan = ai::parse_plan(&text).map_err(err)?;

    // New presentations get their document (and assets folder) now.
    let mut new_doc = None;
    let assets_dir = if new {
        let s = state.settings.lock().map_err(err)?.clone();
        let theme = req.theme.clone().unwrap_or(s.default_theme.clone());
        let (w, h) = settings::aspect_size(req.aspect.as_deref().unwrap_or(&s.default_aspect));
        let mut doc = Document::new(&state.sessions_root, layouts::starter_deck(&theme, w, h)).map_err(err)?;
        if let Some(def) = custom_theme::find(&state.config_dir, &theme) {
            apply_theme_to(&mut doc, def).map_err(err)?;
            doc.reset_baseline();
        }
        let dir = doc.assets_dir();
        new_doc = Some(doc);
        dir
    } else {
        state.with_doc_id(label, req.doc_id.as_deref().ok_or(ai::CANCELLED)?, |doc| Ok(doc.assets_dir()))?
    };

    // Photos are fetched before touching the presentation.
    let queries = if want_images { ai::image_queries(&plan) } else { Vec::new() };
    let images = if queries.is_empty() {
        HashMap::new()
    } else {
        emit(json!({ "phase": "images", "count": queries.len() }));
        let unsplash = state.secrets.get("unsplash");
        let cancel = cancel.clone();
        tauri::async_runtime::spawn_blocking(move || ai_find_images(&queries, &assets_dir, unsplash, &cancel))
            .await
            .map_err(err)?
    };
    if cancel.load(Ordering::SeqCst) {
        if let Some(doc) = new_doc {
            doc.dispose();
        }
        return Err(ai::CANCELLED.into());
    }

    if let Some(mut doc) = new_doc {
        let active = doc.deck.active_custom_theme().map(|t| t.id.clone());
        let theme_ok = |t: &str| themes::THEMES.iter().any(|x| x.id == t) || active.as_deref() == Some(t);
        let mut deck = doc.deck.clone();
        deck.slides.clear();
        let summary = ai::apply_plan(&mut deck, &plan, &images, &theme_ok);
        if deck.slides.is_empty() {
            doc.dispose();
            let why = if plan.reply.trim().is_empty() {
                "The AI didn't create any slides. Try describing the presentation in more detail.".to_string()
            } else {
                plan.reply.clone()
            };
            return Err(why);
        }
        doc.apply(
            Op::ReplaceDeck {
                deck: Box::new(deck),
                label: Some("AI Presentation".into()),
            },
            None,
        )
        .map_err(err)?;
        let view = {
            let mut docs = state.docs.lock().map_err(err)?;
            if cancel.load(Ordering::SeqCst) || docs.contains_key(label) {
                doc.dispose();
                return Err(ai::CANCELLED.into());
            }
            let view = doc.view(true, None);
            docs.insert(label.to_string(), doc);
            view
        };
        return Ok(AiResponse {
            reply: plan.reply,
            summary,
            view: Some(view),
        });
    }

    let active = custom.as_ref().map(|t| t.id.clone());
    let theme_ok = |t: &str| themes::THEMES.iter().any(|x| x.id == t) || active.as_deref() == Some(t);
    let (summary, view) = state.with_doc(label, |doc| {
        ai::apply_edit(doc, req.doc_id.as_deref().unwrap_or(""), cancel, &plan, &images, &theme_ok)
    })?;
    Ok(AiResponse {
        reply: plan.reply,
        summary,
        view,
    })
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

fn ui_reader(app: &AppHandle) -> impl Fn(&str) -> Option<Vec<u8>> + '_ {
    move |path: &str| app.asset_resolver().get(path.to_string()).map(|a| a.bytes)
}

#[tauri::command]
async fn export_html(app: AppHandle, window: Window, state: State<'_, AppState>, path: String, single: bool) -> CmdResult<String> {
    let (deck, assets) = {
        let docs = state.docs.lock().map_err(err)?;
        let doc = docs.get(window.label()).ok_or("No presentation is open")?;
        (doc.deck.clone(), doc.assets_dir())
    };
    let target = PathBuf::from(&path);
    let reader = ui_reader(&app);
    if single {
        let file = if target.extension().is_none() {
            target.with_extension("html")
        } else {
            target
        };
        export::export_single(&deck, &assets, &file, &reader).map_err(err)?;
        Ok(file.to_string_lossy().to_string())
    } else {
        let index = export::export_folder(&deck, &assets, &target, &reader).map_err(err)?;
        Ok(index.to_string_lossy().to_string())
    }
}

/// Opens a window that lays the deck out for printing and shows the
/// system print dialog (choose "Print to File"/"Save as PDF").
#[tauri::command]
fn export_pdf(app: AppHandle, window: Window) -> CmdResult<()> {
    let label = format!("print-{}", window.label());
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.close();
    }
    let url = format!("present.html?doc={}&print-pdf", util::encode_uri_component(window.label()));
    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.into()))
        .title("Cadenza — Print / Export PDF")
        .inner_size(1100.0, 800.0)
        .build()
        .map_err(err)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Presenting
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PresentSlide {
    id: String,
    h: usize,
    v: usize,
    notes: String,
    index: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PresentData {
    title: String,
    sections: String,
    config: Value,
    theme_css: &'static str,
    deck_css: String,
    code_theme: String,
    needs: render::Needs,
    slides: Vec<PresentSlide>,
    width: u32,
    height: u32,
    /// Interface language for the presenter display.
    language: String,
}

/// Everything a presentation window needs to start reveal.js.
#[tauri::command]
fn present_data(window: Window, state: State<AppState>) -> CmdResult<PresentData> {
    let owner = owner_label(window.label()).to_string();
    let docs = state.docs.lock().map_err(err)?;
    let doc = docs.get(&owner).ok_or("The presentation is no longer open")?;
    let deck = &doc.deck;
    let assets = doc.assets_dir();
    let url = move |name: &str| document::file_url(&assets.join(name));
    let ctx = Ctx {
        mode: Mode::Show,
        asset_url: &url,
    };
    let mut slides = Vec::new();
    for (h, group) in render::visible_groups(deck).iter().enumerate() {
        for (v, s) in group.iter().enumerate() {
            slides.push(PresentSlide {
                id: s.id.clone(),
                h,
                v,
                notes: s.notes.clone(),
                index: slides.len(),
            });
        }
    }
    Ok(PresentData {
        title: deck.title.clone(),
        sections: render::sections(deck, &ctx),
        config: render::reveal_config(deck, json!({ "hash": false, "respondToHashChanges": false })),
        theme_css: deck.theme_href().unwrap_or(""),
        deck_css: render::deck_css(deck, &ctx),
        code_theme: if deck.settings.code_theme == "zenburn" {
            "zenburn".into()
        } else {
            "monokai".into()
        },
        needs: render::needs(deck),
        slides,
        width: deck.width,
        height: deck.height,
        language: state.settings.lock().map(|s| s.language.clone()).unwrap_or_default(),
    })
}

#[tauri::command]
fn present_start(app: AppHandle, window: Window, state: State<AppState>, from_slide: Option<String>, presenter: Option<bool>) -> CmdResult<()> {
    let owner = window.label().to_string();
    let presenter = presenter.unwrap_or_else(|| state.settings.lock().map(|s| s.presenter_view).unwrap_or(true));
    let (h, v) = {
        let docs = state.docs.lock().map_err(err)?;
        let doc = docs.get(&owner).ok_or("No presentation is open")?;
        if render::visible_groups(&doc.deck).is_empty() {
            return Err("All slides are skipped — there is nothing to present.".into());
        }
        from_slide.as_deref().and_then(|id| render::slide_indices(&doc.deck, id)).unwrap_or((0, 0))
    };
    close_presentation(&app, &owner);

    let monitors = window.available_monitors().map_err(err)?;
    let current = window.current_monitor().map_err(err)?.or_else(|| monitors.first().cloned());
    // With two screens, the audience sees the one the editor is not on.
    let audience = if presenter && monitors.len() > 1 {
        let cur_pos = current.as_ref().map(|m| *m.position());
        monitors.iter().find(|m| Some(*m.position()) != cur_pos).cloned().or(current.clone())
    } else {
        current.clone()
    };

    let present_label = format!("present-{owner}");
    let url = format!("present.html?doc={}&h={h}&v={v}", util::encode_uri_component(&owner));
    let w = WebviewWindowBuilder::new(&app, &present_label, WebviewUrl::App(url.into()))
        .title("Cadenza — Presentation")
        .visible(false)
        .decorations(false)
        .build()
        .map_err(err)?;
    if let Some(m) = &audience {
        let _ = w.set_position(tauri::Position::Physical(*m.position()));
        let _ = w.set_size(tauri::Size::Physical(*m.size()));
    }
    let _ = w.set_fullscreen(true);
    let _ = w.show();
    let _ = w.set_focus();

    if presenter {
        let pl = format!("presenter-{owner}");
        let url = format!("presenter.html?doc={}", util::encode_uri_component(&owner));
        let p = WebviewWindowBuilder::new(&app, &pl, WebviewUrl::App(url.into()))
            .title("Cadenza — Presenter")
            .inner_size(1280.0, 800.0)
            .visible(false)
            .build()
            .map_err(err)?;
        if let Some(m) = &current {
            let _ = p.set_position(tauri::Position::Physical(*m.position()));
        }
        let _ = p.maximize();
        let _ = p.show();
        // Keep keyboard focus on the screen the presenter is looking at.
        if monitors.len() > 1 {
            let _ = p.set_focus();
        }
    }
    let _ = app.emit("cz-present-started", json!({ "doc": owner }));
    Ok(())
}

fn close_presentation(app: &AppHandle, owner: &str) {
    for prefix in ["present-", "presenter-"] {
        if let Some(w) = app.get_webview_window(&format!("{prefix}{owner}")) {
            let _ = w.close();
        }
    }
}

/// Stops the show. `slide` is the slide id the show ended on, so the editor
/// can jump there.
#[tauri::command]
fn present_stop(app: AppHandle, window: Window, slide: Option<String>) -> CmdResult<()> {
    let owner = owner_label(window.label()).to_string();
    close_presentation(&app, &owner);
    let _ = app.emit_to(owner.as_str(), "cz-present-ended", json!({ "slide": slide }));
    if let Some(w) = app.get_webview_window(&owner) {
        let _ = w.set_focus();
    }
    Ok(())
}

/// Forwards UI errors to the terminal (helps diagnose problems in the field).
#[tauri::command]
fn ui_log(window: Window, level: String, message: String) {
    eprintln!("[ui:{}:{}] {}", window.label(), level, message);
}

#[tauri::command]
fn present_toggle_fullscreen(window: Window) -> CmdResult<()> {
    let fs = window.is_fullscreen().map_err(err)?;
    window.set_fullscreen(!fs).map_err(err)
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

fn open_editor_window(app: &AppHandle, file: Option<PathBuf>) -> tauri::Result<()> {
    let state = app.state::<AppState>();
    let n = state.window_counter.fetch_add(1, Ordering::SeqCst);
    let label = format!("editor-{n}");
    if let Some(f) = file {
        if let Ok(mut p) = state.pending_open.lock() {
            p.insert(label.clone(), f);
        }
    }
    WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title("Cadenza")
        .inner_size(1440.0, 900.0)
        .min_inner_size(900.0, 560.0)
        .center()
        .build()?;
    Ok(())
}

#[tauri::command]
fn window_new(app: AppHandle, path: Option<String>) -> CmdResult<()> {
    open_editor_window(&app, path.map(PathBuf::from)).map_err(err)
}

/// Focuses the editor window that already has `path` open, if any.
#[tauri::command]
fn window_focus_document(app: AppHandle, state: State<AppState>, path: String) -> CmdResult<bool> {
    let target = PathBuf::from(&path);
    let docs = state.docs.lock().map_err(err)?;
    for (label, doc) in docs.iter() {
        if doc.path.as_deref() == Some(target.as_path()) {
            if let Some(w) = app.get_webview_window(label) {
                let _ = w.unminimize();
                let _ = w.set_focus();
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn files_from_args<I: IntoIterator<Item = String>>(args: I, cwd: &Path) -> Vec<PathBuf> {
    args.into_iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .map(|a| {
            let p = PathBuf::from(&a);
            if p.is_absolute() { p } else { cwd.join(p) }
        })
        .filter(|p| p.extension().map(|e| e == package::EXTENSION).unwrap_or(false) && p.exists())
        .collect()
}

/// `CADENZA_DATA_DIR` gives Cadenza an isolated profile (settings and
/// sessions) and allows several instances — handy for development and
/// portable installs.
fn data_dir_override() -> Option<PathBuf> {
    std::env::var_os("CADENZA_DATA_DIR").filter(|v| !v.is_empty()).map(PathBuf::from)
}

pub fn run() {
    let context = tauri::generate_context!();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--media-browser") {
        let lang = args
            .iter()
            .position(|a| a == "--lang")
            .and_then(|j| args.get(j + 1))
            .cloned()
            .unwrap_or_default();
        run_media_browser(context, args.get(i + 1).cloned().unwrap_or_default(), lang, args.iter().any(|a| a == "--safe"));
        return;
    }
    run_editor(context);
}

fn run_editor(context: tauri::Context<tauri::Wry>) {
    let mut builder = tauri::Builder::default();
    if data_dir_override().is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            let files = files_from_args(argv, Path::new(&cwd));
            if files.is_empty() {
                // Bring an existing editor window forward.
                if let Some(w) = app.webview_windows().iter().find(|(l, _)| l.starts_with("editor-")).map(|(_, w)| w.clone()) {
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                } else {
                    let _ = open_editor_window(app, None);
                }
            }
            for f in files {
                let _ = open_editor_window(app, Some(f));
            }
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let (cache, config) = match data_dir_override() {
                Some(dir) => (dir.join("cache"), dir.clone()),
                None => (app.path().app_cache_dir()?, app.path().app_config_dir()?),
            };
            let sessions_root = cache.join("sessions");
            std::fs::create_dir_all(&sessions_root)?;
            // Media of open documents is served to the webview from here.
            app.asset_protocol_scope().allow_directory(&sessions_root, true)?;
            let recoverable = document::list_recoverable(&sessions_root);
            let settings_path = config.join("settings.json");
            app.manage(AppState {
                docs: Mutex::new(HashMap::new()),
                settings: Mutex::new(Settings::load(&settings_path)),
                settings_path,
                sessions_root,
                config_dir: config.clone(),
                recoverable: Mutex::new(recoverable),
                pending_open: Mutex::new(HashMap::new()),
                window_counter: AtomicU32::new(1),
                secrets: secrets::Secrets::new(&config, data_dir_override().is_none()),
                browsers: Mutex::new(HashMap::new()),
                ai_runs: Mutex::new(HashMap::new()),
                clipboard: clipboard::Store::new(cache.join("clipboard")),
            });
            fonts::warm_up();

            let cwd = std::env::current_dir().unwrap_or_default();
            let files = files_from_args(std::env::args(), &cwd);
            if files.is_empty() {
                open_editor_window(app.handle(), None)?;
            }
            for f in files {
                open_editor_window(app.handle(), Some(f))?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::Destroyed = event {
                let label = window.label().to_string();
                let app = window.app_handle();
                if label.starts_with("editor-") {
                    close_presentation(app, &label);
                    if let Some(w) = app.get_webview_window(&format!("print-{label}")) {
                        let _ = w.close();
                    }
                    stop_browser(app, &label);
                    let state = app.state::<AppState>();
                    let doc = state.docs.lock().ok().and_then(|mut d| {
                        state.cancel_ai(&label);
                        d.remove(&label)
                    });
                    if let Some(doc) = doc {
                        doc.dispose();
                    }
                } else if label.starts_with("present-") || label.starts_with("presenter-") {
                    // Closing either presentation window ends the show.
                    let owner = owner_label(&label).to_string();
                    close_presentation(app, &owner);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            settings_update,
            recent_list,
            recent_remove,
            fonts_list,
            theme_css,
            doc_new,
            doc_open,
            doc_state,
            doc_refresh,
            doc_apply,
            doc_undo,
            doc_redo,
            doc_seal,
            doc_save,
            doc_checkpoint,
            doc_close,
            doc_recover,
            doc_discard_recovery,
            clipboard_copy,
            clipboard_paste,
            asset_import_file,
            asset_import_bytes,
            font_install,
            themes_user,
            theme_from_builtin,
            theme_save,
            theme_delete,
            theme_apply,
            theme_export,
            theme_import,
            theme_preview,
            ai_models,
            ai_run,
            ai_cancel,
            secret_status,
            secret_set,
            secret_delete,
            media_search,
            media_oembed,
            media_browse,
            media_import,
            import_markdown,
            export_html,
            export_pdf,
            present_data,
            present_start,
            present_stop,
            present_toggle_fullscreen,
            ui_log,
            window_new,
            window_focus_document,
        ])
        .run(context)
        .expect("error while running Cadenza");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_labels() {
        assert_eq!(owner_label("present-editor-1"), "editor-1");
        assert_eq!(owner_label("presenter-editor-2"), "editor-2");
        assert_eq!(owner_label("print-editor-3"), "editor-3");
        assert_eq!(owner_label("editor-4"), "editor-4");
    }

    #[test]
    fn cli_files_are_filtered() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.cadenza");
        std::fs::write(&f, b"x").unwrap();
        let args = vec![
            "cadenza".to_string(),
            "--flag".into(),
            "a.cadenza".into(),
            "missing.cadenza".into(),
            "b.txt".into(),
        ];
        assert_eq!(files_from_args(args, dir.path()), vec![f]);
    }
}
