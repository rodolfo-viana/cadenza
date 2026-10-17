//! Cadenza — a presentation studio built on reveal.js.
//!
//! The Rust side owns the document model, editing operations and undo
//! history, the `.cadenza` file format, the asset pipeline, slide rendering,
//! exports and window management. The UI (in `ui/`) runs in the system
//! webview and talks to it through the commands defined here.

pub mod browse;
pub mod chart;
pub mod clipboard;
pub mod custom_theme;
pub mod document;
pub mod embed;
pub mod export;
pub mod fonts;
pub mod layouts;
pub mod maps;
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
pub mod updates;
pub mod util;
pub mod webfonts;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

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
    /// What freshly created editor windows open.
    pending_open: Mutex<HashMap<String, Pending>>,
    window_counter: AtomicU32,
    secrets: secrets::Secrets,
    /// Media browser helper processes, per editor window.
    browsers: Mutex<HashMap<String, (u64, std::process::Child)>>,
    /// Media of the latest copy, shared by every window.
    clipboard: clipboard::Store,
    updates: updates::Updates,
}

/// What a new editor window opens once its page has loaded.
enum Pending {
    File(PathBuf),
    /// A presentation with unsaved changes, from its recovery snapshot.
    Session(String),
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
    /// Whether this copy can update itself.
    updates: bool,
}

#[tauri::command]
fn app_info(state: State<AppState>) -> CmdResult<AppInfo> {
    let settings = state.settings.lock().map_err(err)?.clone();
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        platform: if cfg!(windows) { "windows" } else { "linux" },
        themes: themes::THEMES,
        layouts: layouts::LAYOUTS,
        shapes: shapes::SHAPES.iter().map(|(id, name)| json!({"id": id, "name": name})).collect(),
        charts: chart::CHART_TYPES.iter().map(|(id, name)| json!({"id": id, "name": name})).collect(),
        recent: settings.existing_recent(),
        settings,
        recoverable: state.recoverable.lock().map_err(err)?.clone(),
        asset_base: document::asset_protocol_base(),
        updates: updates::package().is_some(),
    })
}

/// Countries with a map of their states or provinces, as (id, name).
#[tauri::command]
async fn map_countries(language: String) -> Vec<(String, String)> {
    maps::countries(&language)
}

/// The regions of a map and how a chart's rows (labels) name them.
#[tauri::command]
async fn map_regions(map: String, language: String, labels: Vec<String>) -> maps::MapRows {
    maps::regions(&map, &language, &labels)
}

/// A file of data for Chart from Data, as text.
#[tauri::command]
async fn read_data_file(path: String) -> CmdResult<String> {
    let meta = std::fs::metadata(&path).map_err(err)?;
    if meta.len() > 20 * 1024 * 1024 {
        return Err("The file is too large to chart (over 20 MB).".into());
    }
    Ok(util::decode_text(&std::fs::read(&path).map_err(err)?))
}

/// Charts drawn `width` × `height` for the chart gallery (Chart from Data).
#[tauri::command]
async fn chart_previews(charts: Vec<model::ChartContent>, width: f64, height: f64) -> Vec<String> {
    charts.iter().map(|c| chart::chart_svg(c, width, height, None)).collect()
}

