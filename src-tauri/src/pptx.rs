//! PPTX export jobs pin document data and assets until conversion finishes.
//! The mapper runs in JavaScript; native capture remains the shared renderer.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager, State, Webview, WebviewUrl, Window};

use crate::{AppState, CmdResult, document, err, model::Deck, render, util, window_builder};

const fn n_(message: &'static str) -> &'static str {
    message
}

#[derive(Default)]
pub struct Exports {
    jobs: Mutex<HashMap<String, Arc<Job>>>,
    captures: Mutex<HashMap<String, PendingCapture>>,
}

struct Job {
    id: String,
    owner: String,
    deck: Deck,
    assets: PathBuf,
}
impl Drop for Job {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.assets.parent().unwrap_or(&self.assets));
    }
}
struct PendingCapture {
    job: Arc<Job>,
    slide: String,
    element: Option<String>,
    background: bool,
    done: mpsc::Sender<CmdResult<Raster>>,
}

#[derive(Serialize)]
pub struct Raster {
    data: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct CaptureBounds {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    scale: f64,
    viewport_width: f64,
}

impl Exports {
    /// Release pinned assets and pending captures when an editor or capture is closed.
    pub fn window_closed(&self, app: &AppHandle, label: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.retain(|_, job| job.owner != label);
        }
        let labels = if let Ok(mut pending) = self.captures.lock() {
            let labels: Vec<_> = pending
                .iter()
                .filter(|(key, p)| key.as_str() == label || p.job.owner == label)
                .map(|(key, _)| key.clone())
                .collect();
            for key in &labels {
                if let Some(p) = pending.remove(key) {
                    let _ = p.done.send(Err("Export cancelled".into()));
                }
            }
            labels
        } else {
            Vec::new()
        };
        for key in labels {
            if key != label
                && let Some(w) = app.get_webview_window(&key)
            {
                let _ = w.close();
            }
        }
    }
    fn get(&self, owner: &str, id: &str) -> CmdResult<Arc<Job>> {
        self.jobs
            .lock()
            .map_err(err)?
            .get(id)
            .filter(|j| j.owner == owner)
            .cloned()
            .ok_or_else(|| n_("This export is no longer active").into())
    }
}

/// Snapshot while the document lock still pins its session. Hard links avoid
/// copying large local media; replacing/deleting originals cannot change them.
#[tauri::command]
pub fn pptx_begin_export(window: Window, state: State<AppState>, doc_id: String) -> CmdResult<Value> {
    let job = state.with_doc_id(window.label(), &doc_id, |doc| {
        let id = uuid::Uuid::new_v4().to_string();
        let assets = state.sessions_root.join(format!("export-{id}")).join("assets");
        std::fs::create_dir_all(&assets)?;
        let job = Arc::new(Job {
            id,
            owner: window.label().into(),
            deck: doc.deck.clone(),
            assets,
        });
        for name in job.deck.retained_assets() {
            let source = doc.assets_dir().join(&name);
            let dest = job.assets.join(&name);
            if std::fs::hard_link(&source, &dest).is_err() {
                std::fs::copy(&source, &dest)?;
            }
        }
        Ok(job)
    })?;
    let url = |name: &str| document::file_url(&job.assets.join(name));
    let ctx = render::Ctx {
        mode: render::Mode::Editor,
        asset_url: &url,
        youtube_host: None,
    };
    let slides: Vec<_> = job
        .deck
        .slides
        .iter()
        .map(|slide| {
            let rendered = render::slide_for_editor(&job.deck, slide, &ctx);
            json!({ "id": slide.id, "html": rendered.html, "classes": rendered.classes })
        })
        .collect();
    let snapshot = json!({ "jobId": job.id, "deck": job.deck, "width": job.deck.width, "height": job.deck.height,
        "themeCss": job.deck.theme_href().unwrap_or(""), "deckCss": render::deck_css(&job.deck, &ctx), "slides": slides });
    state.exports.jobs.lock().map_err(err)?.insert(job.id.clone(), job);
    Ok(snapshot)
}

