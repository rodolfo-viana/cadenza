//! Raster fallback through the same native WebView that paints Cadenza.
//! Call only after the export page has loaded fonts/images and settled layout.

use std::sync::mpsc;
use std::time::Duration;

const MAX_PIXELS: u64 = 16_777_216;
const MAX_BYTES: u64 = 80 * 1024 * 1024;

const fn n_(message: &'static str) -> &'static str {
    message
}

/// Capture the calling export WebView, with a bounded asynchronous lifetime.
pub async fn png(webview: &tauri::Webview) -> Result<Vec<u8>, String> {
    let (tx, rx) = mpsc::channel();
    let failed = tx.clone();
    webview
        .with_webview(move |wv| {
            if let Err(error) = start(&wv, tx) {
                let _ = failed.send(Err(error));
            }
        })
        .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        rx.recv_timeout(Duration::from_secs(20))
            .map_err(|_| n_("The export image could not be captured in time").to_string())?
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(target_os = "linux")]
fn start(wv: &tauri::webview::PlatformWebview, done: mpsc::Sender<Result<Vec<u8>, String>>) -> Result<(), String> {
    use webkit2gtk::WebViewExt;
    wv.inner().snapshot(
        webkit2gtk::SnapshotRegion::Visible,
        webkit2gtk::SnapshotOptions::TRANSPARENT_BACKGROUND,
        None::<&gtk::gio::Cancellable>,
        move |result| {
            let result = result.map_err(|e| e.to_string()).and_then(|surface| {
                let image = surface.map_to_image(None).map_err(|e| e.to_string())?;
                let (width, height, stride) = (image.width() as u32, image.height() as u32, image.stride() as usize);
                if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
                    return Err(n_("The export image is too large").into());
                }
                let alpha = image.format() == gtk::cairo::Format::ARgb32;
                let mut rgba = vec![0; width as usize * height as usize * 4];
                image
                    .with_data(|bytes| {
                        for y in 0..height as usize {
                            for x in 0..width as usize {
                                let from = y * stride + x * 4;
                                let pixel = u32::from_ne_bytes(bytes[from..from + 4].try_into().unwrap());
                                let a = if alpha { (pixel >> 24) as u8 } else { 255 };
                                let channels = [(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8];
                                let to = (y * width as usize + x) * 4;
                                for (i, channel) in channels.into_iter().enumerate() {
                                    rgba[to + i] = if a == 0 {
                                        0
                                    } else {
                                        ((u32::from(channel) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
                                    };
                                }
                                rgba[to + 3] = a;
                            }
                        }
                    })
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                {
                    let mut encoder = ::png::Encoder::new(&mut out, width, height);
                    encoder.set_color(::png::ColorType::Rgba);
                    encoder.set_depth(::png::BitDepth::Eight);
                    encoder
                        .write_header()
                        .map_err(|e| e.to_string())?
                        .write_image_data(&rgba)
                        .map_err(|e| e.to_string())?;
                }
                if out.len() as u64 > MAX_BYTES {
                    return Err(n_("The export image is too large").into());
                }
                Ok(out)
            });
            let _ = done.send(result);
        },
    );
    Ok(())
}

#[cfg(windows)]
fn start(wv: &tauri::webview::PlatformWebview, done: mpsc::Sender<Result<Vec<u8>, String>>) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG;
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::Com::{STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET, StructuredStorage::CreateStreamOnHGlobal};
    // SAFETY: COM calls and the completion handler run on this WebView's UI thread.
    unsafe {
        let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true).map_err(|e| e.to_string())?;
        let read = stream.clone();
        let handler = webview2_com::CapturePreviewCompletedHandler::create(Box::new(move |status| {
            let result = (|| {
                status.map_err(|e| e.to_string())?;
                let mut stat = STATSTG::default();
                read.Stat(&mut stat, STATFLAG_NONAME).map_err(|e| e.to_string())?;
                if stat.cbSize == 0 || stat.cbSize > MAX_BYTES {
                    return Err(n_("The export image is too large").to_string());
                }
                read.Seek(0, STREAM_SEEK_SET, None).map_err(|e| e.to_string())?;
                let mut bytes = vec![0u8; stat.cbSize as usize];
                let mut count = 0;
                read.Read(bytes.as_mut_ptr().cast(), bytes.len() as u32, Some(&mut count))
                    .ok()
                    .map_err(|e| e.to_string())?;
                if count as usize != bytes.len() {
                    return Err(n_("The export image is incomplete").to_string());
                }
                let size = imagesize::blob_size(&bytes).map_err(|e| e.to_string())?;
                if size.width as u64 * size.height as u64 > MAX_PIXELS {
                    return Err(n_("The export image is too large").to_string());
                }
                Ok(bytes)
            })();
            let _ = done.send(result);
            Ok(())
        }));
        wv.controller()
            .CoreWebView2()
            .and_then(|view| view.CapturePreview(COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG, &stream, &handler))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(not(any(windows, target_os = "linux")))]
fn start(_: &tauri::webview::PlatformWebview, _: mpsc::Sender<Result<Vec<u8>, String>>) -> Result<(), String> {
    Err(n_("Native slide capture is not available on this platform").into())
}
