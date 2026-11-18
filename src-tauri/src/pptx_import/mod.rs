//! Bounded, offline import of an explicit editable subset of PresentationML.
//! Parsing never accesses the active document. Assets go only to caller-owned
//! staging; the caller publishes a new imported document after success.
mod archive;
mod charts;
mod drawing;
mod text;

use crate::model::*;
use anyhow::{Context, ensure};
use archive::{Archive, attr, child, desc, named, num, rel_attr};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use text::Theme;

const MAX_NATIVE_JSON: usize = 64 * 1024 * 1024;

/// Cumulative conversion work/allocation budget, shared by every slide/object.
/// Unlike the serialized-size guard, this is charged BEFORE allocating output.
struct MaterializationBudget(usize);
impl MaterializationBudget {
    fn charge(&mut self, bytes: usize) -> anyhow::Result<()> {
        self.0 = self
            .0
            .checked_sub(bytes)
            .context(n_("PowerPoint conversion exceeds the materialization limit"))?;
        Ok(())
    }
    fn array<T>(&mut self, count: usize) -> anyhow::Result<()> {
        self.charge(count.checked_mul(std::mem::size_of::<T>()).context(n_("PowerPoint conversion size overflow"))?)
    }
    fn escaped(&mut self, text: &str) -> anyhow::Result<()> {
        self.charge(text.len().checked_mul(6).context(n_("PowerPoint conversion size overflow"))?)
    }
}

/// A reused master can expand far beyond the input ZIP. Charge materialized
/// slides incrementally, then check native pretty JSON before publishing assets.
struct JsonBudget(usize);
impl std::io::Write for JsonBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.0 {
            return Err(std::io::Error::other(n_("Imported presentation exceeds the 64 MiB native document limit")));
        }
        self.0 -= bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Marks persistent report messages for the interface translation catalog.
const fn n_(message: &'static str) -> &'static str {
    message
}

#[derive(Debug)]
pub struct ImportCandidate {
    pub deck: Deck,
    pub issues: Vec<ImportIssue>,
}

struct Importer<'a> {
    archive: &'a Archive,
    issues: Vec<ImportIssue>,
    assets: BTreeSet<String>,
    slide_ids: BTreeMap<String, String>,
    budget: MaterializationBudget,
}

impl Importer<'_> {
    fn issue(&mut self, slide: usize, element: Option<&str>, feature: &str, code: &str, disposition: IssueDisposition, message: &str) {
        if self.issues.len() < 10_000 {
            self.issues.push(ImportIssue {
                code: code.into(),
                severity: IssueSeverity::Warning,
                slide,
                element: element.map(str::to_owned),
                feature: feature.into(),
                disposition,
                message: message.into(),
            });
        } else if self.issues.len() == 10_000 {
            self.issues.push(ImportIssue {
                code: "report-limit".into(),
                severity: IssueSeverity::Warning,
                slide: 0,
                element: None,
                feature: n_("conversion report").into(),
                disposition: IssueDisposition::Omitted,
                message: n_(
                    "More than 10,000 conversion details were found. Further details were omitted from this report; inspect the presentation carefully.",
                )
                .into(),
            });
        }
    }
    fn image(&mut self, part: &str) -> anyhow::Result<Option<(u32, u32)>> {
        let Some(bytes) = self.archive.parts.get(part) else { return Ok(None) };
        let ext = part.rsplit('.').next().unwrap_or_default().to_ascii_lowercase();
        if !["png", "jpg", "jpeg", "gif", "webp", "bmp"].contains(&ext.as_str()) {
            return Ok(None);
        }
        let size = imagesize::blob_size(bytes).context(n_("Invalid embedded PowerPoint image"))?;
        ensure!(
            size.width <= 16384 && size.height <= 16384 && size.width as u64 * size.height as u64 <= 40_000_000,
            n_("PowerPoint image exceeds 40 million pixels")
        );
        self.assets.insert(part.into());
        Ok(Some((size.width as u32, size.height as u32)))
    }
}