#[tauri::command]
pub fn pptx_export_asset(window: Window, state: State<AppState>, job_id: String, name: String) -> CmdResult<String> {
    let job = state.exports.get(window.label(), &job_id)?;
    if !util::is_valid_asset_name(&name) || !job.deck.retained_assets().contains(&name) {
        return Err("Unknown export asset".into());
    }
    let path = job.assets.join(&name);
    if std::fs::metadata(&path).map_err(err)?.len() > 128 * 1024 * 1024 {
        return Err("The export asset is too large".into());
    }
    let bytes = std::fs::read(path).map_err(err)?;
    Ok(format!(
        "data:{};base64,{}",
        mime_guess::from_path(name).first_or_octet_stream(),
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub async fn pptx_capture(
    app: AppHandle,
    window: Window,
    state: State<'_, AppState>,
    job_id: String,
    slide_id: String,
    element_id: Option<String>,
    background: Option<bool>,
) -> CmdResult<Raster> {
    let job = state.exports.get(window.label(), &job_id)?;
    let slide = job.deck.slides.iter().find(|s| s.id == slide_id).ok_or("Unknown export slide")?;
    if element_id.as_ref().is_some_and(|id| !slide.elements.iter().any(|e| &e.id == id)) {
        return Err("Unknown export object".into());
    }
    let label = format!("capture-{}", uuid::Uuid::new_v4());
    let (tx, rx) = mpsc::channel();
    let size = (job.deck.width.min(1280) as f64, job.deck.height.min(720) as f64);
    state.exports.captures.lock().map_err(err)?.insert(
        label.clone(),
        PendingCapture {
            job,
            slide: slide_id,
            element: element_id,
            background: background.unwrap_or(false),
            done: tx,
        },
    );
    // Cancellation can race the get/snapshot step. Do not leave a capture
    // window behind if its job was ended before the window was built.
    if state.exports.get(window.label(), &job_id).is_err() {
        state.exports.captures.lock().map_err(err)?.remove(&label);
        return Err("Export cancelled".into());
    }
    let opened = window_builder(&app, &label, WebviewUrl::App("pptx-capture.html".into()))
        .title("Cadenza — Exporting PowerPoint")
        .inner_size(size.0, size.1)
        .min_inner_size(100.0, 100.0)
        .decorations(false)
        .transparent(true)
        .resizable(false)
        // Realized windows are required by both native snapshot backends, but
        // capturing must not steal keyboard focus from the export's Cancel UI.
        .focused(false)
        .skip_taskbar(true)
        .build();
    if let Err(error) = opened {
        state.exports.captures.lock().map_err(err)?.remove(&label);
        return Err(error.to_string());
    }
    if !state.exports.captures.lock().map_err(err)?.contains_key(&label) {
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.close();
        }
        return Err("Export cancelled".into());
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        rx.recv_timeout(Duration::from_secs(40))
            .unwrap_or_else(|_| Err("The slide image could not be prepared in time".into()))
    })
    .await
    .map_err(err)?;
    state.exports.captures.lock().map_err(err)?.remove(&label);
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.close();
    }
    result
}

#[tauri::command]
pub fn pptx_capture_data(window: Window, state: State<AppState>) -> CmdResult<Value> {
    let captures = state.exports.captures.lock().map_err(err)?;
    let pending = captures.get(window.label()).ok_or("Unknown export capture")?;
    let job = &pending.job;
    let slide = job.deck.slides.iter().find(|s| s.id == pending.slide).ok_or("Unknown export slide")?;
    let url = |name: &str| document::file_url(&job.assets.join(name));
    let ctx = render::Ctx {
        mode: render::Mode::Editor,
        asset_url: &url,
        youtube_host: None,
    };
    let rendered = render::slide_for_editor(&job.deck, slide, &ctx);
    Ok(
        json!({ "width": job.deck.width, "height": job.deck.height, "html": rendered.html, "classes": rendered.classes,
        "themeCss": job.deck.theme_href().unwrap_or(""), "deckCss": render::deck_css(&job.deck, &ctx),
        "codeTheme": job.deck.settings.code_theme, "elementId": pending.element, "background": pending.background }),
    )
}

#[tauri::command]
pub async fn pptx_capture_ready(webview: Webview, state: State<'_, AppState>, bounds: CaptureBounds, error: Option<String>) -> CmdResult<()> {
    let done = state
        .exports
        .captures
        .lock()
        .map_err(err)?
        .get(webview.label())
        .map(|p| p.done.clone())
        .ok_or("Unknown export capture")?;
    let result = if let Some(error) = error {
        Err(error)
    } else {
        match crate::capture::png(&webview).await {
            Ok(bytes) => crop_capture(&bytes, &bounds),
            Err(error) => Err(error),
        }
    };
    let _ = done.send(result);
    Ok(())
}

