//! System font discovery for the font picker.

use std::collections::BTreeSet;
use std::sync::OnceLock;

static FONTS: OnceLock<Vec<String>> = OnceLock::new();

/// Font families installed on the system, sorted and de-duplicated.
/// The first call scans the system (tens of milliseconds); later calls are
/// free. [`warm_up`] runs the scan on a background thread at startup.
pub fn families() -> &'static [String] {
    FONTS.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let mut set = BTreeSet::new();
        for face in db.faces() {
            if let Some((name, _)) = face.families.first() {
                let n = name.trim();
                // Skip hidden/system faces such as ".SF NS".
                if !n.is_empty() && !n.starts_with('.') {
                    set.insert(n.to_string());
                }
            }
        }
        let mut v: Vec<String> = set.into_iter().collect();
        v.sort_by_key(|s| s.to_lowercase());
        v
    })
}

pub fn warm_up() {
    std::thread::spawn(|| {
        let _ = families();
    });
}
