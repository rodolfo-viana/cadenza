//! API keys for online services (media providers).
//!
//! Keys are kept in the operating system's credential store (Secret Service
//! on Linux, Credential Manager on Windows, Keychain on macOS). When no store
//! is available (e.g. a minimal Linux session), they fall back to a private
//! file in the configuration folder. Keys are never written into
//! presentations and never sent to the UI.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SERVICE: &str = "app.cadenza.studio";

/// Services Cadenza knows how to use.
pub const NAMES: &[&str] = &["unsplash", "giphy", "youtube"];

/// Services of the AI assistant that Cadenza 0.1 had.
const RETIRED: &[&str] = &["anthropic", "openai", "compatible"];

pub struct Secrets {
    fallback: PathBuf,
    /// Independent of settings, which may be saved before cleanup succeeds.
    retired_marker: PathBuf,
    /// Isolated profiles (`CADENZA_DATA_DIR`) keep their keys to themselves
    /// instead of sharing the system keychain with the normal profile.
    keychain: bool,
    lock: Mutex<()>,
}

impl Secrets {
    pub fn new(config_dir: &Path, keychain: bool) -> Self {
        Secrets {
            fallback: config_dir.join("secrets.json"),
            retired_marker: config_dir.join("retired-ai-keys-v1"),
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
        self.read_fallback_checked().unwrap_or_default()
    }

    fn read_fallback_checked(&self) -> anyhow::Result<BTreeMap<String, String>> {
        match fs::read(&self.fallback) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn write_fallback(&self, map: &BTreeMap<String, String>) -> anyhow::Result<()> {
        if let Some(dir) = self.fallback.parent() {
            fs::create_dir_all(dir)?;
        }
        if map.is_empty() {
            return match fs::remove_file(&self.fallback) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.into()),
            };
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

    /// Deletes retired keys once. Failed cleanup leaves no completion marker
    /// and is retried on the next launch, even after old settings are gone.
    pub fn forget_retired(&self) -> anyhow::Result<()> {
        self.forget_retired_with(|name| keyring::Entry::new(SERVICE, name)?.delete_credential())
    }

    fn forget_retired_with(&self, mut delete_credential: impl FnMut(&str) -> keyring::Result<()>) -> anyhow::Result<()> {
        let _g = self.lock.lock().map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        match fs::read(&self.retired_marker) {
            Ok(done) if done == b"keychain\n" || (!self.keychain && done == b"file\n") => return Ok(()),
            Ok(_) => {} // An interrupted marker write must also be retried.
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }

        let mut map = self.read_fallback_checked()?;
        let before = map.len();
        for name in RETIRED {
            map.remove(*name);
        }
        if map.len() != before {
            self.write_fallback(&map)?;
        }

        if self.keychain {
            for name in RETIRED {
                match delete_credential(name) {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
        if let Some(dir) = self.retired_marker.parent() {
            fs::create_dir_all(dir)?;
        }
        // A file-only profile must never mark the system keychain as cleared.
        fs::write(&self.retired_marker, if self.keychain { "keychain\n" } else { "file\n" })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_keys_are_forgotten() {
        for keep_media_key in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join("secrets.json");
            let mut keys = BTreeMap::from([("anthropic", "a"), ("openai", "o"), ("compatible", "c")]);
            if keep_media_key {
                keys.insert("unsplash", "u");
            }
            fs::write(&file, serde_json::to_vec(&keys).unwrap()).unwrap();
            let secrets = Secrets::new(dir.path(), false);
            assert_eq!(secrets.get("anthropic"), None);
            secrets.forget_retired().unwrap();
            secrets.forget_retired().unwrap();
            assert_eq!(secrets.get("unsplash").as_deref(), keep_media_key.then_some("u"));
            if keep_media_key {
                let left: BTreeMap<String, String> = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
                assert_eq!(left.keys().collect::<Vec<_>>(), ["unsplash"]);
            } else {
                assert!(!file.exists());
            }
        }
    }

    #[test]
    fn failed_keychain_cleanup_retries_after_settings_are_saved() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        fs::write(&settings_path, r#"{"aiEnabled":true,"uiTheme":"dark"}"#).unwrap();
        let secrets = Secrets::new(dir.path(), true);
        let mut calls = Vec::new();
        let result = secrets.forget_retired_with(|name| {
            calls.push(name.to_string());
            if name == "openai" { Err(keyring::Error::NoDefaultStore) } else { Ok(()) }
        });
        assert!(result.is_err());
        assert!(!secrets.retired_marker.exists());
        assert_eq!(calls, ["anthropic", "openai"]);

        crate::settings::Settings::load(&settings_path).save(&settings_path).unwrap();
        assert!(!crate::settings::has_ai_settings(&settings_path));
        let restarted = Secrets::new(dir.path(), true);
        calls.clear();
        restarted
            .forget_retired_with(|name| {
                calls.push(name.to_string());
                // Retrying deletion of an already removed key is successful.
                Err(keyring::Error::NoEntry)
            })
            .unwrap();
        assert_eq!(calls, RETIRED);
        restarted
            .forget_retired_with(|_| panic!("completed cleanup must not access the keychain"))
            .unwrap();
    }

    #[test]
    fn unreadable_or_invalid_fallback_is_not_marked_complete() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Secrets::new(dir.path(), false);
        // A read error must not be mistaken for an absent file.
        fs::create_dir(&secrets.fallback).unwrap();
        assert!(secrets.forget_retired().is_err());
        fs::remove_dir(&secrets.fallback).unwrap();
        fs::write(&secrets.fallback, b"invalid json").unwrap();
        assert!(secrets.forget_retired().is_err());
        assert!(!secrets.retired_marker.exists());
        assert_eq!(fs::read(&secrets.fallback).unwrap(), b"invalid json");
        fs::write(&secrets.fallback, r#"{"openai":"o"}"#).unwrap();
        Secrets::new(dir.path(), false).forget_retired().unwrap();
        assert!(!secrets.fallback.exists());
    }

    #[test]
    fn file_only_cleanup_does_not_skip_keychain_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        Secrets::new(dir.path(), false).forget_retired().unwrap();
        let secrets = Secrets::new(dir.path(), true);
        let mut calls = Vec::new();
        secrets
            .forget_retired_with(|name| {
                calls.push(name.to_string());
                Ok(())
            })
            .unwrap();
        assert_eq!(calls, RETIRED);
    }

    #[cfg(unix)]
    #[test]
    fn failed_fallback_write_and_removal_retry_on_next_launch() {
        use std::os::unix::fs::PermissionsExt;

        for keep_media_key in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let secrets = Secrets::new(dir.path(), false);
            let contents = if keep_media_key {
                r#"{"openai":"o","unsplash":"u"}"#
            } else {
                r#"{"openai":"o"}"#
            };
            fs::write(&secrets.fallback, contents).unwrap();
            // Deny writing the file, or removing it when it would be empty.
            let (path, readonly, writable) = if keep_media_key {
                (secrets.fallback.as_path(), 0o400, 0o600)
            } else {
                (dir.path(), 0o500, 0o700)
            };
            fs::set_permissions(path, fs::Permissions::from_mode(readonly)).unwrap();
            let result = secrets.forget_retired();
            fs::set_permissions(path, fs::Permissions::from_mode(writable)).unwrap();
            // Root can bypass these permissions; the other failure tests still apply.
            if result.is_ok() {
                assert!(!secrets.read_fallback().contains_key("openai"));
                continue;
            }
            assert!(!secrets.retired_marker.exists());
            assert_eq!(fs::read_to_string(&secrets.fallback).unwrap(), contents);
            Secrets::new(dir.path(), false).forget_retired().unwrap();
            assert!(!secrets.read_fallback().contains_key("openai"));
            assert_eq!(secrets.get("unsplash").as_deref(), keep_media_key.then_some("u"));
        }
    }
}
