use super::*;
use std::io::Write;
use zip::write::SimpleFileOptions;

const NS: &str = r#"xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart""#;
fn relationships(entries: &[(&str, &str, &str)]) -> String {
    format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{}</Relationships>"#,
        entries
            .iter()
            .map(|(id, kind, target)| format!(
                r#"<Relationship Id="{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}" Target="{target}"/>"#
            ))
            .collect::<String>()
    )
}
fn slide(shapes: &str) -> String {
    format!("<p:sld {NS}><p:cSld><p:spTree>{shapes}</p:spTree></p:cSld></p:sld>")
}
fn xfrm(x: u32, y: u32, w: u32, h: u32) -> String {
    format!(
        r#"<a:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm>"#,
        x * 9525,
        y * 9525,
        w * 9525,
        h * 9525
    )
}
fn text_shape(id: u32, text: &str) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="Text {id}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr>{}</p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr sz="2400"/><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp>"#,
        xfrm(100, 100, 500, 100)
    )
}
fn fixture() -> BTreeMap<String, Vec<u8>> {
    let mut p = BTreeMap::new();
    let text=[
        ("[Content_Types].xml",r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slides/slide7.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/><Override PartName="/ppt/slides/slide2.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/></Types>"#.into()),
        ("_rels/.rels",relationships(&[("rId1","officeDocument","ppt/presentation.xml")])),
        ("ppt/presentation.xml",format!(r#"<p:presentation {NS}><p:sldIdLst><p:sldId id="256" r:id="rId7"/><p:sldId id="257" r:id="rId2"/></p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/></p:presentation>"#)),
        ("ppt/_rels/presentation.xml.rels",relationships(&[("rId7","slide","slides/slide7.xml"),("rId2","slide","slides/slide2.xml")])),
        ("ppt/slides/slide7.xml",slide(&text_shape(1,"First"))),
        ("ppt/slides/slide2.xml",slide(&text_shape(2,"Second"))),
    ];
    for (name, value) in text {
        p.insert(name.into(), value.into_bytes());
    }
    p
}
fn write_package(path: &Path, parts: &BTreeMap<String, Vec<u8>>) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, data) in parts {
        zip.start_file(name, SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated))
            .unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
}
fn parse_fixture(parts: BTreeMap<String, Vec<u8>>) -> (tempfile::TempDir, ImportCandidate) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("foreign.pptx");
    write_package(&path, &parts);
    let result = parse(&path, &dir.path().join("assets")).unwrap();
    (dir, result)
}

#[test]
fn ordered_editable_foreign_slides_persist_and_import_dirty() {
    let (dir, result) = parse_fixture(fixture());
    assert_eq!((result.deck.width, result.deck.height), (1280, 720));
    assert!(result.deck.slides[0].elements[0].content.text().unwrap().html.contains("First"));
    assert!(result.deck.slides[1].elements[0].content.text().unwrap().html.contains("Second"));
    let mut doc = crate::document::Document::from_import(dir.path(), result.deck.clone()).unwrap();
    assert!(doc.is_dirty());
    assert!(doc.path.is_none());
    assert!(doc.history.undo_label().is_none());
    let native = dir.path().join("saved.cadenza");
    doc.save(&native).unwrap();
    assert!(!doc.is_dirty());
    let reopened = crate::document::Document::open(dir.path(), &native).unwrap();
    assert_eq!(reopened.deck, result.deck);
    assert!(!reopened.deck.import_issues.is_empty());
}

