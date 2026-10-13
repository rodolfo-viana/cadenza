//! The macOS menu bar.
//!
//! Mac apps are expected to have one, and the webview relies on its Edit
//! menu for undo, cut, copy and paste in text fields. The other items run
//! Cadenza's own commands (ui/js/commands.js) in the frontmost window. Their
//! shortcuts are the same as in the interface: a web page sees each key press
//! before the menu does, and the menu only gets the keys Cadenza leaves alone.

use tauri::menu::{Menu, MenuEvent, MenuItem, Submenu, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, Wry};

use crate::{owner_label, tr, translator};

const QUIT: &str = "cz.quit";
const MINIMIZE: &str = "cz.minimize";

/// The menu bar in the interface language.
pub fn build(app: &AppHandle, lang: &str) -> tauri::Result<Menu<Wry>> {
    let text = translator(app, lang);
    // Item ids are Cadenza command ids, except QUIT and MINIMIZE.
    let item = |id: &str, label: String, keys: Option<&str>| MenuItem::with_id(app, id, label, true, keys);

    let cadenza = SubmenuBuilder::new(app, "Cadenza")
        .item(&item("app.about", tr(&text, "About Cadenza"), None)?)
        .separator()
        .item(&item("app.settings", tr(&text, "Settings…"), Some("CmdOrCtrl+,"))?)
        .separator()
        .services_with_text(tr(&text, "Services"))
        .separator()
        .hide_with_text(tr(&text, "Hide Cadenza"))
        .hide_others_with_text(tr(&text, "Hide Others"))
        .show_all_with_text(tr(&text, "Show All"))
        .separator()
        .item(&item(QUIT, tr(&text, "Quit Cadenza"), Some("CmdOrCtrl+Q"))?)
        .build()?;
    let file = SubmenuBuilder::new(app, tr(&text, "File"))
        .item(&item("file.new", tr(&text, "New Presentation…"), Some("CmdOrCtrl+N"))?)
        .item(&item("file.newWindow", tr(&text, "New Window"), Some("Shift+CmdOrCtrl+N"))?)
        .item(&item("file.open", tr(&text, "Open…"), Some("CmdOrCtrl+O"))?)
        .separator()
        .item(&item("file.save", tr(&text, "Save"), Some("CmdOrCtrl+S"))?)
        .item(&item("file.saveAs", tr(&text, "Save As…"), Some("Shift+CmdOrCtrl+S"))?)
        .separator()
        .item(&item("file.exportFolder", tr(&text, "Export as Website (folder)…"), None)?)
        .item(&item("file.exportSingle", tr(&text, "Export as Single HTML File…"), None)?)
        .item(&item("file.exportPdf", tr(&text, "Export as PDF / Print…"), Some("CmdOrCtrl+P"))?)
        .separator()
        .item(&item("file.close", tr(&text, "Close Presentation"), Some("CmdOrCtrl+W"))?)
        .build()?;
    // Native items, so that text fields get the usual editing commands.
    let edit = SubmenuBuilder::new(app, tr(&text, "Edit"))
        .undo_with_text(tr(&text, "Undo"))
        .redo_with_text(tr(&text, "Redo"))
        .separator()
        .cut_with_text(tr(&text, "Cut"))
        .copy_with_text(tr(&text, "Copy"))
        .paste_with_text(tr(&text, "Paste"))
        .select_all_with_text(tr(&text, "Select All"))
        .build()?;
    let view = SubmenuBuilder::new(app, tr(&text, "View"))
        .item(&item("view.zoomIn", tr(&text, "Zoom In"), Some("CmdOrCtrl+="))?)
        .item(&item("view.zoomOut", tr(&text, "Zoom Out"), Some("CmdOrCtrl+-"))?)
        .item(&item("view.zoomFit", tr(&text, "Zoom to Fit"), Some("CmdOrCtrl+0"))?)
        .item(&item("view.zoom100", tr(&text, "Actual Size"), Some("CmdOrCtrl+1"))?)
        .separator()
        .item(&item("view.inspector", tr(&text, "Show / Hide Inspector"), Some("Alt+CmdOrCtrl+I"))?)
        .item(&item("view.notes", tr(&text, "Show / Hide Speaker Notes"), Some("Alt+CmdOrCtrl+N"))?)
        .item(&item("view.rulers", tr(&text, "Show / Hide Rulers"), Some("CmdOrCtrl+R"))?)
        .item(&item("view.guides", tr(&text, "Show / Hide Guides"), Some("CmdOrCtrl+;"))?)
        .separator()
        .fullscreen_with_text(tr(&text, "Enter Full Screen"))
        .build()?;
    let play = SubmenuBuilder::new(app, tr(&text, "Play"))
        .item(&item("play.start", tr(&text, "Play from Beginning"), Some("F5"))?)
        .item(&item("play.current", tr(&text, "Play from Current Slide"), Some("Shift+F5"))?)
        .build()?;
    // ⌘M stays "New Slide", as everywhere else in Cadenza.
    let window: Submenu<Wry> = SubmenuBuilder::new(app, tr(&text, "Window"))
        .item(&item(MINIMIZE, tr(&text, "Minimize"), None)?)
        .separator()
        .bring_all_to_front_with_text(tr(&text, "Bring All to Front"))
        .build()?;
    let help = SubmenuBuilder::new(app, tr(&text, "Help"))
        .item(&item("app.shortcuts", tr(&text, "Keyboard Shortcuts"), Some("CmdOrCtrl+/"))?)
        .item(&item("app.palette", tr(&text, "Command Palette…"), Some("CmdOrCtrl+K"))?)
        .build()?;
    Menu::with_items(app, &[&cadenza, &file, &edit, &view, &play, &window, &help])
}

pub fn on_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        QUIT => quit(app),
        MINIMIZE => {
            if let Some(w) = app.webview_windows().into_values().find(|w| w.is_focused().unwrap_or(false)) {
                let _ = w.minimize();
            }
        }
        command => {
            if let Some(w) = target_editor(app) {
                let _ = app.emit_to(w.label(), "cz-command", command);
            }
        }
    }
}

/// The editor of the frontmost window (a show belongs to its editor).
fn target_editor(app: &AppHandle) -> Option<WebviewWindow> {
    let windows = app.webview_windows();
    let focused = windows.values().find(|w| w.is_focused().unwrap_or(false)).map(|w| owner_label(w.label()));
    focused
        .and_then(|label| windows.get(label))
        .or_else(|| windows.iter().find(|(label, _)| label.starts_with("editor-")).map(|(_, w)| w))
        .cloned()
}

/// Closes every editor window the way its close button does, so each one
/// asks about unsaved changes; Cadenza quits when the last one has closed.
fn quit(app: &AppHandle) {
    let editors: Vec<WebviewWindow> = app
        .webview_windows()
        .into_iter()
        .filter(|(label, _)| label.starts_with("editor-"))
        .map(|(_, w)| w)
        .collect();
    if editors.is_empty() {
        app.exit(0);
    }
    for w in editors {
        let _ = w.close();
    }
}
