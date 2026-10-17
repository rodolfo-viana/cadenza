//! Updates: when Cadenza starts, it asks GitHub for the latest release and,
//! if the person agrees, downloads it and installs it over the running
//! version with the package type it was installed from (.deb, .rpm,
//! AppImage or a Windows installer), then restarts.
//!
//! Every release carries `latest.json`, written by the release workflow. It
//! names the installer of each package type and gives that installer's
//! signature. The updater plugin checks the signature against the public
//! key in `tauri.conf.json` before installing anything.
//!
//! Restarting closes every window. The presentations that were open reopen
//! afterwards; those with unsaved changes come back from recovery snapshots.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};
use tauri::utils::config::BundleType;
use tauri_plugin_updater::Update;

/// Where Cadenza can also be downloaded by hand.
pub const RELEASES_URL: &str = "https://github.com/rodolfo-viana/cadenza/releases";

const REOPEN_FILE: &str = "reopen.json";

/// The package type this copy was installed from. It is unknown for a build
/// run from the source tree, which never updates itself: a package would
/// replace the developer's own build.
pub fn package() -> Option<&'static str> {
    match tauri::utils::platform::bundle_type()? {
        BundleType::Deb => Some("deb"),
        BundleType::Rpm => Some("rpm"),
        BundleType::AppImage => Some("appimage"),
        BundleType::Msi => Some("msi"),
        BundleType::Nsis => Some("nsis"),
        _ => None,
    }
}

/// What the person is told about an update.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    /// The opening paragraph of the release notes.
    pub summary: String,
    /// The release page, with all the notes.
    pub url: String,
    /// The package type being updated ("deb", "nsis"…).
    pub package: &'static str,
}

impl UpdateInfo {
    pub fn of(update: &Update) -> Self {
        UpdateInfo {
            version: update.version.clone(),
            current: update.current_version.clone(),
            summary: summary(update.body.as_deref().unwrap_or("")),
            url: format!("{RELEASES_URL}/tag/v{}", update.version),
            package: package().unwrap_or(""),
        }
    }
}

/// The update found and its download, shared by every window.
#[derive(Default)]
pub struct Updates {
    /// The check at startup runs once per launch, for whichever window asks
    /// first.
    pub checked_at_start: AtomicBool,
    pub found: Mutex<Option<Update>>,
    /// The downloaded package, its signature verified, and its version.
    pub downloaded: Mutex<Option<(String, Vec<u8>)>>,
    /// Stops the download under way.
    pub cancel: Mutex<Option<Box<dyn Fn() + Send>>>,
    /// Set while installing: windows that close then keep their sessions
    /// for reopening.
    pub installing: AtomicBool,
}

impl Updates {
    /// Whether `version` is downloaded, ready to install.
    pub fn has_downloaded(&self, version: &str) -> bool {
        self.downloaded.lock().is_ok_and(|d| d.as_ref().is_some_and(|(v, _)| v == version))
    }
}

/// The opening paragraph of release notes as plain text (every CHANGELOG
/// section starts with one), or nothing when they start with a heading or a
/// list.
pub fn summary(notes: &str) -> String {
    let lines: Vec<&str> = notes
        .lines()
        .map(str::trim)
        .skip_while(|l| l.is_empty())
        .take_while(|l| !l.is_empty())
        .collect();
    let prose = |l: &str| !l.starts_with(['#', '|', '>']) && !["- ", "* ", "+ "].iter().any(|m| l.starts_with(m));
    match lines.first() {
        Some(first) if prose(first) => lines.join(" ").replace("**", "").replace('`', ""),
        _ => String::new(),
    }
}

/// A presentation to reopen after restarting into a new version.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Reopen {
    /// Saved and unchanged: opened from its file again.
    File { path: PathBuf },
    /// With unsaved changes: recovered from its session.
    Session { id: String },
}

pub fn write_reopen(config_dir: &Path, list: &[Reopen]) -> anyhow::Result<()> {
    fs::create_dir_all(config_dir)?;
    crate::util::write_atomic(&config_dir.join(REOPEN_FILE), &serde_json::to_vec(list)?, false)?;
    Ok(())
}

pub fn forget_reopen(config_dir: &Path) {
    let _ = fs::remove_file(config_dir.join(REOPEN_FILE));
}

/// The presentations to reopen, once: the list is removed as it is read.
pub fn take_reopen(config_dir: &Path) -> Vec<Reopen> {
    let path = config_dir.join(REOPEN_FILE);
    let list = fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let _ = fs::remove_file(path);
    list
}

/// Removes what earlier updates left in the temporary folder `temp`: on
/// Windows the updater can't delete the installer it starts, since Cadenza
/// has quit by the time the installer ends.
pub fn remove_old_installers(temp: &Path, app_name: &str) {
    let Ok(entries) = fs::read_dir(temp) else { return };
    let prefix = format!("{app_name}-");
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(&prefix) && name.contains("-updater-") && entry.path().is_dir() {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_is_the_opening_paragraph() {
        let notes = "Fixes for Windows: starting a show no longer\nfreezes Cadenza.\n\n### Fixes\n\n- One\n- Two\n\nInstallers are attached below.";
        assert_eq!(summary(notes), "Fixes for Windows: starting a show no longer freezes Cadenza.");
        assert_eq!(summary("\r\n  **New** `charts`.\r\n\r\nMore"), "New charts.");
        assert_eq!(summary("### Fixes\n\n- One"), "");
        assert_eq!(summary("- One\n- Two"), "");
        assert_eq!(summary("* One"), "");
        assert_eq!(summary(""), "");
    }

    #[test]
    fn the_reopen_list_is_read_once() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        let list = vec![
            Reopen::File {
                path: PathBuf::from("/talks/a.cadenza"),
            },
            Reopen::Session { id: "1234-abcd".into() },
        ];
        write_reopen(&config, &list).unwrap();
        assert_eq!(take_reopen(&config), list);
        assert!(take_reopen(&config).is_empty());

        write_reopen(&config, &list).unwrap();
        forget_reopen(&config);
        assert!(take_reopen(&config).is_empty());

        fs::write(config.join(REOPEN_FILE), b"not json").unwrap();
        assert!(take_reopen(&config).is_empty());
        assert!(!config.join(REOPEN_FILE).exists());
    }

    #[test]
    fn installers_of_earlier_updates_are_removed() {
        let temp = tempfile::tempdir().unwrap();
        let keep = ["Cadenza-notes", "Other-0.3.3-updater-x1"];
        for d in ["Cadenza-0.3.3-updater-kfnfsx", "Cadenza-0.4.0-updater-a1b2c3"].iter().chain(&keep) {
            fs::create_dir_all(temp.path().join(d)).unwrap();
            fs::write(temp.path().join(d).join("setup.exe"), b"MZ").unwrap();
        }
        fs::write(temp.path().join("Cadenza-0.3.3-updater-file"), b"x").unwrap();
        remove_old_installers(temp.path(), "Cadenza");
        let mut left: Vec<String> = fs::read_dir(temp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        left.sort();
        assert_eq!(left, ["Cadenza-0.3.3-updater-file", "Cadenza-notes", "Other-0.3.3-updater-x1"]);
    }
}