#[test]
fn inherits_layout_geometry_master_font_theme_color_and_artwork() {
    let mut parts = fixture();
    let title = r#"<p:sp><p:nvSpPr><p:cNvPr id="1" name="Title"/><p:cNvSpPr/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>Inherited</a:t></a:r></a:p></p:txBody></p:sp>"#;
    parts.insert(
        "ppt/slides/slide7.xml".into(),
        slide(&title.replace("type=\"title\"", "type=\"ctrTitle\"")).into_bytes(),
    );
    parts.insert(
        "ppt/slides/_rels/slide7.xml.rels".into(),
        relationships(&[("layout", "slideLayout", "../slideLayouts/layout.xml")]).into_bytes(),
    );
    let layout = format!(
        r#"<p:sldLayout {NS}><p:cSld><p:spTree>{}</p:spTree></p:cSld></p:sldLayout>"#,
        title
            .replace("<p:spPr/>", &format!("<p:spPr>{}</p:spPr>", xfrm(50, 60, 1000, 120)))
            .replace("<a:bodyPr/>", "<a:bodyPr anchor=\"ctr\"/>")
    );
    parts.insert("ppt/slideLayouts/layout.xml".into(), layout.into_bytes());
    parts.insert(
        "ppt/slideLayouts/_rels/layout.xml.rels".into(),
        relationships(&[("master", "slideMaster", "../slideMasters/master.xml")]).into_bytes(),
    );
    let master = format!(
        r#"<p:sldMaster {NS}><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val="F0F0F0"/></a:solidFill></p:bgPr></p:bg><p:spTree>{}</p:spTree></p:cSld><p:txStyles><p:titleStyle><a:lvl1pPr><a:defRPr sz="3600"><a:solidFill><a:schemeClr val="accent1"/></a:solidFill><a:latin typeface="+mj-lt"/></a:defRPr></a:lvl1pPr></p:titleStyle></p:txStyles></p:sldMaster>"#,
        text_shape(8, "Master artwork")
    );
    parts.insert("ppt/slideMasters/master.xml".into(), master.into_bytes());
    parts.insert(
        "ppt/slideMasters/_rels/master.xml.rels".into(),
        relationships(&[("theme", "theme", "../theme/theme.xml")]).into_bytes(),
    );
    parts.insert("ppt/theme/theme.xml".into(),format!(r#"<a:theme {NS}><a:themeElements><a:clrScheme name="Test"><a:accent1><a:srgbClr val="123456"/></a:accent1></a:clrScheme><a:fontScheme name="Test"><a:majorFont><a:latin typeface="Test Heading"/></a:majorFont><a:minorFont><a:latin typeface="Test Body"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#).into_bytes());
    let (_, result) = parse_fixture(parts);
    let slide = &result.deck.slides[0];
    assert_eq!(slide.elements.len(), 2);
    assert_eq!(slide.background.color, "#f0f0f0");
    let el = &slide.elements[1];
    assert_eq!((el.x, el.y, el.w, el.h), (50., 60., 1000., 120.));
    let html = &el.content.text().unwrap().html;
    assert!(html.contains("font-size:48px"), "{html}");
    assert!(html.contains("Test Heading"));
    assert!(html.contains("#123456"));
    assert_eq!(el.content.text().unwrap().style.valign, "middle");
}

#[test]
fn embedded_image_crop_is_portable_and_external_relationships_are_not_fetched() {
    let mut parts = fixture();
    let pic = format!(
        r#"<p:pic><p:nvPicPr><p:cNvPr id="5" name="Photo"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="img"/><a:srcRect l="25000" r="25000"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}</p:spPr></p:pic>"#,
        xfrm(10, 20, 400, 200)
    );
    parts.insert("ppt/slides/slide7.xml".into(), slide(&pic).into_bytes());
    parts.insert(
        "ppt/slides/_rels/slide7.xml.rels".into(),
        relationships(&[("img", "image", "../media/pixel.png")]).into_bytes(),
    );
    let png = crate::util::encode_png(&[255, 0, 0, 255, 0, 255, 0, 255], 2, 1).unwrap();
    parts.insert("ppt/media/pixel.png".into(), png.clone());
    let (dir, result) = parse_fixture(parts.clone());
    let Content::Image(image) = &result.deck.slides[0].elements[0].content else {
        panic!()
    };
    assert_eq!(image.crop, Some(CropRect { x: 0.25, y: 0., w: 0.5, h: 1. }));
    assert_eq!(std::fs::read(dir.path().join("assets").join(&image.src)).unwrap(), png);
    parts.insert(
        "ppt/slides/_rels/slide7.xml.rels".into(),
        relationships(&[("img", "image", "https://invalid.example/image.png")])
            .replace("Target=", "TargetMode=\"External\" Target=")
            .into_bytes(),
    );
    let (_, result) = parse_fixture(parts);
    assert!(matches!(result.deck.slides[0].elements[0].content, Content::Text(_)));
    assert!(result.issues.iter().any(|i| i.disposition == IssueDisposition::Placeholder));
}

#[test]
fn actual_embedded_preview_keeps_pixels_rotation_opacity_and_reflection() {
    let mut parts = fixture();
    let pic = format!(
        r#"<p:pic><p:nvPicPr><p:cNvPr id="5" name="OLE preview"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="img"><a:alphaModFix amt="65000"/></a:blip></p:blipFill><p:spPr>{}</p:spPr></p:pic>"#,
        xfrm(10, 20, 400, 200).replace("<a:xfrm>", "<a:xfrm rot=\"5400000\" flipV=\"1\">")
    );
    let frame = format!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="4" name="OLE"/></p:nvGraphicFramePr>{}<a:graphic><a:graphicData><p:oleObj>{pic}</p:oleObj></a:graphicData></a:graphic></p:graphicFrame>"#,
        xfrm(10, 20, 400, 200)
    );
    parts.insert("ppt/slides/slide7.xml".into(), slide(&frame).into_bytes());
    parts.insert(
        "ppt/slides/_rels/slide7.xml.rels".into(),
        relationships(&[("img", "image", "../media/pixel.png")]).into_bytes(),
    );
    let png = crate::util::encode_png(&[255, 0, 0, 255], 1, 1).unwrap();
    parts.insert("ppt/media/pixel.png".into(), png.clone());
    let (dir, result) = parse_fixture(parts);
    let el = &result.deck.slides[0].elements[0];
    let Content::Image(image) = &el.content else { panic!() };
    assert!((el.rotation - 90.).abs() < 0.0001);
    assert!(image.flip_y);
    assert!((el.opacity - 0.65).abs() < 0.0001);
    assert_eq!(std::fs::read(dir.path().join("assets").join(&image.src)).unwrap(), png);
    assert!(
        result
            .issues
            .iter()
            .any(|i| i.code == "embedded-preview" && i.disposition == IssueDisposition::Rasterized)
    );
}

#[test]
fn color_luminance_uses_hsl_and_keeps_source_hue() {
    for (effect, expected) in [(r#"<a:lumMod val="50000"/>"#, "#800000"), (r#"<a:lumOff val="20000"/>"#, "#ff6666")] {
        let xml = format!(r#"<a:srgbClr {NS} val="FF0000">{effect}</a:srgbClr>"#);
        let doc = archive::parse_xml(xml.as_bytes()).unwrap();
        assert_eq!(text::color(doc.root_element(), &Theme::default()).as_deref(), Some(expected));
    }
}

#[test]
fn materialized_json_budget_accounts_for_repeated_slide_content_and_pretty_encoding() {
    let slide = Slide {
        notes: "Repeated master content".repeat(50),
        ..Slide::default()
    };
    let compact = serde_json::to_vec(&slide).unwrap().len();
    let mut budget = JsonBudget(2 * compact - 1);
    serde_json::to_writer(&mut budget, &slide).unwrap();
    assert!(serde_json::to_writer(&mut budget, &slide).unwrap_err().to_string().contains("64 MiB"));
    assert!(serde_json::to_writer_pretty(JsonBudget(compact), &slide).is_err());
}

#[test]
fn repeated_chart_references_share_a_budget_before_the_slide_is_built() {
    let dir = tempfile::tempdir().unwrap();
    for copies in [1, 80] {
        let mut parts = fixture();
        let frame = format!(
            r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="4" name="Repeated chart"/></p:nvGraphicFramePr>{}<a:graphic><a:graphicData><c:chart r:id="chart"/></a:graphicData></a:graphic></p:graphicFrame>"#,
            xfrm(20, 20, 600, 400)
        );
        parts.insert("ppt/slides/slide7.xml".into(), slide(&frame.repeat(copies)).into_bytes());
        parts.insert(
            "ppt/slides/_rels/slide7.xml.rels".into(),
            relationships(&[("chart", "chart", "../charts/chart.xml")]).into_bytes(),
        );
        parts.insert("ppt/charts/chart.xml".into(),format!(r#"<c:chartSpace {NS}><c:chart><c:plotArea><c:barChart><c:ser><c:cat><c:strLit><c:ptCount val="1"/><c:pt idx="0"><c:v>{}</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:ptCount val="1"/><c:pt idx="0"><c:v>42</c:v></c:pt></c:numLit></c:val></c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>"#,"Large label ".repeat(180)).into_bytes());
        let path = dir.path().join(format!("references-{copies}.pptx"));
        write_package(&path, &parts);
        let assets = dir.path().join(format!("assets-{copies}"));
        let result = parse_with_budget(&path, &assets, 32 * 1024);
        if copies == 1 {
            assert!(result.is_ok(), "single reference fits the small shared budget: {result:?}");
        } else {
            assert!(result.unwrap_err().to_string().contains("materialization limit"));
            assert!(!assets.exists(), "repeated reference rejection happens before publication");
        }
    }
}

#[test]
fn chart_sparse_point_vectors_are_budgeted_before_allocation() {
    let xml = format!(
        r#"<c:chartSpace {NS}><c:chart><c:plotArea><c:barChart><c:ser><c:val><c:numLit><c:ptCount val="10000"/></c:numLit></c:val></c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>"#
    );
    let doc = archive::parse_xml(xml.as_bytes()).unwrap();
    let mut budget = MaterializationBudget(4096);
    assert!(
        charts::parse(doc.root_element(), &Theme::default(), &mut budget)
            .unwrap_err()
            .to_string()
            .contains("materialization limit")
    );
    assert!(budget.0 > 0, "failed reservations do not consume or wrap the remaining budget");
}

#[test]
fn flat_multilevel_chart_cache_is_editable_but_hierarchical_categories_are_not_guessed() {
    for levels in [1, 2] {
        let level = r#"<c:lvl><c:pt idx="0"><c:v>First</c:v></c:pt><c:pt idx="1"><c:v>Second</c:v></c:pt></c:lvl>"#;
        let xml = format!(
            r#"<c:chartSpace {NS}><c:chart><c:plotArea><c:barChart><c:ser><c:cat><c:multiLvlStrRef><c:multiLvlStrCache><c:ptCount val="2"/>{}</c:multiLvlStrCache></c:multiLvlStrRef></c:cat><c:val><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>12</c:v></c:pt><c:pt idx="1"><c:v>18</c:v></c:pt></c:numLit></c:val></c:ser></c:barChart><c:valAx><c:txPr><a:p><a:pPr><a:defRPr sz="1800"><a:solidFill><a:srgbClr val="DBE4F3"/></a:solidFill></a:defRPr></a:pPr></a:p></c:txPr></c:valAx></c:plotArea></c:chart></c:chartSpace>"#,
            level.repeat(levels)
        );
        let doc = archive::parse_xml(xml.as_bytes()).unwrap();
        let parsed = charts::parse(doc.root_element(), &Theme::default(), &mut MaterializationBudget(32 * 1024)).unwrap();
        if levels == 1 {
            let parsed = parsed.unwrap();
            assert_eq!(parsed.labels, vec!["First", "Second"]);
            assert_eq!(parsed.series[0].values, vec![Some(12.), Some(18.)]);
            assert_eq!(parsed.color, "#dbe4f3");
            assert_eq!(parsed.font_size, 24.);
        } else {
            assert!(parsed.is_none());
        }
    }
}

#[test]
fn inherited_fonts_are_budgeted_for_every_text_run_before_html_growth() {
    let theme = Theme {
        minor: "Long inherited font ".repeat(80),
        ..Theme::default()
    };
    for copies in [1, 50] {
        let run = r#"<a:r><a:t>Text</a:t></a:r>"#;
        let xml = format!(r#"<p:sp {NS}><p:txBody><a:bodyPr/><a:p>{}</a:p></p:txBody></p:sp>"#, run.repeat(copies));
        let doc = archive::parse_xml(xml.as_bytes()).unwrap();
        let mut budget = MaterializationBudget(32 * 1024);
        let result = text::rich_text(doc.root_element(), &[], None, &theme, 1., &[], &mut budget);
        if copies == 1 {
            assert!(result.is_ok());
        } else {
            assert!(result.unwrap_err().to_string().contains("materialization limit"));
        }
    }
}

#[test]
fn shared_external_link_targets_are_budgeted_for_each_imported_action() {
    let dir = tempfile::tempdir().unwrap();
    for copies in [1, 30] {
        let mut parts = fixture();
        let linked = text_shape(1, "Link").replace(r#"name="Text 1"/>"#, r#"name="Text 1"><a:hlinkClick r:id="url"/></p:cNvPr>"#);
        parts.insert("ppt/slides/slide7.xml".into(), slide(&linked.repeat(copies)).into_bytes());
        parts.insert(
            "ppt/slides/_rels/slide7.xml.rels".into(),
            relationships(&[("url", "hyperlink", &format!("https://example.invalid/{}", "a".repeat(4000)))])
                .replace("Target=", "TargetMode=\"External\" Target=")
                .into_bytes(),
        );
        let path = dir.path().join(format!("links-{copies}.pptx"));
        write_package(&path, &parts);
        let result = parse_with_budget(&path, &dir.path().join(format!("assets-{copies}")), 32 * 1024);
        if copies == 1 {
            assert!(result.is_ok());
        } else {
            assert!(result.unwrap_err().to_string().contains("materialization limit"));
        }
    }
}

#[test]
fn text_insets_are_honest_and_uniform_insets_are_retained() {
    for (body, padding, warning) in [
        ("<a:bodyPr/>", 4.8, true),
        (r#"<a:bodyPr lIns="95250" rIns="95250" tIns="95250" bIns="95250"/>"#, 10., false),
    ] {
        let mut parts = fixture();
        parts.insert(
            "ppt/slides/slide7.xml".into(),
            slide(&text_shape(1, "Insets").replace("<a:bodyPr/>", body)).into_bytes(),
        );
        let (_, result) = parse_fixture(parts);
        assert!((result.deck.slides[0].elements[0].style.padding - padding).abs() < 0.00001);
        assert_eq!(result.issues.iter().any(|i| i.slide == 1 && i.code == "text-insets"), warning);
    }
}

#[test]
fn placeholder_geometry_is_validated_before_unsupported_content_is_preserved() {
    let mut parts = fixture();
    let shape = text_shape(1, "Invalid")
        .replace("<p:spPr>", "<p:spPr><a:custGeom/>")
        .replace("cx=\"4762500\"", "cx=\"999999999999\"");
    parts.insert("ppt/slides/slide7.xml".into(), slide(&shape).into_bytes());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.pptx");
    write_package(&path, &parts);
    assert!(parse(&path, &dir.path().join("assets")).unwrap_err().to_string().contains("geometry"));
    assert!(!dir.path().join("assets").exists());
}

#[test]
fn chart_cached_missing_values_remain_missing_and_have_no_fabricated_csv() {
    let mut parts = fixture();
    let frame = format!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="4" name="Chart"/></p:nvGraphicFramePr>{}<a:graphic><a:graphicData><c:chart r:id="chart"/></a:graphicData></a:graphic></p:graphicFrame>"#,
        xfrm(20, 20, 600, 400)
    );
    parts.insert("ppt/slides/slide7.xml".into(), slide(&frame).into_bytes());
    parts.insert(
        "ppt/slides/_rels/slide7.xml.rels".into(),
        relationships(&[("chart", "chart", "../charts/chart.xml")]).into_bytes(),
    );
    parts.insert("ppt/charts/chart.xml".into(),format!(r#"<c:chartSpace {NS}><c:chart><c:plotArea><c:barChart><c:barDir val="col"/><c:ser><c:tx><c:v>SE</c:v></c:tx><c:cat><c:strLit><c:ptCount val="3"/><c:pt idx="0"><c:v>A</c:v></c:pt><c:pt idx="1"><c:v>B</c:v></c:pt><c:pt idx="2"><c:v>C</c:v></c:pt></c:strLit></c:cat><c:val><c:numRef><c:f>Sheet1!A1:A3</c:f><c:numCache><c:ptCount val="3"/><c:pt idx="0"><c:v>10</c:v></c:pt><c:pt idx="2"><c:v>30</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>"#).into_bytes());
    let (_, result) = parse_fixture(parts);
    let el = &result.deck.slides[0].elements[0];
    let Content::Chart(chart) = &el.content else { panic!() };
    assert_eq!(chart.series[0].name, "SE");
    assert_eq!(chart.series[0].values, vec![Some(10.), None, Some(30.)]);
    assert!(el.data_source.is_none());
}

#[test]
fn malformed_archives_and_relationship_escapes_publish_no_assets() {
    for failure in ["escape", "cycle", "dtd", "depth", "malformed", "size"] {
        let mut parts = fixture();
        match failure {
            "escape" => {
                parts.insert(
                    "ppt/slides/_rels/slide7.xml.rels".into(),
                    relationships(&[("x", "image", "../../../private.png")]).into_bytes(),
                );
            }
            "cycle" => {
                parts.insert(
                    "ppt/slides/_rels/slide7.xml.rels".into(),
                    relationships(&[("x", "slideLayout", "slide7.xml")]).into_bytes(),
                );
            }
            "dtd" => {
                parts.insert("ppt/slides/slide7.xml".into(), b"<!DOCTYPE x [<!ENTITY x 'secret'>]><x>&x;</x>".to_vec());
            }
            "depth" => {
                parts.insert(
                    "ppt/slides/slide7.xml".into(),
                    format!("{}{}", "<x>".repeat(140), "</x>".repeat(140)).into_bytes(),
                );
            }
            "malformed" => {
                parts.insert("ppt/slides/slide7.xml".into(), b"<broken>".to_vec());
            }
            "size" => {
                parts.insert(
                    "ppt/presentation.xml".into(),
                    String::from_utf8(parts["ppt/presentation.xml"].clone())
                        .unwrap()
                        .replace("12192000", "999999999999")
                        .into_bytes(),
                );
            }
            _ => unreachable!(),
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.pptx");
        write_package(&path, &parts);
        let assets = dir.path().join("assets");
        assert!(parse(&path, &assets).is_err(), "{failure}");
        assert!(!assets.exists(), "{failure} left assets");
    }
}

#[test]
fn relative_relationships_accept_normal_parent_paths_and_reject_root_escape() {
    assert_eq!(
        archive::resolve("ppt/slides/slide1.xml", "../media/image1.png").unwrap(),
        "ppt/media/image1.png"
    );
    assert_eq!(
        archive::resolve("ppt/slides/slide1.xml", "/ppt/media/image%201.png").unwrap(),
        "ppt/media/image 1.png"
    );
    for target in ["../../../x", "..%2f..%2f..%2fx", "C:/secret", "..\\secret"] {
        assert!(archive::resolve("ppt/slides/s.xml", target).is_err());
    }
}

#[test]
fn independent_libreoffice_presentation_has_editable_text_shapes_and_tables() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/fixtures/pptx/foreign-libreoffice.pptx");
    let dir = tempfile::tempdir().unwrap();
    let result = parse(&path, &dir.path().join("assets")).unwrap();
    assert_eq!(result.deck.slides.len(), 2);
    let elements = &result.deck.slides[0].elements;
    assert!(
        elements
            .iter()
            .any(|e| e.content.text().is_some_and(|t| t.html.contains("Independent LibreOffice fixture")))
    );
    assert!(elements.iter().any(|e| matches!(&e.content,Content::Shape(s)if s.shape=="rect")));
    assert!(elements.iter().any(|e| matches!(&e.content,Content::Shape(s)if s.shape=="ellipse")));
    assert!(
        elements
            .iter()
            .any(|e| matches!(&e.content,Content::Table(t)if t.rows.iter().flatten().any(|v|v.contains("Brazil"))))
    );
    let table = elements.iter().find(|e| matches!(&e.content, Content::Table(_))).unwrap();
    assert_ne!(table.style.fill, "transparent", "white cell text must retain its original dark background");
    assert_ne!(table.style.fill, "#ffffff");
    let Content::Table(content) = &table.content else { unreachable!() };
    assert!(content.rows[0][0].contains("color:#ffffff"));
    let asset_url = |name: &str| name.to_owned();
    let ctx = crate::render::Ctx {
        mode: crate::render::Mode::Editor,
        asset_url: &asset_url,
        youtube_host: None,
    };
    let rendered = crate::render::slide_for_editor(&result.deck, &result.deck.slides[0], &ctx);
    assert!(
        rendered.html.contains(&format!("background:{}", table.style.fill)),
        "rendered table must retain its fill"
    );
}

#[test]
fn mixed_table_fills_remain_attached_to_editable_text_after_sanitization() {
    let cell = |fill: &str, ink: &str, text: &str| {
        format!(
            r#"<a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:rPr><a:solidFill><a:srgbClr val="{ink}"/></a:solidFill></a:rPr><a:t>{text}</a:t></a:r></a:p></a:txBody><a:tcPr><a:solidFill><a:srgbClr val="{fill}"/></a:solidFill></a:tcPr></a:tc>"#
        )
    };
    let mut parts = fixture();
    let table = format!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="4" name="Mixed cells"/></p:nvGraphicFramePr>{}<a:graphic><a:graphicData><a:tbl><a:tblPr/><a:tr>{}{}</a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#,
        xfrm(10, 20, 500, 100),
        cell("000080", "FFFFFF", "Light text"),
        cell("FFFACD", "000000", "Dark text")
    );
    parts.insert("ppt/slides/slide7.xml".into(), slide(&table).into_bytes());
    let (_, result) = parse_fixture(parts);
    let Content::Table(table) = &result.deck.slides[0].elements[0].content else {
        panic!()
    };
    assert!(table.rows[0][0].contains("background-color:#000080"));
    assert!(table.rows[0][0].contains("color:#ffffff"));
    assert!(table.rows[0][1].contains("background-color:#fffacd"));
    assert!(table.rows[0][1].contains("color:#000000"));
    assert!(result.issues.iter().any(|i| i.code == "table-cell-backgrounds"));
}

#[test]
fn unresolved_table_fill_uses_readable_text_and_reports_color_change() {
    let literal = r#"<p style="text-align:left"><span style="color:#ffffff">literal color:#ffffff and #ffffff</span></p>"#;
    assert_eq!(
        text::legible_on_white(literal).0,
        r#"<p style="text-align:left"><span style="color:#000000">literal color:#ffffff and #ffffff</span></p>"#
    );
    let mut parts = fixture();
    let body = text_shape(1, "Light text").replace(
        "<a:rPr sz=\"2400\"/>",
        r#"<a:rPr sz="2400"><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></a:rPr>"#,
    );
    let body = body.split("<p:txBody>").nth(1).unwrap().split("</p:txBody>").next().unwrap();
    let table = format!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="4" name="Unresolved theme"/></p:nvGraphicFramePr>{}<a:graphic><a:graphicData><a:tbl><a:tblPr/><a:tr><a:tc><a:txBody>{body}</a:txBody><a:tcPr/></a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#,
        xfrm(10, 20, 500, 100)
    );
    parts.insert("ppt/slides/slide7.xml".into(), slide(&table).into_bytes());
    let (_, result) = parse_fixture(parts);
    let el = &result.deck.slides[0].elements[0];
    let Content::Table(table) = &el.content else { panic!() };
    assert_eq!(el.style.fill, "#ffffff");
    assert!(table.rows[0][0].contains("color:#000000"));
    assert!(!table.rows[0][0].contains("color:#ffffff"));
    assert!(result.issues.iter().any(|i| i.code == "table-text-contrast"));
}

#[test]
fn resolved_transparent_table_cells_keep_the_slide_background_visible() {
    let cell = |alpha| {
        format!(
            r#"<a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:rPr><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></a:rPr><a:t>Light text</a:t></a:r></a:p></a:txBody><a:tcPr><a:solidFill><a:srgbClr val="94A3B8"><a:alpha val="{alpha}"/></a:srgbClr></a:solidFill></a:tcPr></a:tc>"#
        )
    };
    let table = format!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="4" name="Transparent table"/></p:nvGraphicFramePr>{}<a:graphic><a:graphicData><a:tbl><a:tblPr/><a:tr>{}{}</a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#,
        xfrm(10, 20, 500, 100),
        cell(0),
        cell(12000)
    );
    let mut parts = fixture();
    parts.insert("ppt/slides/slide7.xml".into(), slide(&table).into_bytes());
    let (_, result) = parse_fixture(parts);
    let el = &result.deck.slides[0].elements[0];
    assert_eq!(el.style.fill, "transparent");
    let Content::Table(table) = &el.content else { panic!() };
    assert!(
        table.rows[0][0].replace(' ', "").contains("background-color:rgba(148,163,184,0)"),
        "{}",
        table.rows[0][0]
    );
    assert!(
        table.rows[0][1].replace(' ', "").contains("background-color:rgba(148,163,184,0.12)"),
        "{}",
        table.rows[0][1]
    );
    assert!(table.rows[0].iter().all(|cell| cell.contains("color:#ffffff")));
    assert!(!result.issues.iter().any(|i| i.code == "table-text-contrast"));
}

#[test]
fn groups_notes_and_internal_links_keep_their_targets_without_private_metadata() {
    let mut parts = fixture();
    let grouped = text_shape(1, "Grouped").replace(&xfrm(100, 100, 500, 100), &xfrm(10, 20, 50, 30));
    let group = format!(
        r#"<p:grpSp><p:nvGrpSpPr/><p:grpSpPr><a:xfrm><a:off x="952500" y="952500"/><a:ext cx="3810000" cy="3810000"/><a:chOff x="0" y="0"/><a:chExt cx="1905000" cy="1905000"/></a:xfrm></p:grpSpPr>{grouped}</p:grpSp>"#
    );
    let linked = text_shape(3, "Go to second").replace(r#"name="Text 3"/>"#, r#"name="Text 3"><a:hlinkClick r:id="next"/></p:cNvPr>"#);
    parts.insert("ppt/slides/slide7.xml".into(), slide(&(group + &linked)).into_bytes());
    parts.insert(
        "ppt/slides/_rels/slide7.xml.rels".into(),
        relationships(&[("next", "slide", "slide2.xml"), ("notes", "notesSlide", "../notesSlides/notes.xml")]).into_bytes(),
    );
    parts.insert(
        "ppt/notesSlides/notes.xml".into(),
        format!(
            r#"<p:notes {NS}><p:cSld><p:spTree>{}</p:spTree></p:cSld></p:notes>"#,
            text_shape(1, "Presenter notes")
        )
        .into_bytes(),
    );
    let (_, result) = parse_fixture(parts);
    let first = &result.deck.slides[0];
    assert_eq!(
        (first.elements[0].x, first.elements[0].y, first.elements[0].w, first.elements[0].h),
        (120., 140., 100., 60.)
    );
    assert!(first.elements[0].content.text().unwrap().html.contains("font-size:64px"));
    assert_eq!(first.elements[1].action.slide, result.deck.slides[1].id);
    assert_eq!(first.notes, "Presenter notes");
    assert!(result.issues.iter().any(|i| i.code == "flattened-group"));
}

#[test]
fn unsupported_objects_stay_visible_and_real_alternate_fallbacks_are_used() {
    let mut parts = fixture();
    let unsupported = format!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="9" name="SmartArt"/></p:nvGraphicFramePr>{}<a:graphic><a:graphicData uri="unsupported"/></a:graphic></p:graphicFrame>"#,
        xfrm(20, 40, 500, 300)
    );
    let fallback = format!(
        r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice Requires="unknown"><unknown/></mc:Choice><mc:Fallback>{}</mc:Fallback></mc:AlternateContent>"#,
        text_shape(7, "Real fallback")
    );
    parts.insert("ppt/slides/slide7.xml".into(), slide(&(unsupported + &fallback)).into_bytes());
    let (_, result) = parse_fixture(parts);
    let elements = &result.deck.slides[0].elements;
    assert_eq!(elements.len(), 2);
    assert!(elements[0].content.text().unwrap().html.contains("Unsupported PowerPoint content"));
    assert!(elements[1].content.text().unwrap().html.contains("Real fallback"));
    assert!(
        result
            .issues
            .iter()
            .any(|i| i.disposition == IssueDisposition::Placeholder && i.element.as_deref() == Some(elements[0].id.as_str()))
    );
}

#[test]
fn resource_limits_reject_expanded_xml_nodes_and_oversized_image_dimensions() {
    let mut parts = fixture();
    parts.insert("unused.xml".into(), format!("<root>{}</root>", "<node/>".repeat(200_001)).into_bytes());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nodes.pptx");
    write_package(&path, &parts);
    assert!(parse(&path, &dir.path().join("assets")).unwrap_err().to_string().contains("XML"));
    assert!(!dir.path().join("assets").exists());
    let mut parts = fixture();
    let pic = format!(
        r#"<p:pic><p:nvPicPr><p:cNvPr id="1" name="Large"/></p:nvPicPr><p:blipFill><a:blip r:embed="img"/></p:blipFill><p:spPr>{}</p:spPr></p:pic>"#,
        xfrm(0, 0, 100, 100)
    );
    parts.insert("ppt/slides/slide7.xml".into(), slide(&pic).into_bytes());
    parts.insert(
        "ppt/slides/_rels/slide7.xml.rels".into(),
        relationships(&[("img", "image", "../media/large.png")]).into_bytes(),
    );
    let mut png = crate::util::encode_png(&[255, 0, 0, 255], 1, 1).unwrap();
    png[16..20].copy_from_slice(&20000u32.to_be_bytes());
    parts.insert("ppt/media/large.png".into(), png);
    let path = dir.path().join("pixels.pptx");
    write_package(&path, &parts);
    assert!(parse(&path, &dir.path().join("assets")).unwrap_err().to_string().contains("pixels"));
    assert!(!dir.path().join("assets").exists());
}

#[test]
fn unsupported_merged_table_is_a_persistent_visible_marker() {
    let mut parts = fixture();
    let table = format!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="4" name="Merged table"/></p:nvGraphicFramePr>{}<a:graphic><a:graphicData><a:tbl><a:tr h="100"><a:tc gridSpan="2"><a:txBody><a:bodyPr/><a:p><a:r><a:t>Merged</a:t></a:r></a:p></a:txBody></a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#,
        xfrm(10, 20, 500, 100)
    );
    parts.insert("ppt/slides/slide7.xml".into(), slide(&table).into_bytes());
    let (_, result) = parse_fixture(parts);
    assert!(matches!(result.deck.slides[0].elements[0].content, Content::Text(_)));
    assert!(
        result
            .deck
            .import_issues
            .iter()
            .any(|i| i.feature == "merged table" && i.disposition == IssueDisposition::Placeholder)
    );
}