#[tauri::command]
fn settings_update(app: AppHandle, state: State<AppState>, patch: Value) -> CmdResult<Settings> {
    let updated = {
        let mut s = state.settings.lock().map_err(err)?;
        let mut v = serde_json::to_value(&*s).map_err(err)?;
        ops::merge_patch(&mut v, &patch);
        *s = serde_json::from_value(v).map_err(err)?;
        s.save(&state.settings_path).map_err(err)?;
        s.clone()
    };
    // Other windows (editors, the presenter display) keep a copy.
    let _ = app.emit("cz-settings", &updated);
    Ok(updated)
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
    if let Ok(docs) = state.docs.lock()
        && let Some(doc) = docs.get(label)
    {
        let mut from_deck = def.clone();
        custom_theme::embed_fonts(&mut from_deck, &doc.deck.web_fonts, &doc.assets_dir());
        for f in from_deck.fonts {
            fonts.retain(|x| x.family != f.family);
            fonts.push(f);
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
fn theme_preview(app: AppHandle, window: Window, state: State<AppState>, def: custom_theme::ThemeDef) -> CmdResult<Value> {
    let language = state.settings.lock().map(|s| s.language.clone()).unwrap_or_default();
    let text = translator(&app, &language);
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
    deck.slides = layouts::sample_slides(deck.width, deck.height, &text);
    let assets = doc.map(|d| d.assets_dir()).unwrap_or_default();
    let url = move |name: &str| document::file_url(&assets.join(name));
    let ctx = Ctx {
        mode: Mode::Editor,
        asset_url: &url,
        youtube_host: None,
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
    match state.pending_open.lock().map_err(err)?.remove(window.label()) {
        Some(Pending::File(path)) => return Ok(json!({ "pendingOpen": path })),
        Some(Pending::Session(id)) => return Ok(json!({ "pendingRecover": id })),
        None => {}
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
    let target = presentation_path(target);
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

/// Where a presentation is saved: with its extension, once ("talk.cadenza"
/// typed into a save dialog that keeps its own ".cadenza" gives
/// "talk.cadenza.cadenza").
fn presentation_path(path: PathBuf) -> PathBuf {
    let double = format!(".{0}.{0}", package::EXTENSION);
    match path.file_name().and_then(|n| n.to_str()) {
        None => path,
        Some(name) if name.len() > double.len() && name.to_lowercase().ends_with(&double) => path.with_extension(""),
        Some(_) if path.extension().is_none() => path.with_extension(package::EXTENSION),
        Some(_) => path,
    }
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

/// Imports the image on the system clipboard as a PNG, if there is one.
/// WebKitGTK gives pasted images to the page in no paste event, so pasting
/// asks here when the event brings neither text nor files.
#[tauri::command]
async fn asset_import_clipboard_image(window: Window, state: State<'_, AppState>, doc_id: String) -> CmdResult<Option<AssetInfo>> {
    let app = window.app_handle().clone();
    let pixels = tauri::async_runtime::spawn_blocking(move || {
        let image = app.clipboard().read_image().ok()?;
        Some((image.rgba().to_vec(), image.width(), image.height()))
    })
    .await
    .map_err(err)?;
    let Some((rgba, width, height)) = pixels else { return Ok(None) };
    let png = util::encode_png(&rgba, width, height).map_err(err)?;
    state.with_doc_id(window.label(), &doc_id, |doc| doc.import_bytes(&png, "png")).map(Some)
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
        "pexels" | "pixabay" | "giphy" | "youtube" => state.secrets.get(provider),
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

#[tauri::command]
async fn media_link(url: String) -> CmdResult<media::MediaItem> {
    tauri::async_runtime::spawn_blocking(move || media::link(&url)).await.map_err(err)?.map_err(err)
}

/// Opens a provider's website (YouTube, Vimeo, GIPHY) with an
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
    // WebKitGTK 2.52 with NVIDIA's EGL driver crashes on YouTube video pages
    // (WebKit's own MiniBrowser crashes the same way). Mesa's EGL avoids it;
    // without a GPU driver of its own it renders on the CPU, which is fine for
    // picking a video. A value the user set themselves is kept.
    #[cfg(target_os = "linux")]
    if std::env::var_os("__EGL_VENDOR_LIBRARY_FILENAMES").is_none()
        && let Some(mesa) = ["/usr/share/glvnd/egl_vendor.d/50_mesa.json", "/etc/glvnd/egl_vendor.d/50_mesa.json"]
            .into_iter()
            .find(|p| Path::new(p).exists())
    {
        cmd.env("__EGL_VENDOR_LIBRARY_FILENAMES", mesa);
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
            // The window alone: on Windows the keyboard stays out of the page.
            let _ = AsRef::<tauri::Webview>::as_ref(&w).set_focus();
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
/// A `context::` prefix (as in the interface's `t()`) is dropped when the
/// catalog has no entry.
pub(crate) fn tr(text: &dyn Fn(&str) -> String, key: &str) -> String {
    let out = text(key);
    match key.split_once("::") {
        Some((context, english))
            if out == key && context.starts_with(|c: char| c.is_ascii_lowercase()) && context.chars().all(|c| c.is_ascii_lowercase() || c == ' ') =>
        {
            english.to_string()
        }
        _ => out,
    }
}

/// Entry point of the `--media-browser` helper process.
fn run_media_browser(context: tauri::Context<tauri::Wry>, provider: String, lang: String, safe: bool) {
    let Some(start) = browse::start_url(&provider) else { std::process::exit(2) };
    let name = match provider.as_str() {
        "youtube" => "YouTube",
        "vimeo" => "Vimeo",
        _ => "GIPHY",
    };
    tauri::Builder::default()
        .setup(move |app| {
            let text = translator(app.handle(), &lang);
            let prov = provider.clone();
            let handle = app.handle().clone();
            let builder = window_builder(app, "media-browser", WebviewUrl::External(start.parse()?));
            let w = sized(builder, app.handle(), (1200.0, 840.0))
                .title(tr(&text, "{name} — pick something to insert into Cadenza").replace("{name}", name))
                .center()
                .initialization_script(browse::bar_script(&provider, &text))
                .on_navigation(move |url| {
                    if url.host_str() != Some(browse::PICK_HOST) {
                        return true;
                    }
                    use std::io::Write;
                    if let Some(picked) = browse::resolve(&prov, url)
                        && let Ok(json) = serde_json::to_string(&picked)
                    {
                        println!("{json}");
                    }
                    let _ = std::io::stdout().flush();
                    // Closing the window properly: a process ending while
                    // WebView2 is busy can crash the web engine the editor
                    // shares with it.
                    handle.exit(0);
                    false
                })
                .build()?;
            harden_browser(&w, safe);
            Ok(())
        })
        .build(context)
        .expect("error while running the media browser")
        .run(|_, event| {
            if let tauri::RunEvent::Exit = event {
                return_focus();
            }
        });
}

/// Lets the editor come back to the front when the media browser closes
/// (picked, cancelled or closed). Windows only lets the process the user last
/// clicked in bring a window forward, and that is this one.
fn return_focus() {
    #[cfg(windows)]
    // SAFETY: a plain Win32 call without pointers.
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
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
async fn media_import(window: Window, state: State<'_, AppState>, url: String) -> CmdResult<AssetInfo> {
    if !state.docs.lock().map_err(err)?.contains_key(window.label()) {
        return Err("No presentation is open".into());
    }
    let (bytes, ext) = tauri::async_runtime::spawn_blocking(move || media::download(&url))
        .await
        .map_err(err)?
        .map_err(err)?;
    state.with_doc(window.label(), |doc| doc.import_bytes(&bytes, &ext))
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
///
/// Commands that open windows are async: a synchronous command runs inside
/// the calling page's IPC handler, and on Windows building a WebView2 window
/// there deadlocks the whole app (see `WebviewWindowBuilder::new`).
#[tauri::command]
async fn export_pdf(app: AppHandle, window: Window) -> CmdResult<()> {
    let label = format!("print-{}", window.label());
    close_and_wait(&app, &[&label]).await;
    let url = format!("present.html?doc={}&print-pdf", util::encode_uri_component(window.label()));
    sized(window_builder(&app, &label, WebviewUrl::App(url.into())), &app, (1100.0, 800.0))
        .title("Cadenza — Print / Export PDF")
        .build()
        .map_err(err)?;
    Ok(())
}

/// Opens the print dialog for the print window's slides, with pages the size
/// of a slide and no margins. WebKitGTK leaves out the page size reveal.js
/// asks for (`@page`), so the slides came out on A4 portrait pages, black
/// below. False where the web engine prints at that size by itself: the page
/// calls `window.print()` there.
#[tauri::command]
fn print_slides(webview: tauri::Webview, width: f64, height: f64) -> bool {
    #[cfg(target_os = "linux")]
    {
        let _ = webview.with_webview(move |wv| {
            use gtk::prelude::*;
            use webkit2gtk::PrintOperationExt;
            let view = wv.inner();
            let op = webkit2gtk::PrintOperation::new(&view);
            // CSS pixels are 3/4 of a point. The paper has the slide's own
            // shape, upright: WebKit prints landscape pages blank.
            let paper = gtk::PaperSize::new_custom("cadenza-slide", "Slide", width * 0.75, height * 0.75, gtk::Unit::Points);
            let setup = gtk::PageSetup::new();
            setup.set_paper_size(&paper);
            for set in [
                gtk::PageSetup::set_top_margin,
                gtk::PageSetup::set_bottom_margin,
                gtk::PageSetup::set_left_margin,
                gtk::PageSetup::set_right_margin,
            ] {
                set(&setup, 0.0, gtk::Unit::Points);
            }
            op.set_page_setup(&setup);
            // Tests write a PDF without choosing it in the dialog.
            if let Some(uri) = std::env::var_os("CADENZA_PRINT_TO").filter(|_| cfg!(debug_assertions)) {
                let settings = gtk::PrintSettings::new();
                settings.set_printer("Print to File");
                settings.set(gtk::PRINT_SETTINGS_OUTPUT_URI, uri.to_str());
                settings.set(gtk::PRINT_SETTINGS_OUTPUT_FILE_FORMAT, Some("pdf"));
                op.set_print_settings(&settings);
            }
            let parent = view.toplevel().and_then(|w| w.downcast::<gtk::Window>().ok());
            op.run_dialog(parent.as_ref());
        });
        true
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (webview, width, height);
        false
    }
}

/// Writes the print window's slides to a PDF file, with pages the size of a
/// slide and no margins, headers or footers (Windows). The print dialog lays
/// slides out on the default printer's paper, with the browser's headers, and
/// Windows' own PDF printer only knows paper sizes.
#[tauri::command]
async fn print_to_pdf(webview: tauri::Webview, path: String, width: f64, height: f64) -> CmdResult<()> {
    #[cfg(windows)]
    {
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let failed = tx.clone();
        webview
            .with_webview(move |wv| {
                if let Err(e) = start_pdf(&wv, &path, width, height, tx) {
                    let _ = failed.send(Err(e));
                }
            })
            .map_err(err)?;
        tauri::async_runtime::spawn_blocking(move || rx.recv().unwrap_or_else(|_| Err("Could not write the PDF".into())))
            .await
            .map_err(err)?
    }
    #[cfg(not(windows))]
    {
        let _ = (webview, path, width, height);
        Err("Could not write the PDF".into())
    }
}

/// Starts WebView2's PDF printing; `done` gets the outcome.
#[cfg(windows)]
fn start_pdf(
    wv: &tauri::webview::PlatformWebview,
    path: &str,
    width: f64,
    height: f64,
    done: std::sync::mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT, ICoreWebView2_7, ICoreWebView2Environment6, ICoreWebView2PrintSettings,
    };
    use windows_core::{HSTRING, Interface};
    // SAFETY: WebView2 COM calls on the window's own thread (with_webview).
    unsafe {
        let view: ICoreWebView2_7 = wv.controller().CoreWebView2().and_then(|v| v.cast()).map_err(err)?;
        let env: ICoreWebView2Environment6 = wv.environment().cast().map_err(err)?;
        let settings = env.CreatePrintSettings().map_err(err)?;
        // CSS pixels are 1/96 inch.
        settings.SetPageWidth(width / 96.0).map_err(err)?;
        settings.SetPageHeight(height / 96.0).map_err(err)?;
        settings.SetOrientation(COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT).map_err(err)?;
        for set in [
            ICoreWebView2PrintSettings::SetMarginTop,
            ICoreWebView2PrintSettings::SetMarginBottom,
            ICoreWebView2PrintSettings::SetMarginLeft,
            ICoreWebView2PrintSettings::SetMarginRight,
        ] {
            set(&settings, 0.0).map_err(err)?;
        }
        settings.SetScaleFactor(1.0).map_err(err)?;
        settings.SetShouldPrintBackgrounds(true).map_err(err)?;
        settings.SetShouldPrintHeaderAndFooter(false).map_err(err)?;
        let handler = webview2_com::PrintToPdfCompletedHandler::create(Box::new(move |result, ok| {
            let _ = done.send(if result.is_ok() && ok {
                Ok(())
            } else {
                Err("Could not write the PDF".into())
            });
            Ok(())
        }));
        view.PrintToPdf(&HSTRING::from(path), &settings, &handler).map_err(err)?;
    }
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
    teleprompter: settings::Teleprompter,
}

/// Everything a presentation window needs to start reveal.js.
#[tauri::command]
fn present_data(window: Window, state: State<AppState>) -> CmdResult<PresentData> {
    let owner = owner_label(window.label()).to_string();
    let (language, teleprompter) = {
        let s = state.settings.lock().map_err(err)?;
        (s.language.clone(), s.teleprompter.clone())
    };
    let docs = state.docs.lock().map_err(err)?;
    let doc = docs.get(&owner).ok_or("The presentation is no longer open")?;
    let deck = &doc.deck;
    let assets = doc.assets_dir();
    let url = move |name: &str| document::file_url(&assets.join(name));
    let ctx = Ctx {
        mode: Mode::Show,
        asset_url: &url,
        youtube_host: embed::host(),
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
        language,
        teleprompter,
    })
}

/// Async, like every command that opens windows (see `export_pdf`).
#[tauri::command]
async fn present_start(app: AppHandle, window: Window, state: State<'_, AppState>, from_slide: Option<String>, presenter: Option<bool>) -> CmdResult<()> {
    let owner = window.label().to_string();
    // The teleprompter lives in the presenter display.
    let presenter = presenter.unwrap_or_else(|| state.settings.lock().map(|s| s.presenter_view || s.teleprompter.enabled).unwrap_or(true));
    let (h, v) = {
        let docs = state.docs.lock().map_err(err)?;
        let doc = docs.get(&owner).ok_or("No presentation is open")?;
        if render::visible_groups(&doc.deck).is_empty() {
            return Err("All slides are skipped — there is nothing to present.".into());
        }
        from_slide.as_deref().and_then(|id| render::slide_indices(&doc.deck, id)).unwrap_or((0, 0))
    };
    close_and_wait(&app, &[&format!("present-{owner}"), &format!("presenter-{owner}")]).await;

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
    let w = window_builder(&app, &present_label, WebviewUrl::App(url.into()))
        .title("Cadenza — Presentation")
        .visible(false)
        .decorations(false)
        .on_permission_request(camera_only)
        .build()
        .map_err(err)?;
    enable_camera(&w);
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
        let p = window_builder(&app, &pl, WebviewUrl::App(url.into()))
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
        // Keep keyboard focus on the screen the presenter is looking at. With
        // one screen the slides stay in front (the screen may be mirrored to a
        // projector); X brings the presenter display forward (present_swap).
        // Windows brings a window it shows to the front, hence the focus.
        if monitors.len() > 1 {
            let _ = p.set_focus();
        } else {
            let _ = w.set_focus();
        }
    }
    let _ = app.emit("cz-present-started", json!({ "doc": owner }));
    Ok(())
}

/// The other window of a show: the audience window for the presenter display
/// and vice versa.
fn swap_label(label: &str) -> Option<String> {
    let owner = owner_label(label);
    if label.starts_with("presenter-") {
        Some(format!("present-{owner}"))
    } else if label.starts_with("present-") {
        Some(format!("presenter-{owner}"))
    } else {
        None
    }
}

/// Brings the other window of the show to the front when both share one
/// screen (one screen: share the slides in a call, read the presenter display).
#[tauri::command]
fn present_swap(app: AppHandle, window: Window) -> CmdResult<()> {
    let Some(other) = swap_label(window.label()).and_then(|l| app.get_webview_window(&l)) else {
        return Ok(());
    };
    let here = window.current_monitor().map_err(err)?.map(|m| *m.position());
    let there = other.current_monitor().map_err(err)?.map(|m| *m.position());
    if here == there {
        other.set_focus().map_err(err)?;
    }
    Ok(())
}

/// Closes windows and waits (up to two seconds) until they are gone, so new
/// ones can take their labels: closing goes through the event loop.
async fn close_and_wait(app: &AppHandle, labels: &[&str]) {
    let open = || labels.iter().filter_map(|l| app.get_webview_window(l)).collect::<Vec<_>>();
    for w in open() {
        let _ = w.close();
    }
    for _ in 0..80 {
        if open().is_empty() {
            return;
        }
        let _ = tauri::async_runtime::spawn_blocking(|| std::thread::sleep(std::time::Duration::from_millis(25))).await;
    }
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
// Updates
// ---------------------------------------------------------------------------

/// The updater. Debug builds can read the release manifest from
/// `CADENZA_UPDATE_URL` instead of GitHub, to test updates locally.
fn updater(app: &AppHandle) -> anyhow::Result<tauri_plugin_updater::Updater> {
    use tauri_plugin_updater::UpdaterExt;
    let mut builder = app.updater_builder().timeout(std::time::Duration::from_secs(30));
    if cfg!(debug_assertions)
        && let Some(url) = std::env::var("CADENZA_UPDATE_URL").ok().filter(|u| !u.is_empty())
    {
        builder = builder.endpoints(vec![url.parse()?])?;
    }
    Ok(builder.build()?)
}

/// Looks for a newer version. The check at startup (`manual: false`) runs
/// once per launch when the settings allow it, says nothing when it fails
/// (no connection), and leaves out a version the person chose to skip.
#[tauri::command]
async fn update_check(app: AppHandle, state: State<'_, AppState>, manual: bool) -> CmdResult<Option<updates::UpdateInfo>> {
    if updates::package().is_none() {
        return if manual {
            Err("This copy of Cadenza can't update itself.".into())
        } else {
            Ok(None)
        };
    }
    let skipped = {
        let s = state.settings.lock().map_err(err)?;
        if !manual && (!s.check_updates || state.updates.checked_at_start.swap(true, Ordering::SeqCst)) {
            return Ok(None);
        }
        s.skipped_update.clone()
    };
    let found = match updater(&app).map_err(err)?.check().await {
        Ok(found) => found,
        Err(e) if manual => return Err(e.to_string()),
        Err(e) => {
            eprintln!("Could not check for updates: {e}");
            return Ok(None);
        }
    };
    let info = found.as_ref().map(updates::UpdateInfo::of);
    *state.updates.found.lock().map_err(err)? = found;
    Ok(info.filter(|i| manual || i.version != skipped))
}

/// Downloads the update found and verifies its signature, reporting progress
/// to the window that asked (`cz-update-progress`).
#[tauri::command]
async fn update_download(window: Window, state: State<'_, AppState>) -> CmdResult<()> {
    let update = state.updates.found.lock().map_err(err)?.clone().ok_or("No update was found.")?;
    if state.updates.has_downloaded(&update.version) {
        return Ok(());
    }
    let version = update.version.clone();
    let app = window.app_handle().clone();
    let label = window.label().to_string();
    let task = tauri::async_runtime::spawn(async move {
        let (mut done, mut sent) = (0u64, 0u64);
        let progress = move |n: usize, total: Option<u64>| {
            done += n as u64;
            // Often enough for a smooth bar, without flooding the window.
            let step = total.map_or(1 << 20, |t| (t / 200).max(1 << 16));
            if done - sent >= step || Some(done) == total {
                sent = done;
                let _ = app.emit_to(label.as_str(), "cz-update-progress", json!({ "done": done, "total": total }));
            }
        };
        update.download(progress, || {}).await
    });
    let abort = task.inner().abort_handle();
    *state.updates.cancel.lock().map_err(err)? = Some(Box::new(move || abort.abort()));
    let result = task.await;
    state.updates.cancel.lock().map_err(err)?.take();
    let bytes = result.map_err(|_| "Cancelled")?.map_err(err)?;
    *state.updates.downloaded.lock().map_err(err)? = Some((version, bytes));
    Ok(())
}

#[tauri::command]
fn update_cancel(state: State<AppState>) {
    if let Some(cancel) = state.updates.cancel.lock().ok().and_then(|mut c| c.take()) {
        cancel();
    }
}

/// What to reopen after restarting, window by window in the order they were
/// opened. Unsaved changes go into recovery snapshots first, so they survive
/// even an installation cut short.
fn reopen_list(state: &AppState) -> Vec<updates::Reopen> {
    let Ok(mut docs) = state.docs.lock() else { return vec![] };
    let mut labels: Vec<String> = docs.keys().cloned().collect();
    labels.sort_by_key(|l| l.strip_prefix("editor-").and_then(|n| n.parse::<u32>().ok()).unwrap_or(u32::MAX));
    labels
        .iter()
        .filter_map(|l| {
            let doc = docs.get_mut(l)?;
            if doc.is_dirty() {
                doc.maybe_write_recovery(true);
                Some(updates::Reopen::Session { id: doc.id.clone() })
            } else {
                doc.path.clone().map(|path| updates::Reopen::File { path })
            }
        })
        .collect()
}

/// Installs the downloaded update and restarts into it; the presentations
/// open now reopen afterwards. On Windows the installer takes over: it
/// closes Cadenza and starts the new version itself.
#[tauri::command]
async fn update_install(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let update = state.updates.found.lock().map_err(err)?.clone().ok_or("No update was found.")?;
    let bytes = match state.updates.downloaded.lock().map_err(err)?.take() {
        Some((version, bytes)) if version == update.version => bytes,
        _ => return Err("The update hasn't been downloaded.".into()),
    };
    updates::write_reopen(&state.config_dir, &reopen_list(&state)).map_err(err)?;
    state.updates.installing.store(true, Ordering::SeqCst);
    // The media browsers run Cadenza's own program, which Windows can't
    // replace while it runs.
    for label in app.webview_windows().into_keys().filter(|l| l.starts_with("editor-")) {
        stop_browser(&app, &label);
    }
    let version = update.version.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let result = update.install(&bytes);
        (result, bytes)
    })
    .await;
    let message = match result {
        Ok((Ok(()), _)) => {
            app.request_restart();
            return Ok(());
        }
        Ok((Err(e), bytes)) => {
            // Kept for another try.
            *state.updates.downloaded.lock().map_err(err)? = Some((version, bytes));
            e.to_string()
        }
        Err(e) => e.to_string(),
    };
    state.updates.installing.store(false, Ordering::SeqCst);
    updates::forget_reopen(&state.config_dir);
    Err(message)
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

fn open_editor_window(app: &AppHandle, file: Option<PathBuf>) -> tauri::Result<()> {
    open_editor(app, file.map(Pending::File))
}

fn open_editor(app: &AppHandle, pending: Option<Pending>) -> tauri::Result<()> {
    let state = app.state::<AppState>();
    let n = state.window_counter.fetch_add(1, Ordering::SeqCst);
    let label = format!("editor-{n}");
    if let Some(p) = pending
        && let Ok(mut map) = state.pending_open.lock()
    {
        map.insert(label.clone(), p);
    }
    sized(window_builder(app, &label, WebviewUrl::App("index.html".into())), app, (1440.0, 900.0))
        .title("Cadenza")
        .min_inner_size(900.0, 560.0)
        .center()
        .build()?;
    Ok(())
}

/// The audience window may use the camera (live video elements), never the
/// microphone. No other window gets the camera.
fn camera_only<R: tauri::Runtime>(_: tauri::Webview<R>, kind: tauri::webview::PermissionKind) -> tauri::webview::PermissionResponse {
    use tauri::webview::{PermissionKind, PermissionResponse};
    match kind {
        PermissionKind::Camera => PermissionResponse::Allow,
        PermissionKind::Microphone => PermissionResponse::Deny,
        _ => PermissionResponse::Default,
    }
}

/// WebKitGTK lets pages use the camera only with MediaStream enabled.
#[cfg(target_os = "linux")]
fn enable_camera(w: &tauri::WebviewWindow) {
    let _ = w.with_webview(|wv| {
        use webkit2gtk::{SettingsExt, WebViewExt};
        if let Some(settings) = WebViewExt::settings(&wv.inner()) {
            settings.set_enable_media_stream(true);
            // Test displays have no camera: WebKit can fake one.
            if cfg!(debug_assertions) && std::env::var_os("CADENZA_FAKE_CAMERA").is_some() {
                settings.set_enable_mock_capture_devices(true);
            }
        }
    });
}

#[cfg(not(target_os = "linux"))]
fn enable_camera(_w: &tauri::WebviewWindow) {}

/// Async, like every command that opens windows (see `export_pdf`).
#[tauri::command]
async fn window_new(app: AppHandle, path: Option<String>) -> CmdResult<()> {
    open_editor_window(&app, path.map(PathBuf::from)).map_err(err)
}

/// Focuses the editor window that already has `path` open, if any.
#[tauri::command]
fn window_focus_document(app: AppHandle, state: State<AppState>, path: String) -> CmdResult<bool> {
    let target = PathBuf::from(&path);
    let docs = state.docs.lock().map_err(err)?;
    for (label, doc) in docs.iter() {
        if doc.path.as_deref() == Some(target.as_path())
            && let Some(w) = app.get_webview_window(label)
        {
            let _ = w.unminimize();
            let _ = w.set_focus();
            return Ok(true);
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
        .filter(|p| is_presentation(p))
        .collect()
}

fn is_presentation(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == package::EXTENSION) && path.exists()
}

/// Files another launch asked to open while Cadenza was starting (before
/// setup has run); `None` once it has started.
static STARTUP_FILES: Mutex<Option<Vec<PathBuf>>> = Mutex::new(Some(Vec::new()));

/// Opens presentations in new windows, or keeps them for startup.
fn open_files(app: &AppHandle, files: Vec<PathBuf>) {
    if let Ok(mut startup) = STARTUP_FILES.lock()
        && let Some(pending) = startup.as_mut()
    {
        pending.extend(files);
        return;
    }
    for f in files {
        let _ = open_editor_window(app, Some(f));
    }
}

/// Quitting with presentations still open (logging out, shutting down):
/// unsaved work is kept for recovery at the next start.
fn keep_unsaved_work(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>()
        && let Ok(mut docs) = state.docs.lock()
    {
        for doc in docs.values_mut() {
            doc.maybe_write_recovery(true);
        }
    }
}

/// The free part of the screen new windows open on (without taskbar or
/// dock), in logical pixels: the screen of the focused window, else the main
/// one.
fn work_area<R: tauri::Runtime>(app: &AppHandle<R>) -> Option<(f64, f64)> {
    let focused = app.webview_windows().into_values().find(|w| w.is_focused().unwrap_or(false));
    let monitor = focused
        .and_then(|w| w.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    Some((area.size.width as f64 / scale, area.size.height as f64 / scale))
}

/// A new window `w`×`h` big, or maximized on a screen too small for that:
/// its bottom would hide behind the taskbar (small laptops, large text
/// scaling). Un-maximized, it then fits the screen.
fn sized<'a, R: tauri::Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
    app: &AppHandle<R>,
    (w, h): (f64, f64),
) -> WebviewWindowBuilder<'a, R, M> {
    // Room for the title bar and the window's border.
    let Some((free_w, free_h)) = work_area(app).map(|(fw, fh)| (fw - 16.0, fh - 48.0)) else {
        return builder.inner_size(w, h);
    };
    builder.inner_size(w.min(free_w), h.min(free_h)).maximized(w > free_w || h > free_h)
}

/// `CADENZA_DATA_DIR` gives Cadenza an isolated profile (settings and
/// sessions) and allows several instances — handy for development and
/// portable installs.
fn data_dir_override() -> Option<PathBuf> {
    std::env::var_os("CADENZA_DATA_DIR").filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// A new window. An isolated profile (`CADENZA_DATA_DIR`) keeps the pages'
/// own storage there too, apart from the installed app's.
fn window_builder<'a, R: tauri::Runtime, M: Manager<R>>(manager: &'a M, label: &str, url: WebviewUrl) -> WebviewWindowBuilder<'a, R, M> {
    let builder = WebviewWindowBuilder::new(manager, label, url);
    match data_dir_override() {
        Some(dir) => builder.data_directory(dir.join("webview")),
        None => builder,
    }
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
            open_files(app, files);
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let (cache, config) = match data_dir_override() {
                Some(dir) => (dir.join("cache"), dir.clone()),
                None => (app.path().app_cache_dir()?, app.path().app_config_dir()?),
            };
            let sessions_root = cache.join("sessions");
            std::fs::create_dir_all(&sessions_root)?;
            // Media of open documents is served to the webview from here.
            app.asset_protocol_scope().allow_directory(&sessions_root, true)?;
            embed::allow_dir(&sessions_root);
            let mut recoverable = document::list_recoverable(&sessions_root);
            // After an update: the presentations open before the restart.
            let mut reopen_files = Vec::new();
            let mut reopen_sessions = Vec::new();
            for r in updates::take_reopen(&config) {
                match r {
                    updates::Reopen::File { path } if is_presentation(&path) => reopen_files.push(path),
                    updates::Reopen::Session { id } if recoverable.iter().any(|r| r.session_id == id) => reopen_sessions.push(id),
                    _ => {}
                }
            }
            recoverable.retain(|r| !reopen_sessions.contains(&r.session_id));
            let settings_path = config.join("settings.json");
            let forget_ai_settings = settings::has_ai_settings(&settings_path);
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
                clipboard: clipboard::Store::new(cache.join("clipboard")),
                updates: updates::Updates::default(),
            });
            fonts::warm_up();
            // Credential cleanup has its own completion marker and retries on
            // later launches even after the old AI settings have been removed.
            // Keep keychain access in the background because it can be slow.
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                let state = app_handle.state::<AppState>();
                if let Err(e) = state.secrets.forget_retired() {
                    eprintln!("Could not remove retired API keys; will retry on next launch: {e}");
                }
                if forget_ai_settings && let Ok(s) = state.settings.lock() {
                    let _ = s.save(&state.settings_path);
                }
                if cfg!(windows) {
                    updates::remove_old_installers(&std::env::temp_dir(), &app_handle.package_info().name);
                }
            });

            let cwd = std::env::current_dir().unwrap_or_default();
            let mut files = files_from_args(std::env::args(), &cwd);
            files.extend(STARTUP_FILES.lock().ok().and_then(|mut f| f.take()).unwrap_or_default());
            for f in reopen_files {
                if !files.contains(&f) {
                    files.push(f);
                }
            }
            if files.is_empty() && reopen_sessions.is_empty() {
                open_editor_window(app.handle(), None)?;
            }
            for f in files {
                open_editor_window(app.handle(), Some(f))?;
            }
            for id in reopen_sessions {
                open_editor(app.handle(), Some(Pending::Session(id)))?;
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
                    let doc = state.docs.lock().ok().and_then(|mut d| d.remove(&label));
                    // Closed for an update: the session reopens after the restart.
                    if let Some(doc) = doc
                        && !state.updates.installing.load(Ordering::SeqCst)
                    {
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
            map_countries,
            map_regions,
            chart_previews,
            read_data_file,
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
            asset_import_clipboard_image,
            font_install,
            themes_user,
            theme_from_builtin,
            theme_save,
            theme_delete,
            theme_apply,
            theme_export,
            theme_import,
            theme_preview,
            secret_status,
            secret_set,
            secret_delete,
            media_search,
            media_oembed,
            media_link,
            media_browse,
            media_import,
            import_markdown,
            export_html,
            export_pdf,
            print_slides,
            print_to_pdf,
            present_data,
            present_start,
            present_stop,
            present_swap,
            present_toggle_fullscreen,
            ui_log,
            window_new,
            window_focus_document,
            update_check,
            update_download,
            update_cancel,
            update_install,
        ])
        .build(context)
        .expect("error while running Cadenza")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                keep_unsaved_work(app);
            }
        });
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
    fn swap_labels() {
        assert_eq!(swap_label("presenter-editor-1").as_deref(), Some("present-editor-1"));
        assert_eq!(swap_label("present-editor-1").as_deref(), Some("presenter-editor-1"));
        assert_eq!(swap_label("editor-1"), None);
        assert_eq!(swap_label("print-editor-1"), None);
    }

    #[test]
    fn presentations_are_saved_with_one_extension() {
        let p = |s: &str| presentation_path(PathBuf::from(s));
        assert_eq!(p("/t/talk"), PathBuf::from("/t/talk.cadenza"));
        assert_eq!(p("/t/talk.cadenza"), PathBuf::from("/t/talk.cadenza"));
        assert_eq!(p("/t/talk.cadenza.cadenza"), PathBuf::from("/t/talk.cadenza"));
        assert_eq!(p("/t/talk.v2"), PathBuf::from("/t/talk.v2"));
        assert_eq!(p("/t/.cadenza.cadenza"), PathBuf::from("/t/.cadenza.cadenza"));
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
