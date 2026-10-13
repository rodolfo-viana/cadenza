//! API keys for online services (media providers, AI assistants).
//!
//! Keys are kept in the operating system's credential store (Secret Service
//! on Linux, Credential Manager on Windows, Keychain on macOS). When no store
//! is available (e.g. a minimal Linux session), they fall back to a private
//! file in the configuration folder. Keys are never written into
//! presentations and never sent to the UI.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SERVICE: &str = "app.cadenza.studio";

/// Services Cadenza knows how to use.
pub const NAMES: &[&str] = &["unsplash", "giphy", "youtube", "anthropic", "openai", "compatible"];

pub struct Secrets {
    fallback: PathBuf,
    /// Isolated profiles (`CADENZA_DATA_DIR`) keep their keys to themselves
    /// instead of sharing the system keychain with the normal profile.
    keychain: bool,
    lock: Mutex<()>,
}

impl Secrets {
    pub fn new(config_dir: &Path, keychain: bool) -> Self {
        Secrets {
            fallback: config_dir.join("secrets.json"),
            keychain,
            lock: Mutex::new(()),
        }
    }

    fn entry(&self, name: &str) -> Option<keyring::Entry> {
        if self.keychain { keyring::Entry::new(SERVICE, name).ok() } else { None }
    }

    fn valid(name: &str) -> anyhow::Result<()> {
        if NAMES.contains(&name) {
            Ok(())
        } else {
            anyhow::bail!("unknown service {name}")
        }
    }

    fn read_fallback(&self) -> BTreeMap<String, String> {
        fs::read(&self.fallback).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn write_fallback(&self, map: &BTreeMap<String, String>) -> anyhow::Result<()> {
        if let Some(dir) = self.fallback.parent() {
            fs::create_dir_all(dir)?;
        }
        if map.is_empty() {
            let _ = fs::remove_file(&self.fallback);
            return Ok(());
        }
        fs::write(&self.fallback, serde_json::to_vec_pretty(map)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&self.fallback, fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<String> {
        Self::valid(name).ok()?;
        let _g = self.lock.lock().ok()?;
        if let Some(entry) = self.entry(name) {
            if let Ok(v) = entry.get_password() {
                return Some(v);
            }
        }
        self.read_fallback().get(name).cloned()
    }

    /// Stores a key; returns where it went ("keychain" or "file").
    pub fn set(&self, name: &str, value: &str) -> anyhow::Result<&'static str> {
        Self::valid(name)?;
        let value = value.trim();
        if value.is_empty() {
            self.delete(name)?;
            return Ok("none");
        }
        let _g = self.lock.lock().map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        let stored = self.entry(name).is_some_and(|e| e.set_password(value).is_ok());
        let mut map = self.read_fallback();
        if stored {
            if map.remove(name).is_some() {
                self.write_fallback(&map)?;
            }
            Ok("keychain")
        } else {
            map.insert(name.to_string(), value.to_string());
            self.write_fallback(&map)?;
            Ok("file")
        }
    }

    pub fn delete(&self, name: &str) -> anyhow::Result<()> {
        Self::valid(name)?;
        let _g = self.lock.lock().map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        if let Some(e) = self.entry(name) {
            let _ = e.delete_credential();
        }
        let mut map = self.read_fallback();
        if map.remove(name).is_some() {
            self.write_fallback(&map)?;
        }
        Ok(())
    }

    /// Which services have a key configured.
    pub fn status(&self) -> BTreeMap<&'static str, bool> {
        NAMES.iter().map(|n| (*n, self.get(n).is_some_and(|v| !v.is_empty()))).collect()
    }
}