fn crop_capture(bytes: &[u8], bounds: &CaptureBounds) -> CmdResult<Raster> {
    let CaptureBounds {
        x,
        y,
        w,
        h,
        scale,
        viewport_width,
    } = *bounds;
    if [x, y, w, h, scale, viewport_width].iter().any(|n| !n.is_finite()) || x < 0.0 || y < 0.0 || w <= 0.0 || h <= 0.0 || scale <= 0.0 || viewport_width <= 0.0
    {
        return Err("Invalid export image bounds".into());
    }
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(err)?;
    if u64::from(reader.info().width) * u64::from(reader.info().height) > 16_777_216 {
        return Err("The export image is too large".into());
    }
    let length = reader
        .output_buffer_size()
        .filter(|n| *n <= 80 * 1024 * 1024)
        .ok_or("The export image is too large")?;
    let mut buffer = vec![0; length];
    let info = reader.next_frame(&mut buffer).map_err(err)?;
    let ratio = f64::from(info.width) / viewport_width;
    let left = (x * ratio).floor() as u32;
    let top = (y * ratio).floor() as u32;
    let width = ((w * ratio).ceil() as u32).min(info.width.saturating_sub(left));
    let height = ((h * ratio).ceil() as u32).min(info.height.saturating_sub(top));
    if width == 0 || height == 0 {
        return Err("The export object is outside the slide".into());
    }
    let channels = info.color_type.samples();
    let mut cropped = Vec::with_capacity(width as usize * height as usize * channels);
    for row in top..top + height {
        let start = (row as usize * info.width as usize + left as usize) * channels;
        cropped.extend_from_slice(&buffer[start..start + width as usize * channels]);
    }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(info.color_type);
        encoder.set_depth(info.bit_depth);
        encoder.write_header().map_err(err)?.write_image_data(&cropped).map_err(err)?;
    }
    Ok(Raster {
        data: format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(out)),
        x: x / scale,
        y: y / scale,
        w: w / scale,
        h: h / scale,
    })
}

#[tauri::command]
pub fn pptx_end_export(app: AppHandle, window: Window, state: State<AppState>, job_id: String) -> CmdResult<()> {
    let job = state.exports.get(window.label(), &job_id)?;
    state.exports.jobs.lock().map_err(err)?.remove(&job_id);
    let mut pending = state.exports.captures.lock().map_err(err)?;
    let labels: Vec<_> = pending.iter().filter(|(_, p)| p.job.id == job.id).map(|(label, _)| label.clone()).collect();
    for label in &labels {
        if let Some(p) = pending.remove(label) {
            let _ = p.done.send(Err("Export cancelled".into()));
        }
    }
    drop(pending);
    for label in labels {
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.close();
        }
    }
    Ok(())
}

/// Binary IPC keeps large presentations out of JSON/base64 transport.
#[tauri::command]
pub fn pptx_save(window: Window, state: State<AppState>, request: tauri::ipc::Request<'_>) -> CmdResult<String> {
    let id = request.headers().get("x-export-id").and_then(|v| v.to_str().ok()).ok_or("Missing export id")?;
    let _job = state.exports.get(window.label(), id)?;
    let path = request
        .headers()
        .get("x-export-path")
        .and_then(|v| v.to_str().ok())
        .ok_or("Missing output path")?;
    let path = String::from_utf8(base64::engine::general_purpose::STANDARD.decode(path).map_err(err)?).map_err(err)?;
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Expected a binary presentation".into());
    };
    if bytes.len() > 512 * 1024 * 1024 || !bytes.starts_with(b"PK\x03\x04") {
        return Err("Invalid PowerPoint output".into());
    }
    let path = crate::with_extension_once(PathBuf::from(path), "pptx");
    util::write_atomic(&path, bytes, false).map_err(err)?;
    Ok(path.to_string_lossy().into_owned())
}

/// Convert in isolated storage before publishing a new, unsaved editor document.
#[tauri::command]
pub async fn import_pptx(app: AppHandle, window: Window, state: State<'_, AppState>, path: String, doc_id: Option<String>) -> CmdResult<()> {
    if let Some(id) = &doc_id {
        state.with_doc_id(window.label(), id, |_| Ok(()))?;
    }
    let root = state.sessions_root.clone();
    let doc = tauri::async_runtime::spawn_blocking(move || prepare_import(&root, std::path::Path::new(&path)))
        .await
        .map_err(err)??;
    if app.get_webview_window(window.label()).is_none() {
        doc.dispose();
        return Err("The importing window was closed".into());
    }
    if let Some(id) = &doc_id
        && let Err(error) = state.with_doc_id(window.label(), id, |_| Ok(()))
    {
        doc.dispose();
        return Err(error);
    }
    crate::open_editor(&app, Some(crate::Pending::Imported(Box::new(doc)))).map_err(err)
}

