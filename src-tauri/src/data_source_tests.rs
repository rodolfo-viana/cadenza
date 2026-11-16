//! Source provenance through actual document operations, packaging and clipboard.
use super::*;
use crate::clipboard::{Clipboard, Selection, Store};
use crate::document::Document;
use serde_json::json;

fn input() -> DataSource {
    DataSource {
        text: "\u{feff}Label;Value\r\n{A};1,5\r\n{B};2,5\r\n".into(),
        name: "original.csv".into(),
        delimiter: ";".into(),
        header: true,
        roles: vec!["label".into(), "value".into()],
        ..DataSource::default()
    }
}

fn document(root: &std::path::Path, kind: &str, source: bool) -> Document {
    let mut el = layouts::new_element(kind, "", 1280, 720);
    if source {
        el.data_source = Some(input());
    }
    Document::new(
        root,
        Deck {
            slides: vec![Slide {
                elements: vec![el],
                ..Slide::default()
            }],
            ..Deck::default()
        },
    )
    .unwrap()
}

fn edit(doc: &mut Document, patch: Value) {
    doc.apply(
        Op::UpdateElements {
            slide: doc.deck.slides[0].id.clone(),
            updates: vec![ElementPatch {
                id: doc.deck.slides[0].elements[0].id.clone(),
                patch,
            }],
        },
        None,
    )
    .unwrap();
}

fn element(doc: &Document) -> &Element {
    &doc.deck.slides[0].elements[0]
}

#[test]
fn data_source_survives_current_edits_undo_redo_and_save_reopen() {
    let root = tempfile::tempdir().unwrap();
    for kind in ["chart", "table"] {
        let mut doc = document(root.path(), kind, true);
        let before = element(&doc).clone();
        let changed = if kind == "chart" {
            json!({"content": {"labels": ["Changed"]}})
        } else {
            json!({"content": {"rows": [["Changed"]]}})
        };
        edit(&mut doc, changed);
        let edited = element(&doc).data_source.as_ref().unwrap();
        assert_eq!(edited.text, input().text);
        assert!(edited.edited);
        let after = element(&doc).clone();
        doc.undo().unwrap();
        assert_eq!(element(&doc), &before);
        doc.redo().unwrap();
        assert_eq!(element(&doc), &after);
        let path = root.path().join(format!("{kind}.cadenza"));
        doc.save(&path).unwrap();
        let reopened = Document::open(root.path(), &path).unwrap();
        assert_eq!(element(&reopened), &after, "{kind} persisted exact text and parse choices");
    }
}

#[test]
fn data_source_explicit_replacement_is_one_atomic_undo_step() {
    let root = tempfile::tempdir().unwrap();
    let mut doc = document(root.path(), "chart", true);
    let before = element(&doc).clone();
    let mut source = input();
    source.text = "Label,Value\nNew,42".into();
    source.delimiter = ",".into();
    source.name = "C:\\Users\\private\\new.csv".into();
    edit(
        &mut doc,
        json!({"content": {"labels": ["New"], "series": [{"name": "Value", "values": [42]}]}, "dataSource": source}),
    );
    assert_eq!(element(&doc).data_source.as_ref().unwrap().name, "new.csv");
    assert!(!element(&doc).data_source.as_ref().unwrap().edited);
    let after = element(&doc).clone();
    doc.undo().unwrap();
    assert_eq!(element(&doc), &before);
    assert!(doc.undo().is_none(), "source and content are one history entry");
    doc.redo().unwrap();
    assert_eq!(element(&doc), &after);
}

