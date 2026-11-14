//! Regression tests exercise the same operations, document history, packaging
//! and shared renderer as the desktop editor.
use crate::{
    document::Document,
    groups,
    model::*,
    ops,
    render::{self, Ctx, Mode},
};
use serde_json::{Value, json};

fn deck() -> Deck {
    let mut d = Deck::default();
    let mut s = Slide::default();
    for (i, angle) in [25.0, 0.0, -40.0].into_iter().enumerate() {
        let mut e = Element {
            x: 100.0 + i as f64 * 220.0,
            y: 120.0 + i as f64 * 35.0,
            rotation: angle,
            ..Element::default()
        };
        e.content.text_mut().unwrap().html = format!("<p><b>Example {i}</b> <a href=\"https://example.org\">link</a></p>");
        e.content.text_mut().unwrap().style.auto_height = false;
        e.style.border_width = 3.0;
        e.style.padding = 8.0;
        e.build.effect = "fade-in".into();
        e.build.order = i as u32 + 1;
        e.build.path = vec![[0.0, 0.0], [30.0, 40.0]];
        s.elements.push(e);
    }
    d.slides.push(s);
    d
}

fn apply(d: &mut Deck, value: Value) -> anyhow::Result<ops::OpOutcome> {
    ops::apply(d, serde_json::from_value(value)?)
}
fn render(d: &Deck, e: &Element, mode: Mode) -> String {
    render::element(
        d,
        e,
        &Ctx {
            mode,
            asset_url: &str::to_string,
            youtube_host: None,
        },
    )
}
fn grouped(d: &mut Deck) -> groups::GroupFrame {
    let s = &d.slides[0];
    apply(d, json!({"op":"groupElements", "slide":s.id, "ids":[s.elements[0].id, s.elements[2].id]})).unwrap();
    d.slides[0].elements[0].group.clone().unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7, "{a} != {b}");
}

#[test]
fn group_and_ungroup_preserve_interleaved_render_order_and_every_member_property() {
    let mut d = deck();
    let before = d.clone();
    grouped(&mut d);
    for (a, b) in before.slides[0].elements.iter().zip(&d.slides[0].elements) {
        assert_eq!(render(&before, a, Mode::Export), render(&d, b, Mode::Export));
        let mut stripped = b.clone();
        stripped.group = None;
        assert_eq!(*a, stripped);
    }
    let s = &d.slides[0];
    apply(&mut d.clone(), json!({"op":"ungroupElements", "slide":s.id, "ids":[s.elements[0].id]})).unwrap();
    groups::ungroup(&mut d.slides[0], &[before.slides[0].elements[0].id.clone()]);
    assert_eq!(before, d);
}

#[test]
fn group_rotation_scales_centers_content_and_paths_and_is_reversible() {
    let mut d = deck();
    let g = grouped(&mut d);
    let before = d.clone();
    let after = groups::GroupFrame {
        x: g.x + 85.0,
        y: g.y - 20.0,
        w: g.w * 1.75,
        h: g.h * 1.75,
        rotation: 87.0,
        ..g.clone()
    };
    groups::transform(&mut d.slides[0], &g, &after).unwrap();
    let e = &d.slides[0].elements[0];
    near(e.content_scale, 1.75);
    near(e.rotation, 112.0);
    assert_eq!(e.style, before.slides[0].elements[0].style);
    assert_eq!(e.content, before.slides[0].elements[0].content);
    near(e.build.path[1][0].hypot(e.build.path[1][1]), 87.5);
    assert!(render(&d, e, Mode::Export).contains("zoom:1.75"));
    groups::transform(&mut d.slides[0], &after, &g).unwrap();
    for (a, b) in before.slides[0].elements.iter().zip(&d.slides[0].elements) {
        near(a.x, b.x);
        near(a.y, b.y);
        near(a.w, b.w);
        near(a.h, b.h);
        near(a.rotation, b.rotation);
        near(a.content_scale, b.content_scale);
    }
}

#[test]
fn group_translation_keeps_the_saved_oriented_pivot() {
    let mut d = deck();
    let g = grouped(&mut d);
    let rotated = groups::GroupFrame { rotation: 67.0, ..g.clone() };
    groups::transform(&mut d.slides[0], &g, &rotated).unwrap();
    let s = &d.slides[0];
    let updates: Vec<_> = s
        .elements
        .iter()
        .filter(|e| e.group.is_some())
        .map(|e| json!({"id":e.id,"patch":{"x":e.x+13.0,"y":e.y-7.0}}))
        .collect();
    let operation = json!({"op":"updateElements","slide":s.id,"updates":updates});
    apply(&mut d, operation).unwrap();
    let actual = d.slides[0].elements[0].group.as_ref().unwrap();
    near(actual.x, rotated.x + 13.0);
    near(actual.y, rotated.y - 7.0);
    near(actual.rotation, 67.0);
}

