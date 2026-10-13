//! Application preferences and the recent-files list.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const MAX_RECENT: usize = 12;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// "system", "dark" or "light".
    pub ui_theme: String,
    /// Interface language code (see ui/js/i18n.js); English by default.
    pub language: String,
    /// Theme preselected for new presentations.
    pub default_theme: String,
    /// "16:9", "16:10" or "4:3".
    pub default_aspect: String,
    pub recent: Vec<String>,
    pub snap: bool,
    pub show_grid: bool,
    pub grid_size: u32,
    /// Open the presenter display when playing.
    pub presenter_view: bool,
    /// Show the inspector panel.
    pub show_inspector: bool,
    /// Show the speaker-notes panel.
    pub show_notes: bool,
    /// Rulers around the canvas.
    pub show_rulers: bool,
    /// Show (and snap to) the document's guides.
    pub show_guides: bool,
    /// The AI assistant is opt-in: nothing is sent anywhere while off.
    pub ai_enabled: bool,
    /// "anthropic", "openai" or "compatible".
    pub ai_provider: String,
    /// Chosen model per provider.
    pub ai_models: BTreeMap<String, String>,
    /// Address of an OpenAI-compatible server (e.g. Ollama).
    pub ai_base_url: String,
    /// Find stock photos for image slides the assistant creates.
    pub ai_images: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            ui_theme: "system".into(),
            language: "en".into(),
            default_theme: crate::themes::DEFAULT_THEME.into(),
            default_aspect: "16:9".into(),
            recent: Vec::new(),
            snap: true,
            show_grid: false,
            grid_size: 40,
            presenter_view: true,
            show_inspector: true,
            show_notes: true,
            show_rulers: true,
            show_guides: true,
            ai_enabled: false,
            ai_provider: "anthropic".into(),
            ai_models: BTreeMap::from([("anthropic".to_string(), crate::ai::DEFAULT_ANTHROPIC_MODEL.to_string())]),
            ai_base_url: crate::ai::DEFAULT_COMPATIBLE_URL.into(),
            ai_images: true,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn add_recent(&mut self, file: &Path) {
        let s = file.to_string_lossy().to_string();
        self.recent.retain(|r| r != &s);
        self.recent.insert(0, s);
        self.recent.truncate(MAX_RECENT);
    }

    pub fn remove_recent(&mut self, file: &str) {
        self.recent.retain(|r| r != file);
    }

    /// Recent files that still exist.
    pub fn existing_recent(&self) -> Vec<RecentFile> {
        self.recent
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .map(|p| RecentFile {
                name: p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
                folder: p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default(),
                modified: fs::metadata(&p)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
                path: p.to_string_lossy().to_string(),
            })
            .collect()
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RecentFile {
    pub path: String,
    pub name: String,
    pub folder: String,
    pub modified: u64,
}

/// Slide size for an aspect ratio name.
pub fn aspect_size(aspect: &str) -> (u32, u32) {
    match aspect {
        "4:3" => (1024, 768),
        "16:10" => (1280, 800),
        "1:1" => (1080, 1080),
        _ => (1280, 720),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_list_is_deduplicated_and_bounded() {
        let mut s = Settings::default();
        for i in 0..20 {
            s.add_recent(Path::new(&format!("/tmp/{i}.cadenza")));
        }
        s.add_recent(Path::new("/tmp/5.cadenza"));
        assert_eq!(s.recent.len(), MAX_RECENT);
        assert_eq!(s.recent[0], "/tmp/5.cadenza");
        assert_eq!(s.recent.iter().filter(|r| *r == "/tmp/5.cadenza").count(), 1);
    }

    #[test]
    fn settings_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("cfg/settings.json");
        let mut s = Settings::default();
        s.ui_theme = "light".into();
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p), s);
        assert_eq!(Settings::load(&dir.path().join("missing.json")), Settings::default());
    }
}