#[test]
fn data_source_format_changes_do_not_mark_data_as_edited() {
    let root = tempfile::tempdir().unwrap();
    let mut doc = document(root.path(), "table", true);
    let Content::Table(table) = &element(&doc).content else { panic!() };
    let rows: Vec<Vec<String>> = table.rows.iter().map(|r| r.iter().map(|v| format!("<b>{v}</b>")).collect()).collect();
    edit(&mut doc, json!({"content": {"rows": rows, "fontSize": 35, "headerRow": false}, "x": 14}));
    assert_eq!(element(&doc).data_source, Some(input()));
    let mut chart = document(root.path(), "chart", true);
    let Content::Chart(content) = &element(&chart).content else { panic!() };
    let mut series = content.series.clone();
    series[0].color = "#ff0000".into();
    series[0].marker = "square".into();
    edit(&mut chart, json!({"content": {"series": series, "axisMax": 1000, "chart": "line"}}));
    assert_eq!(element(&chart).data_source, Some(input()));
}

#[test]
fn data_source_search_replace_marks_current_table_only() {
    let root = tempfile::tempdir().unwrap();
    let mut doc = document(root.path(), "table", true);
    let Content::Table(content) = &element(&doc).content else { panic!() };
    let op = serde_json::from_value(json!({
        "op": "replaceText", "all": true, "changes": [{
            "slide": doc.deck.slides[0].id, "element": element(&doc).id,
            "field": "tableCell", "row": 0, "column": 0, "before": content.rows[0][0], "after": "Replaced"
        }]
    }))
    .unwrap();
    doc.apply(op, None).unwrap();
    assert!(element(&doc).data_source.as_ref().unwrap().edited);
    assert_eq!(element(&doc).data_source.as_ref().unwrap().text, input().text);
    doc.undo().unwrap();
    assert_eq!(element(&doc).data_source, Some(input()));
}

#[test]
fn data_source_survives_duplication_and_cross_document_clipboard() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(root.path().join("clipboard"));
    for kind in ["chart", "table"] {
        let mut source = document(root.path(), kind, true);
        let original = element(&source).clone();
        source
            .apply(
                Op::InsertElements {
                    slide: source.deck.slides[0].id.clone(),
                    elements: vec![original.clone()],
                    offset: 20.0,
                    duplicate: true,
                },
                None,
            )
            .unwrap();
        let duplicate = &source.deck.slides[0].elements[1];
        assert_ne!(original.id, duplicate.id);
        assert_eq!(duplicate.data_source, original.data_source);
        let clip = Clipboard::copy(
            &source,
            Selection::Elements {
                elements: vec![original.clone()],
            },
            &store,
            |_| Ok(()),
        )
        .unwrap();
        let mut dest = document(root.path(), kind, false);
        let slide = dest.deck.slides[0].id.clone();
        clip.paste(&mut dest, Some(slide), 0.0, &store).unwrap();
        assert_eq!(dest.deck.slides[0].elements[1].data_source, original.data_source);
    }
}

#[test]
fn data_source_legacy_objects_remain_without_fabricated_provenance_and_noops_are_clean() {
    let root = tempfile::tempdir().unwrap();
    for kind in ["chart", "table"] {
        let mut doc = document(root.path(), kind, false);
        let content = serde_json::to_value(&element(&doc).content).unwrap();
        edit(&mut doc, json!({"content": content}));
        assert!(doc.history.undo_label().is_none());
        assert!(!doc.is_dirty());
        edit(&mut doc, json!({"x": 33}));
        let path = root.path().join(format!("legacy-{kind}.cadenza"));
        doc.save(&path).unwrap();
        let reopened = Document::open(root.path(), &path).unwrap();
        assert_eq!(element(&reopened).data_source, None);
        assert!(serde_json::to_value(element(&reopened)).unwrap().get("dataSource").is_none());
    }
}

#[test]
fn data_source_invalid_input_rolls_back_content_together() {
    let root = tempfile::tempdir().unwrap();
    let mut doc = document(root.path(), "chart", true);
    let before = doc.deck.clone();
    let result = doc.apply(
        Op::UpdateElements {
            slide: doc.deck.slides[0].id.clone(),
            updates: vec![ElementPatch {
                id: element(&doc).id.clone(),
                patch: json!({"content": {"labels": ["Should not apply"]}, "dataSource": {"dialect": "unknown"}}),
            }],
        },
        None,
    );
    assert!(result.is_err());
    assert_eq!(doc.deck, before);
}