#[test]
fn groups_reject_nonuniform_stale_and_locked_transforms_atomically() {
    let mut d = deck();
    let g = grouped(&mut d);
    for after in [
        groups::GroupFrame { w: g.w * 2.0, ..g.clone() },
        groups::GroupFrame {
            id: "missing".into(),
            ..g.clone()
        },
        groups::GroupFrame { w: 0.0, h: 0.0, ..g.clone() },
    ] {
        let before = d.clone();
        let id = d.slides[0].id.clone();
        assert!(apply(&mut d, json!({"op":"transformGroup","slide":id,"before":g,"after":after})).is_err());
        assert_eq!(d, before);
    }
    d.slides[0].elements[2].locked = true;
    let before = d.clone();
    let s = &d.slides[0];
    assert!(
        apply(
            &mut d.clone(),
            json!({"op":"updateElements","slide":s.id,"updates":[{"id":s.elements[0].id,"patch":{"x":5}}]})
        )
        .is_err()
    );
    assert!(groups::transform(&mut d.slides[0], &g, &g).is_err());
    assert_eq!(d, before);
}

#[test]
fn duplicated_groups_have_fresh_ids_and_preserve_magic_move_links_across_slides() {
    let mut d = deck();
    let group = grouped(&mut d);
    let source = d.slides[0].clone();
    let duplicated = source.duplicate();
    assert_ne!(duplicated.elements[0].group.as_ref().unwrap().id, group.id);
    assert_eq!(duplicated.elements[0].link, source.elements[0].link);
    let members: Vec<_> = source.elements.iter().filter(|e| e.group.is_some()).cloned().collect();
    apply(
        &mut d,
        json!({"op":"insertElements","slide":source.id,"elements":members,"offset":24,"duplicate":true}),
    )
    .unwrap();
    let copies = &d.slides[0].elements[3..];
    assert_ne!(copies[0].link, members[0].link);
    assert_ne!(copies[0].group.as_ref().unwrap().id, group.id);
    assert_eq!(copies[0].group, copies[1].group);
    near(copies[0].group.as_ref().unwrap().x, group.x + 24.0);
    let other = Slide::default();
    let id = other.id.clone();
    d.slides.push(other);
    apply(&mut d, json!({"op":"insertElements","slide":id,"elements":members})).unwrap();
    assert_eq!(d.slides[1].elements[0].link, members[0].link);
    assert_ne!(d.slides[1].elements[0].group.as_ref().unwrap().id, group.id);
}

#[test]
fn partial_delete_expands_groups_and_invalid_membership_dissolves_without_data_loss() {
    let mut d = deck();
    grouped(&mut d);
    let s = &d.slides[0];
    apply(&mut d.clone(), json!({"op":"deleteElements","slide":s.id,"ids":[s.elements[0].id]})).unwrap();
    let id = s.elements[0].id.clone();
    let sid = s.id.clone();
    apply(&mut d, json!({"op":"deleteElements","slide":sid,"ids":[id]})).unwrap();
    assert_eq!(d.slides[0].elements.len(), 1);
    let mut d = deck();
    grouped(&mut d);
    d.slides[0].elements[2].group.as_mut().unwrap().x += 1.0;
    d.normalize();
    assert_eq!(d.slides[0].elements.len(), 3);
    assert!(d.slides[0].elements.iter().all(|e| e.group.is_none()));
}

fn image_deck() -> Deck {
    let mut d = deck();
    d.slides[0].elements[0].content = Content::Image(ImageContent {
        src: "https://example.org/image.png".into(),
        natural_width: 800,
        natural_height: 400,
        ..ImageContent::default()
    });
    d
}
fn crop_op(d: &Deck, crop: Value) -> Value {
    let s = &d.slides[0];
    json!({"op":"cropImage","slide":s.id,"id":s.elements[0].id,"before":s.elements[0].content,"crop":crop,"naturalWidth":800,"naturalHeight":400})
}

