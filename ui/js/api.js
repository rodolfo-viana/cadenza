// Thin wrappers over the Tauri JavaScript API (exposed as window.__TAURI__).

const T = window.__TAURI__;

export const invoke = (cmd, args) => T.core.invoke(cmd, args);

/** Sends raw bytes (e.g. a pasted image) to Rust without JSON encoding. */
export const importBytes = (bytes, ext, docId) =>
  T.core.invoke('asset_import_bytes', bytes, { headers: { 'x-ext': ext || '', 'x-doc-id': docId } });

export const dialog = T.dialog;
export const events = T.event;
export const opener = T.opener;
export const clipboard = T.clipboardManager;
export const currentWindow = () => T.window.getCurrentWindow();
export const currentWebview = () => T.webview.getCurrentWebview();

export const FILE_FILTER = [{ name: 'Cadenza Presentation', extensions: ['cadenza'] }];
export const IMAGE_EXTS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'avif', 'bmp'];
export const VIDEO_EXTS = ['mp4', 'webm', 'ogv', 'mov', 'm4v'];