pub fn parse(path: &Path, staging_assets: &Path) -> anyhow::Result<ImportCandidate> {
    parse_with_budget(path, staging_assets, MAX_NATIVE_JSON)
}

fn parse_with_budget(path: &Path, staging_assets: &Path, allocation_limit: usize) -> anyhow::Result<ImportCandidate> {
    let archive = Archive::open(path)?;
    let root = archive
        .rels("")?
        .into_iter()
        .find(|r| r.kind == "officeDocument" && !r.external)
        .map(|r| r.target)
        .context(n_("PowerPoint presentation relationship is missing"))?;
    let pres = archive.xml(&root)?;
    ensure!(named(pres.root_element(), "presentation"), n_("Unsupported PowerPoint XML dialect"));
    let size = child(pres.root_element(), "sldSz").context(n_("PowerPoint slide size is missing"))?;
    let width = (num(size, "cx", 0.) / 9525.).round();
    let height = (num(size, "cy", 0.) / 9525.).round();
    ensure!(
        (320.0..=7680.).contains(&width) && (240.0..=4320.).contains(&height),
        n_("PowerPoint slide size is outside the supported range")
    );
    let rels = archive.rels(&root)?;
    let mut paths = vec![];
    for entry in child(pres.root_element(), "sldIdLst")
        .context(n_("PowerPoint slide list is missing"))?
        .children()
        .filter(|n| named(*n, "sldId"))
    {
        let id = rel_attr(entry, "id").context(n_("PowerPoint slide relationship is missing"))?;
        let r = rels
            .iter()
            .find(|r| r.id == id && r.kind == "slide" && !r.external)
            .context(n_("Invalid PowerPoint slide relationship"))?;
        ensure!(!paths.contains(&r.target), n_("PowerPoint slide list repeats a part"));
        paths.push(r.target.clone());
    }
    ensure!(!paths.is_empty() && paths.len() <= 1000, n_("PowerPoint must have 1–1000 slides"));
    let mut importer = Importer {
        archive: &archive,
        issues: vec![],
        assets: BTreeSet::new(),
        slide_ids: paths.iter().map(|p| (p.clone(), crate::util::new_id("s"))).collect(),
        budget: MaterializationBudget(allocation_limit),
    };
    let title = archive
        .xml("docProps/core.xml")
        .ok()
        .and_then(|d| {
            d.descendants()
                .find(|n| n.is_element() && n.tag_name().name() == "title")
                .and_then(|n| n.text())
                .map(str::to_owned)
        })
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into());
    let mut deck = Deck {
        title,
        width: width as u32,
        height: height as u32,
        theme: "cadenza-paper".into(),
        ..Deck::default()
    };
    let mut materialized = JsonBudget(MAX_NATIVE_JSON);
    let mut object_count = 0usize;
    for (index, part) in paths.iter().enumerate() {
        importer.budget.array::<Slide>(1)?;
        let slide_no = index + 1;
        let sdoc = archive.xml(part)?;
        let slide_root = sdoc.root_element();
        ensure!(named(slide_root, "sld"), n_("Invalid PowerPoint slide part"));
        let layout_path = archive.related(part, "slideLayout")?;
        let mut chain = BTreeSet::from([part.clone()]);
        if let Some(p) = &layout_path {
            ensure!(chain.insert(p.clone()), n_("Cyclic PowerPoint layout relationship"));
        }
        let layout = layout_path.as_ref().map(|p| archive.xml(p)).transpose()?;
        ensure!(
            layout.as_ref().is_none_or(|d| named(d.root_element(), "sldLayout")),
            n_("Invalid PowerPoint layout part")
        );
        let master_path = layout_path.as_ref().map(|p| archive.related(p, "slideMaster")).transpose()?.flatten();
        if let Some(p) = &master_path {
            ensure!(chain.insert(p.clone()), n_("Cyclic PowerPoint master relationship"));
        }
        let master = master_path.as_ref().map(|p| archive.xml(p)).transpose()?;
        ensure!(
            master.as_ref().is_none_or(|d| named(d.root_element(), "sldMaster")),
            n_("Invalid PowerPoint master part")
        );
        let theme_path = master_path
            .as_ref()
            .map(|p| archive.related(p, "theme"))
            .transpose()?
            .flatten()
            .or(archive.related(&root, "theme")?);
        if let Some(p) = &theme_path {
            ensure!(chain.insert(p.clone()), n_("Cyclic PowerPoint theme relationship"));
        }
        let theme_doc = theme_path.as_ref().map(|p| archive.xml(p)).transpose()?;
        ensure!(
            theme_doc.as_ref().is_none_or(|d| named(d.root_element(), "theme")),
            n_("Invalid PowerPoint theme part")
        );
        let mut theme = theme_doc.as_ref().map(|d| Theme::from_xml(d.root_element())).unwrap_or_default();
        for layer in master
            .as_ref()
            .map(|d| d.root_element())
            .into_iter()
            .chain(layout.as_ref().map(|d| d.root_element()))
            .chain(std::iter::once(slide_root))
        {
            let mapping = child(layer, "clrMap").or_else(|| child(layer, "clrMapOvr").and_then(|n| child(n, "overrideClrMapping")));
            if let Some(mapping) = mapping {
                for a in mapping.attributes() {
                    if let Some(color) = theme.colors.get(a.value()).cloned() {
                        theme.colors.insert(a.name().into(), color);
                    }
                }
            }
        }
        let stree = desc(slide_root, "spTree").context(n_("PowerPoint slide drawing tree is missing"))?;
        let ltree = layout.as_ref().and_then(|d| desc(d.root_element(), "spTree"));
        let mtree = master.as_ref().and_then(|d| desc(d.root_element(), "spTree"));
        let defaults = master
            .as_ref()
            .and_then(|d| desc(d.root_element(), "txStyles"))
            .or_else(|| child(pres.root_element(), "defaultTextStyle"));
        let mut slide = Slide {
            id: importer.slide_ids[part].clone(),
            hidden: matches!(attr(slide_root, "show"), Some("0" | "false")),
            background: Background {
                kind: "color".into(),
                color: "#ffffff".into(),
                ..Background::default()
            },
            ..Slide::default()
        };
        let layers: Vec<_> = std::iter::once((slide_root, part.as_str()))
            .chain(layout.as_ref().zip(layout_path.as_ref()).map(|(d, p)| (d.root_element(), p.as_str())))
            .chain(master.as_ref().zip(master_path.as_ref()).map(|(d, p)| (d.root_element(), p.as_str())))
            .collect();
        for (layer, owner) in &layers {
            if let Some(bg) = child(*layer, "cSld").and_then(|n| child(n, "bg")) {
                if let Some(props) = child(bg, "bgPr") {
                    if let Some(color) = text::fill(props, &theme) {
                        slide.background.color = color;
                    } else if let Some(id) = desc(props, "blip").and_then(|n| rel_attr(n, "embed")) {
                        if let Some(r) = archive.rels(owner)?.into_iter().find(|r| r.id == id && !r.external) {
                            if importer.image(&r.target)?.is_some() {
                                slide.background.kind = "image".into();
                                slide.background.image = r.target;
                            } else {
                                importer.issue(
                                    slide_no,
                                    None,
                                    n_("background image"),
                                    "unsupported-background",
                                    IssueDisposition::Approximated,
                                    n_("The background image format is unsupported."),
                                );
                            }
                        }
                    } else {
                        importer.issue(
                            slide_no,
                            None,
                            n_("background fill"),
                            "unsupported-background",
                            IssueDisposition::Approximated,
                            n_("The background fill was replaced by white."),
                        );
                    }
                } else if let Some(color) = child(bg, "bgRef").and_then(|n| n.children().find_map(|n| text::color(n, &theme))) {
                    slide.background.color = color;
                    importer.issue(
                        slide_no,
                        None,
                        n_("background style"),
                        "background-style",
                        IssueDisposition::Approximated,
                        n_("The theme background style was approximated by its resolved color."),
                    );
                }
                break;
            }
        }
        if !matches!(attr(slide_root, "showMasterSp"), Some("0" | "false")) {
            if let Some((tree, path)) = mtree.zip(master_path.as_ref()) {
                drawing::tree(
                    &mut importer,
                    tree,
                    &[],
                    defaults,
                    path,
                    &theme,
                    slide_no,
                    drawing::Matrix::root(),
                    true,
                    &mut slide.elements,
                )?;
            }
            if let Some((tree, path)) = ltree.zip(layout_path.as_ref()) {
                drawing::tree(
                    &mut importer,
                    tree,
                    &[],
                    defaults,
                    path,
                    &theme,
                    slide_no,
                    drawing::Matrix::root(),
                    true,
                    &mut slide.elements,
                )?;
            }
        }
        let inherited: Vec<_> = ltree.into_iter().chain(mtree).collect();
        drawing::tree(
            &mut importer,
            stree,
            &inherited,
            defaults,
            part,
            &theme,
            slide_no,
            drawing::Matrix::root(),
            false,
            &mut slide.elements,
        )?;
        if let Some(notes_path) = archive.related(part, "notesSlide")? {
            let notes = archive.xml(&notes_path)?;
            ensure!(named(notes.root_element(), "notes"), n_("Invalid PowerPoint notes part"));
            let mut lines = Vec::new();
            for shape in notes
                .descendants()
                .filter(|n| named(*n, "sp"))
                .filter(|n| desc(*n, "ph").and_then(|p| attr(p, "type")).is_none_or(|t| t == "body"))
            {
                for paragraph in shape.descendants().filter(|p| named(*p, "p")) {
                    importer.budget.array::<String>(1)?;
                    let mut line = String::new();
                    for text in paragraph.descendants().filter(|t| named(*t, "t")).filter_map(|t| t.text()) {
                        // The line and the final joined notes coexist briefly.
                        importer.budget.escaped(text)?;
                        line.push_str(text);
                    }
                    lines.push(line);
                }
            }
            slide.notes = lines.join("\n");
        }
        for feature in ["transition", "timing"] {
            if child(slide_root, feature).is_some() {
                importer.issue(
                    slide_no,
                    None,
                    feature,
                    "presentation-behavior-loss",
                    IssueDisposition::Omitted,
                    n_("PowerPoint animations/transitions are not retained by this import; slide content remains editable."),
                );
            }
        }
        object_count += slide.elements.len();
        ensure!(object_count <= 50_000, n_("PowerPoint presentation has too many objects"));
        serde_json::to_writer(&mut materialized, &slide)?;
        deck.slides.push(slide);
    }
    importer.issue(
        0,
        None,
        "fonts",
        "font-availability",
        IssueDisposition::Approximated,
        n_("Fonts are referenced by name. Missing fonts are substituted by the system; embedded PowerPoint font programs are not imported."),
    );
    deck.import_issues = importer.issues.clone();
    // The ZIP writer uses pretty JSON, so a compact-only check could still
    // produce an imported document that cannot be saved in the native format.
    serde_json::to_writer_pretty(JsonBudget(MAX_NATIVE_JSON), &deck)?;
    // All XML, objects and relationships have been validated before publishing
    // assets into the isolated staging directory supplied by the caller.
    std::fs::create_dir_all(staging_assets)?;
    let mut names = BTreeMap::new();
    for part in &importer.assets {
        let bytes = &archive.parts[part];
        let ext = part.rsplit('.').next().unwrap_or("png");
        names.insert(part.clone(), crate::document::store_asset(staging_assets, bytes, ext)?);
    }
    for slide in &mut deck.slides {
        if let Some(name) = names.get(&slide.background.image) {
            slide.background.image = name.clone();
        }
        for el in &mut slide.elements {
            if let Content::Image(image) = &mut el.content
                && let Some(name) = names.get(&image.src)
            {
                image.src = name.clone();
            }
        }
    }
    deck.normalize();
    crate::sanitize::deck(&mut deck);
    serde_json::to_writer_pretty(JsonBudget(MAX_NATIVE_JSON), &deck)?;
    Ok(ImportCandidate { deck, issues: importer.issues })
}

#[cfg(test)]
mod tests;