#[test]
fn crop_keeps_original_source_frame_filters_and_reset_restores_legacy_rendering() {
    let mut d = image_deck();
    let before = d.clone();
    let legacy = render(&d, &d.slides[0].elements[0], Mode::Export);
    let op = crop_op(&d, json!({"x":0.25,"y":0.0,"w":0.5,"h":1.0}));
    apply(&mut d, op).unwrap();
    let e = &d.slides[0].elements[0];
    assert_eq!(e.x, before.slides[0].elements[0].x);
    assert_eq!(e.rotation, 25.0);
    assert!(render(&d, e, Mode::Export).contains("cz-crop-viewport"));
    for mode in [Mode::Editor, Mode::Show, Mode::Export] {
        let html = render(&d, e, mode);
        assert!(html.contains("width:200%"));
        assert!(html.contains("left:-50%"));
    }
    let op = crop_op(&d, Value::Null);
    apply(&mut d, op).unwrap();
    assert_eq!(render(&d, &d.slides[0].elements[0], Mode::Export), legacy);
    let op = crop_op(&d, json!({"x":0,"y":0,"w":1,"h":1}));
    apply(&mut d, op).unwrap();
    assert_eq!(render(&d, &d.slides[0].elements[0], Mode::Export), legacy);
}

#[test]
fn invalid_or_stale_crops_do_not_mutate_the_document_and_replacing_media_clears_crop() {
    let mut d = image_deck();
    for c in [
        json!({"x":0.8,"y":0,"w":0.4,"h":1}),
        json!({"x":0,"y":0,"w":0,"h":1}),
        json!({"x":-0.1,"y":0,"w":1,"h":1}),
    ] {
        let before = d.clone();
        let op = crop_op(&d, c);
        assert!(apply(&mut d, op).is_err());
        assert_eq!(before, d);
    }
    let mut stale = crop_op(&d, Value::Null);
    stale["before"]["src"] = json!("changed.png");
    assert!(apply(&mut d, stale).is_err());
    let op = crop_op(&d, json!({"x":0.2,"y":0,"w":0.8,"h":1}));
    apply(&mut d, op).unwrap();
    let s = &d.slides[0];
    apply(
        &mut d.clone(),
        json!({"op":"updateElements","slide":s.id,"updates":[{"id":s.elements[0].id,"patch":{"content":{"src":"new.png"}}}]}),
    )
    .unwrap();
    let (sid, id) = (s.id.clone(), s.elements[0].id.clone());
    apply(
        &mut d,
        json!({"op":"updateElements","slide":sid,"updates":[{"id":id,"patch":{"content":{"src":"new.png"}}}]}),
    )
    .unwrap();
    let Content::Image(i) = &d.slides[0].elements[0].content else { panic!() };
    assert!(i.crop.is_none());
    assert_eq!((i.natural_width, i.natural_height), (0, 0));
}

#[test]
fn painter_rejects_geometry_sources_links_builds_and_auto_height() {
    let d = deck();
    let e = &d.slides[0].elements[0];
    for patch in [
        json!({"x":5}),
        json!({"opacity":0.5}),
        json!({"build":{"effect":"fade-in"}}),
        json!({"action":{"url":"https://example.org"}}),
        json!({"content":{"style":{"autoHeight":false}}}),
        json!({"content":{"src":"other.png"}}),
    ] {
        assert!(crate::formatting::validate(e, &patch).is_err(), "{patch}");
    }
    assert!(crate::formatting::validate(e, &json!({"style":{"fill":"red"},"content":{"style":{"fontSize":48},"html":"<b>Same</b>"}})).is_ok());
}

#[test]
fn group_crop_and_painter_each_have_one_undo_step_and_survive_save_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = Document::new(dir.path(), image_deck()).unwrap();
    let before = doc.deck.clone();
    let s = &doc.deck.slides[0];
    doc.apply(
        serde_json::from_value(json!({"op":"groupElements","slide":s.id,"ids":[s.elements[0].id,s.elements[2].id]})).unwrap(),
        None,
    )
    .unwrap();
    let grouped = doc.deck.clone();
    doc.undo().unwrap();
    assert_eq!(doc.deck, before);
    doc.redo().unwrap();
    assert_eq!(doc.deck, grouped);
    let op = crop_op(&doc.deck, json!({"x":0.25,"y":0.25,"w":0.5,"h":0.5}));
    doc.apply(serde_json::from_value(op).unwrap(), None).unwrap();
    let cropped = doc.deck.clone();
    doc.undo().unwrap();
    assert_eq!(doc.deck, grouped);
    doc.redo().unwrap();
    assert_eq!(doc.deck, cropped);
    let s = &doc.deck.slides[0];
    doc.apply(
        serde_json::from_value(json!({"op":"paintFormat","slide":s.id,"updates":[{"id":s.elements[1].id,"patch":{"style":{"fill":"#ff0000"}}}]})).unwrap(),
        None,
    )
    .unwrap();
    let painted = doc.deck.clone();
    doc.undo().unwrap();
    assert_eq!(doc.deck, cropped);
    doc.redo().unwrap();
    assert_eq!(doc.deck, painted);
    let path = dir.path().join("editing.cadenza");
    doc.save(&path).unwrap();
    let reopened = Document::open(dir.path(), &path).unwrap();
    assert_eq!(doc.deck, reopened.deck);
}

