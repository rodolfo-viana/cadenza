//! Manual artifact probe for presentations produced by the native UI.
//! Does not add a production command or require an active GUI session.
//! CADENZA_PPTX_PROBE=/absolute/export.pptx CADENZA_PPTX_PROBE_DIR=/absolute/report \
//! cargo test --test pptx_probe inspect_native_export_artifact -- --ignored --nocapture
//! CADENZA_NATIVE_PROBE=/absolute/saved.cadenza \
//! cargo test --test pptx_probe reopen_native_saved_artifact -- --ignored --nocapture

use cadenza_lib::{document::Document, model::Content, pptx_import};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[test]
#[ignore = "Requires CADENZA_NATIVE_PROBE pointing to a presentation saved through native UI"]
fn reopen_native_saved_artifact() {
    let input = PathBuf::from(std::env::var_os("CADENZA_NATIVE_PROBE").expect("Set CADENZA_NATIVE_PROBE to a saved .cadenza"));
    let temporary = tempfile::tempdir().unwrap();
    let doc = Document::open(temporary.path(), &input).expect("The native UI's saved presentation must reopen");
    assert!(!doc.is_dirty());
    assert!(!doc.deck.slides.is_empty());
    for name in doc.deck.retained_assets() {
        assert!(doc.assets_dir().join(name).is_file());
    }
    println!(
        "Reopened {}: {} slides, {} conversion issues",
        input.display(),
        doc.deck.slides.len(),
        doc.deck.import_issues.len()
    );
    doc.dispose();
}

#[test]
#[ignore = "Requires a native UI export artifact and explicit report directory"]
fn inspect_native_export_artifact() {
    let input = PathBuf::from(std::env::var_os("CADENZA_PPTX_PROBE").expect("Set CADENZA_PPTX_PROBE to the exported .pptx"));
    let output = PathBuf::from(std::env::var_os("CADENZA_PPTX_PROBE_DIR").expect("Set CADENZA_PPTX_PROBE_DIR to a report directory"));
    let temporary = tempfile::tempdir().unwrap();
    let staging = temporary.path().join("staging");
    let imported = pptx_import::parse(&input, &staging).expect("Native export must be readable by the ordinary OOXML importer");
    let mut counts = BTreeMap::<&str, usize>::new();
    for element in imported.deck.slides.iter().flat_map(|slide| &slide.elements) {
        let kind = match element.content {
            Content::Text(_) => "text",
            Content::Shape(_) => "shape",
            Content::Image(_) => "image",
            Content::Table(_) => "table",
            Content::Chart(_) => "chart",
            _ => "other",
        };
        *counts.entry(kind).or_default() += 1;
    }
    let mut doc = Document::from_import(temporary.path(), imported.deck).unwrap();
    for name in doc.deck.retained_assets() {
        std::fs::rename(staging.join(&name), doc.assets_dir().join(&name)).unwrap();
    }
    std::fs::create_dir_all(&output).unwrap();
    let report = serde_json::json!({
        "input": input.file_name().unwrap().to_string_lossy(), "width": doc.deck.width, "height": doc.deck.height,
        "slides": doc.deck.slides.len(), "objects": counts, "assets": doc.deck.retained_assets().len(), "issues": imported.issues,
    });
    std::fs::write(output.join("report.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    let saved = output.join("reimported.cadenza");
    doc.save(&saved).expect("Reimported artifact must save as a native presentation");
    let reopened = Document::open(temporary.path(), &saved).expect("Saved artifact must reopen");
    assert_eq!(reopened.deck.slides.len(), doc.deck.slides.len());
    assert_eq!(reopened.deck.import_issues, doc.deck.import_issues);
    assert_eq!(reopened.deck.retained_assets(), doc.deck.retained_assets());
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    reopened.dispose();
    doc.dispose();
}