fn prepare_import(root: &std::path::Path, path: &std::path::Path) -> CmdResult<document::Document> {
    let staging = root.join(format!("import-{}", uuid::Uuid::new_v4()));
    let result = (|| -> anyhow::Result<document::Document> {
        let candidate = crate::pptx_import::parse(path, &staging)?;
        let doc = document::Document::from_import(root, candidate.deck)?;
        let copied = (|| -> anyhow::Result<()> {
            for name in doc.deck.retained_assets() {
                std::fs::rename(staging.join(&name), doc.assets_dir().join(&name))?;
            }
            Ok(())
        })();
        if let Err(error) = copied {
            doc.dispose();
            return Err(error);
        }
        Ok(doc)
    })();
    let _ = std::fs::remove_dir_all(staging);
    result.map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let pixels: Vec<u8> = (0..height).flat_map(|y| (0..width).flat_map(move |x| [x as u8, y as u8, 90, 128])).collect();
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        bytes
    }
    fn bounds() -> CaptureBounds {
        CaptureBounds {
            x: 2.0,
            y: 1.0,
            w: 3.0,
            h: 2.0,
            scale: 0.5,
            viewport_width: 8.0,
        }
    }
    #[test]
    fn crop_respects_device_scale_and_preserves_alpha() {
        let raster = crop_capture(&png(16, 8), &bounds()).unwrap();
        assert_eq!((raster.x, raster.y, raster.w, raster.h), (4.0, 2.0, 6.0, 4.0));
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(raster.data.split_once(',').unwrap().1)
            .unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!((info.width, info.height), (6, 4));
        assert_eq!(&pixels[..4], &[4, 2, 90, 128]);
        assert_eq!(&pixels[pixels.len() - 4..], &[9, 5, 90, 128]);
    }
    #[test]
    fn capture_rejects_invalid_and_outside_bounds() {
        let bytes = png(16, 8);
        for b in [
            CaptureBounds { x: f64::NAN, ..bounds() },
            CaptureBounds { scale: 0.0, ..bounds() },
            CaptureBounds { w: -1.0, ..bounds() },
            CaptureBounds { x: 99.0, ..bounds() },
            CaptureBounds { y: 99.0, ..bounds() },
        ] {
            assert!(crop_capture(&bytes, &b).is_err());
        }
        assert!(crop_capture(b"not a png", &bounds()).is_err());
    }
    #[test]
    fn export_job_pins_assets_until_last_reference_is_released() {
        let root = tempfile::tempdir().unwrap();
        let assets = root.path().join("export-test/assets");
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(assets.join("example.png"), b"retained media").unwrap();
        let exports = Exports::default();
        let job = Arc::new(Job {
            id: "job".into(),
            owner: "editor-1".into(),
            deck: Deck::default(),
            assets: assets.clone(),
        });
        exports.jobs.lock().unwrap().insert(job.id.clone(), job);
        assert!(exports.get("editor-2", "job").is_err());
        let pending = exports.get("editor-1", "job").unwrap();
        exports.jobs.lock().unwrap().remove("job");
        assert!(assets.exists());
        drop(pending);
        assert!(!assets.parent().unwrap().exists());
    }
    #[test]
    fn imported_document_is_dirty_isolated_and_reopens_with_report() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("foreign-libreoffice.pptx");
        std::fs::write(&source, include_bytes!("../../tools/fixtures/pptx/foreign-libreoffice.pptx")).unwrap();
        let original = std::fs::read(&source).unwrap();
        let mut doc = prepare_import(root.path(), &source).unwrap();
        assert!(doc.is_dirty());
        assert!(doc.path.is_none());
        assert!(!doc.deck.import_issues.is_empty());
        let before = serde_json::to_value(&doc.deck).unwrap();
        let path = root.path().join("converted.cadenza");
        doc.save(&path).unwrap();
        let reopened = document::Document::open(root.path(), &path).unwrap();
        fn same(a: &Value, b: &Value, path: &str) {
            match (a, b) {
                (Value::Number(a), Value::Number(b)) => assert!((a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-8, "{path}: {a} != {b}"),
                (Value::Array(a), Value::Array(b)) => {
                    assert_eq!(a.len(), b.len(), "{path}");
                    for (i, (a, b)) in a.iter().zip(b).enumerate() {
                        same(a, b, &format!("{path}[{i}]"));
                    }
                }
                (Value::Object(a), Value::Object(b)) => {
                    assert_eq!(a.len(), b.len(), "{path}");
                    for (key, a) in a {
                        same(a, b.get(key).unwrap(), &format!("{path}.{key}"));
                    }
                }
                _ => assert_eq!(a, b, "{path}"),
            }
        }
        same(&serde_json::to_value(&reopened.deck).unwrap(), &before, "deck");
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert!(
            !std::fs::read_dir(root.path())
                .unwrap()
                .any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with("import-"))
        );
        reopened.dispose();
        doc.dispose();
    }
    #[test]
    fn failed_import_does_not_leave_partial_sessions() {
        let root = tempfile::tempdir().unwrap();
        assert!(prepare_import(root.path(), &root.path().join("missing.pptx")).is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