#[test]
fn locked_group_alignment_and_oversized_transform_preserve_geometry() {
    let mut d = deck();
    let g = grouped(&mut d);
    d.slides[0].elements[2].locked = true;
    let before = d.clone();
    let op = json!({"op":"alignElements","slide":d.slides[0].id,"ids":[d.slides[0].elements[0].id],"mode":"left","toSlide":true});
    apply(&mut d, op).unwrap();
    assert_eq!(d, before);
    d.slides[0].elements[2].locked = false;
    let before = d.clone();
    let after = groups::GroupFrame {
        w: g.w * 150.0,
        h: g.h * 150.0,
        ..g.clone()
    };
    let op = json!({"op":"transformGroup","slide":d.slides[0].id,"before":g,"after":after});
    assert!(apply(&mut d, op).is_err());
    assert_eq!(d, before);
}

#[test]
fn cut_does_not_expand_membership_after_the_snapshot_was_copied() {
    let mut d = deck();
    grouped(&mut d);
    let copied = vec![d.slides[0].elements[0].id.clone()];
    let op = json!({"op":"cutElements","slide":d.slides[0].id,"ids":copied});
    apply(&mut d, op).unwrap();
    assert_eq!(d.slides[0].elements.len(), 2);
    assert!(d.slides[0].elements.iter().all(|e| e.group.is_none()));
}

#[test]
fn small_visual_scales_are_rendered_without_rounding_to_zero() {
    let d = deck();
    let mut e = d.slides[0].elements[0].clone();
    for scale in [0.004, 0.125, 1.23456789] {
        e.content_scale = scale;
        let html = render(&d, &e, Mode::Export);
        assert!(html.contains(&format!("zoom:{scale};")), "{html}");
    }
}

#[test]
fn cropped_scaled_groups_keep_assets_in_both_real_export_formats() {
    let dir = tempfile::tempdir().unwrap();
    let mut d = image_deck();
    let Content::Image(image) = &mut d.slides[0].elements[0].content else {
        panic!()
    };
    image.src = "quadrants.svg".into();
    image.flip_x = true;
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="400"><path fill="#e25b45" d="M0 0h400v200H0z"/><path fill="#38ad72" d="M400 0h400v200H400z"/><path fill="#4169d1" d="M0 200h400v200H0z"/><path fill="#e9be45" d="M400 200h400v200H400z"/><g font-family="sans-serif" font-size="72" fill="white" text-anchor="middle"><text x="200" y="120">A</text><text x="600" y="120">B</text><text x="200" y="320">C</text><text x="600" y="320">D</text></g></svg>"##;
    let op = crop_op(&d, json!({"x":0.5,"y":0.5,"w":0.5,"h":0.5}));
    apply(&mut d, op).unwrap();
    let g = grouped(&mut d);
    let after = groups::GroupFrame {
        w: g.w * 1.25,
        h: g.h * 1.25,
        rotation: 33.0,
        ..g.clone()
    };
    groups::transform(&mut d.slides[0], &g, &after).unwrap();
    let mut doc = Document::new(dir.path(), d).unwrap();
    std::fs::write(doc.assets_dir().join("quadrants.svg"), svg).unwrap();
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui");
    let read = |name: &str| std::fs::read(ui.join(name)).ok();
    let single = dir.path().join("single.html");
    crate::export::export_single(&doc.deck, &doc.assets_dir(), &single, &read).unwrap();
    let html = std::fs::read_to_string(&single).unwrap();
    assert!(html.contains("data:image/svg+xml;base64,"));
    assert!(html.contains("cz-crop-viewport"));
    assert!(html.contains("zoom:1.25;"));
    let folder = dir.path().join("website");
    crate::export::export_folder(&doc.deck, &doc.assets_dir(), &folder, &read).unwrap();
    assert_eq!(std::fs::read_to_string(folder.join("assets/quadrants.svg")).unwrap(), svg);
    let file = dir.path().join("saved.cadenza");
    doc.save(&file).unwrap();
    let reopened = Document::open(dir.path(), &file).unwrap();
    assert_eq!(doc.deck, reopened.deck);
    assert_eq!(std::fs::read_to_string(reopened.assets_dir().join("quadrants.svg")).unwrap(), svg);
    // Optional local proof artifacts, outside the repository and user's files.
    if let Ok(path) = std::env::var("CADENZA_EDITING_PROOF_DIR") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::copy(single, path.join("export.html")).unwrap();
        std::fs::copy(file, path.join("editing.cadenza")).unwrap();
    }
}
