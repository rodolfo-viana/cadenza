use super::archive::{child, desc, named, num, rel_attr};
use super::n_;
use super::text::{self, Theme};
use super::{Importer, charts};
use crate::model::*;
use roxmltree::Node;

const EMU: f64 = 9525.;

#[derive(Clone, Copy)]
pub struct Matrix(pub [f64; 6]);
impl Matrix {
    pub fn root() -> Self {
        Self([1. / EMU, 0., 0., 1. / EMU, 0., 0.])
    }
    fn shift(x: f64, y: f64) -> Self {
        Self([1., 0., 0., 1., x, y])
    }
    fn scale(x: f64, y: f64) -> Self {
        Self([x, 0., 0., y, 0., 0.])
    }
    fn rotate(degrees: f64) -> Self {
        let (s, c) = degrees.to_radians().sin_cos();
        Self([c, s, -s, c, 0., 0.])
    }
    fn then(self, other: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [g, h, i, j, k, l] = other.0;
        Self([a * g + c * h, b * g + d * h, a * i + c * j, b * i + d * j, a * k + c * l + e, b * k + d * l + f])
    }
    fn point(self, x: f64, y: f64) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.0;
        (a * x + c * y + e, b * x + d * y + f)
    }
    fn font_scale(self) -> f64 {
        ((self.0[0].hypot(self.0[1]) + self.0[2].hypot(self.0[3])) * EMU / 2.).clamp(0.01, 100.)
    }
}

fn transform<'a, 'i>(n: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    child(n, "xfrm").or_else(|| child(n, "spPr").and_then(|n| child(n, "xfrm")))
}
fn pair(n: Option<Node<'_, '_>>, x: &str, y: &str, default: (f64, f64)) -> (f64, f64) {
    n.map(|n| (num(n, x, default.0), num(n, y, default.1))).unwrap_or(default)
}

fn geometry(n: Node<'_, '_>, inherited: &[Node<'_, '_>], parent: Matrix) -> (Element, bool, bool) {
    let xfrm = transform(n).or_else(|| inherited.iter().find_map(|n| transform(*n)));
    let (x, y) = pair(xfrm.and_then(|n| child(n, "off")), "x", "y", (40. * EMU, 40. * EMU));
    let (w, h) = pair(xfrm.and_then(|n| child(n, "ext")), "cx", "cy", (320. * EMU, 120. * EMU));
    let angle = xfrm.map(|n| num(n, "rot", 0.) / 60000.).unwrap_or(0.);
    let flip = |key| {
        if xfrm.is_some_and(|n| matches!(n.attribute(key), Some("1" | "true"))) {
            -1.
        } else {
            1.
        }
    };
    let m = parent
        .then(Matrix::shift(x + w / 2., y + h / 2.))
        .then(Matrix::rotate(angle))
        .then(Matrix::scale(flip("flipH"), flip("flipV")));
    let (cx, cy) = m.point(0., 0.);
    let [a, b, c, d, _, _] = m.0;
    let width = a.hypot(b) * w.abs();
    let height = c.hypot(d) * h.abs();
    let mut el = Element {
        x: cx - width / 2.,
        y: cy - height / 2.,
        w: width.max(4.),
        h: height.max(4.),
        rotation: b.atan2(a).to_degrees(),
        ..Element::default()
    };
    el.name = desc(n, "cNvPr").and_then(|n| n.attribute("name")).unwrap_or_default().into();
    (el, a * d - b * c < 0., (a * c + b * d).abs() > 1e-9 || xfrm.is_none())
}

pub fn placeholder(importer: &mut Importer<'_>, mut el: Element, slide: usize, feature: &str, message: &str) -> Element {
    let feature: String = feature.chars().take(256).collect();
    el.style = BoxStyle {
        fill: "#fff4e5".into(),
        border_color: "#b45309".into(),
        border_width: 2.,
        padding: 12.,
        ..BoxStyle::default()
    };
    el.content = Content::Text(TextContent {
        text: RichText {
            html: format!("<p><b>Unsupported PowerPoint content</b></p><p>{}</p>", crate::util::esc(&feature)),
            style: TextStyle {
                font_size: 18.,
                color: "#7c2d12".into(),
                auto_height: false,
                ..TextStyle::default()
            },
            ..RichText::default()
        },
    });
    importer.issue(slide, Some(&el.id), &feature, "unsupported-content", IssueDisposition::Placeholder, message);
    el
}

fn preset(value: &str) -> Option<&'static str> {
    Some(match value {
        "rect" => "rect",
        "roundRect" => "rounded",
        "ellipse" => "ellipse",
        "triangle" => "triangle",
        "rtTriangle" => "right-triangle",
        "diamond" => "diamond",
        "pentagon" => "pentagon",
        "hexagon" => "hexagon",
        "octagon" => "octagon",
        "star5" => "star",
        "star12" => "burst",
        "rightArrow" => "arrow",
        "leftRightArrow" => "double-arrow",
        "chevron" => "chevron",
        "parallelogram" => "parallelogram",
        "trapezoid" => "trapezoid",
        "plus" => "plus",
        "heart" => "heart",
        "wedgeRoundRectCallout" => "bubble",
        "cloud" => "cloud",
        "line" => "line",
        _ => return None,
    })
}

fn style(el: &mut Element, sources: &[Node<'_, '_>], theme: &Theme, scale: f64) {
    let props: Vec<_> = sources.iter().filter_map(|n| child(*n, "spPr")).collect();
    el.style.fill = props
        .iter()
        .find_map(|n| text::fill(*n, theme))
        .or_else(|| {
            sources.iter().find_map(|n| {
                child(*n, "style")
                    .and_then(|n| child(n, "fillRef"))
                    .and_then(|n| n.children().find_map(|n| text::color(n, theme)))
            })
        })
        .unwrap_or_else(|| "transparent".into());
    if let Some(line) = props.iter().find_map(|n| child(*n, "ln")) {
        el.style.border_color = text::fill(line, theme).unwrap_or_else(|| "#000000".into());
        el.style.border_width = if child(line, "noFill").is_some() {
            0.
        } else {
            num(line, "w", 12700.) / EMU * scale
        };
        el.style.border_style = match child(line, "prstDash").and_then(|n| n.attribute("val")) {
            Some("dash" | "lgDash") => "dashed",
            Some("dot" | "sysDot") => "dotted",
            _ => "solid",
        }
        .into();
    }
}

fn ph<'a, 'i>(n: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    desc(n, "ph")
}
fn ph_kind<'a, 'i>(n: Node<'a, 'i>) -> &'a str {
    match n.attribute("type").unwrap_or("body") {
        "ctrTitle" => "title",
        kind => kind,
    }
}
fn match_placeholder<'a, 'i>(n: Node<'_, '_>, tree: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    let key = ph(n)?;
    let idx = key.attribute("idx").unwrap_or("0");
    let kind = ph_kind(key);
    tree.children()
        .find(|s| ph(*s).is_some_and(|p| p.attribute("idx").unwrap_or("0") == idx && ph_kind(p) == kind))
        .or_else(|| tree.children().find(|s| ph(*s).is_some_and(|p| ph_kind(p) == kind)))
}

fn validate_geometry(el: &Element) -> anyhow::Result<()> {
    anyhow::ensure!(
        [el.x, el.y, el.w, el.h, el.rotation].iter().all(|v| v.is_finite()) && el.x.abs() <= 20000. && el.y.abs() <= 20000. && el.w <= 20000. && el.h <= 20000.,
        n_("PowerPoint object geometry is outside the supported range")
    );
    Ok(())
}

/// Resolve and flatten a drawing layer. Only geometry that cannot be expressed
/// by rotation/scale is approximated, with an issue associated with the object.
#[allow(clippy::too_many_arguments)]
pub fn tree(
    importer: &mut Importer<'_>,
    root: Node<'_, '_>,
    inherited: &[Node<'_, '_>],
    defaults: Option<Node<'_, '_>>,
    part: &str,
    theme: &Theme,
    slide: usize,
    parent: Matrix,
    artwork: bool,
    out: &mut Vec<Element>,
) -> anyhow::Result<()> {
    importer.budget.array::<Node<'_, '_>>(root.children().filter(Node::is_element).count())?;
    draw_nodes(
        importer,
        &root.children().filter(Node::is_element).collect::<Vec<_>>(),
        inherited,
        defaults,
        part,
        theme,
        slide,
        parent,
        artwork,
        out,
    )
}

#[allow(clippy::too_many_arguments)]
fn draw_nodes(
    importer: &mut Importer<'_>,
    nodes: &[Node<'_, '_>],
    inherited: &[Node<'_, '_>],
    defaults: Option<Node<'_, '_>>,
    part: &str,
    theme: &Theme,
    slide: usize,
    parent: Matrix,
    artwork: bool,
    out: &mut Vec<Element>,
) -> anyhow::Result<()> {
    // Nested groups keep parent relationship vectors alive. Count those copies
    // before parsing, including targets shared by many inherited objects.
    importer.budget.charge(importer.archive.relationships_bytes(part).saturating_mul(2))?;
    let rels = importer.archive.rels(part)?;
    for &n in nodes {
        let name = n.tag_name().name();
        if matches!(name, "nvGrpSpPr" | "grpSpPr" | "extLst") || (artwork && ph(n).is_some()) {
            continue;
        }
        anyhow::ensure!(out.len() < 5000, n_("PowerPoint slide has too many objects"));
        // Charge the object/vector slot and its source attributes before any
        // content is copied. Group/master reuse is charged on every traversal.
        importer.budget.array::<Element>(2)?;
        importer.budget.charge(n.range().len())?;
        if named(n, "grpSp") {
            let x = child(n, "grpSpPr").and_then(|n| child(n, "xfrm"));
            let (ox, oy) = pair(x.and_then(|n| child(n, "off")), "x", "y", (0., 0.));
            let (ex, ey) = pair(x.and_then(|n| child(n, "ext")), "cx", "cy", (1., 1.));
            let (cx, cy) = pair(x.and_then(|n| child(n, "chOff")), "x", "y", (0., 0.));
            let (cw, ch) = pair(x.and_then(|n| child(n, "chExt")), "cx", "cy", (ex, ey));
            anyhow::ensure!(cw > 0. && ch > 0., n_("Invalid PowerPoint group geometry"));
            let angle = x.map(|n| num(n, "rot", 0.) / 60000.).unwrap_or(0.);
            let flip = |k| {
                if x.is_some_and(|n| matches!(n.attribute(k), Some("1" | "true"))) {
                    -1.
                } else {
                    1.
                }
            };
            let matrix = parent
                .then(Matrix::shift(ox + ex / 2., oy + ey / 2.))
                .then(Matrix::rotate(angle))
                .then(Matrix::scale(flip("flipH"), flip("flipV")))
                .then(Matrix::shift(-ex / 2., -ey / 2.))
                .then(Matrix::scale(ex / cw, ey / ch))
                .then(Matrix::shift(-cx, -cy));
            tree(importer, n, &[], defaults, part, theme, slide, matrix, false, out)?;
            importer.issue(
                slide,
                None,
                "groups",
                "flattened-group",
                IssueDisposition::Approximated,
                n_("Group geometry was flattened into editable objects; group membership is not retained."),
            );
            continue;
        }
        if named(n, "AlternateContent") {
            let fallback = child(n, "Fallback").or_else(|| child(n, "Choice").filter(|n| n.children().any(|c| named(c, "pic"))));
            if let Some(fallback) = fallback {
                tree(importer, fallback, inherited, defaults, part, theme, slide, parent, artwork, out)?;
                importer.issue(
                    slide,
                    out.last().map(|e| e.id.as_str()),
                    n_("alternate content"),
                    "alternate-representation",
                    IssueDisposition::Approximated,
                    n_("The available alternate representation was imported."),
                );
            } else {
                out.push(placeholder(
                    importer,
                    Element {
                        w: 360.,
                        h: 120.,
                        x: 40.,
                        y: 40.,
                        ..Element::default()
                    },
                    slide,
                    n_("alternate content"),
                    n_("No supported fallback was present in the file."),
                ));
            }
            continue;
        }
        let inherited_shapes: Vec<_> = inherited.iter().filter_map(|t| match_placeholder(n, *t)).collect();
        for source in &inherited_shapes {
            importer.budget.charge(source.range().len())?;
        }
        let (mut el, flipped, approx) = geometry(n, &inherited_shapes, parent);
        validate_geometry(&el)?;
        if approx {
            importer.issue(
                slide,
                Some(&el.id),
                "geometry",
                "approximated-geometry",
                IssueDisposition::Approximated,
                n_("Missing geometry or a skewed transform was approximated."),
            );
        }
        let sources: Vec<_> = std::iter::once(n).chain(inherited_shapes.iter().copied()).collect();
        let scale = parent.font_scale();
        style(&mut el, &sources, theme, scale);
        if named(n, "sp") || named(n, "cxnSp") {
            if n.descendants().any(|v| v.is_element() && matches!(v.tag_name().name(), "oMath" | "oMathPara")) {
                out.push(placeholder(
                    importer,
                    el,
                    slide,
                    n_("Office equation"),
                    n_("OMML equations have no supported editable or visual fallback."),
                ));
                continue;
            }
            let geom = sources
                .iter()
                .find_map(|s| child(*s, "spPr").and_then(|s| child(s, "prstGeom")))
                .and_then(|n| n.attribute("prst"))
                .or_else(|| named(n, "cxnSp").then_some("line"));
            let custom = sources.iter().any(|s| child(*s, "spPr").and_then(|s| child(s, "custGeom")).is_some());
            let rich = text::rich_text(n, &inherited_shapes, defaults, theme, scale, &rels, &mut importer.budget)?;
            if custom || geom.is_some_and(|g| preset(g).is_none()) {
                out.push(placeholder(
                    importer,
                    el,
                    slide,
                    geom.unwrap_or(n_("custom geometry")),
                    n_("The shape geometry cannot be represented faithfully."),
                ));
                continue;
            }
            let is_text = desc(n, "cNvSpPr").is_some_and(|p| matches!(p.attribute("txBox"), Some("1" | "true"))) || geom.is_none();
            if child(n, "txBody").is_some() {
                let props: Vec<_> = sources.iter().filter_map(|s| child(*s, "txBody").and_then(|b| child(b, "bodyPr"))).collect();
                let insets = [("lIns", 91440.), ("rIns", 91440.), ("tIns", 45720.), ("bIns", 45720.)].map(|(name, default)| {
                    props
                        .iter()
                        .find_map(|n| n.attribute(name))
                        .and_then(|v| v.parse::<f64>().ok())
                        .filter(|v| v.is_finite())
                        .unwrap_or(default)
                        / EMU
                        * scale
                });
                el.style.padding = insets.iter().copied().fold(f64::INFINITY, f64::min).clamp(0., 200.);
                if insets.iter().any(|v| (*v - el.style.padding).abs() > 0.01) {
                    importer.issue(
                        slide,
                        Some(&el.id),
                        n_("text frame insets"),
                        "text-insets",
                        IssueDisposition::Approximated,
                        n_("Asymmetric text frame insets were approximated using a uniform inset."),
                    );
                }
            }
            if is_text {
                el.content = Content::Text(TextContent { text: rich });
            } else {
                let cap = |name| {
                    sources
                        .iter()
                        .find_map(|s| child(*s, "spPr").and_then(|p| child(p, "ln")).and_then(|l| child(l, name)))
                        .and_then(|n| n.attribute("type"))
                        .map(|v| match v {
                            "triangle" | "arrow" | "stealth" => "arrow",
                            "oval" => "dot",
                            _ => "none",
                        })
                        .unwrap_or("none")
                        .into()
                };
                el.content = Content::Shape(ShapeContent {
                    shape: preset(geom.unwrap_or("rect")).unwrap_or("rect").into(),
                    start_cap: cap("headEnd"),
                    end_cap: cap("tailEnd"),
                    text: rich,
                });
                if geom == Some("line") && transform(n).and_then(|n| child(n, "ext")).is_some_and(|n| num(n, "cy", 0.) > 0.) {
                    let center = (el.x + el.w / 2., el.y + el.h / 2.);
                    let width = el.w.hypot(el.h);
                    el.rotation += el.h.atan2(el.w).to_degrees();
                    el.w = width;
                    el.h = el.style.border_width.max(4.);
                    el.x = center.0 - el.w / 2.;
                    el.y = center.1 - el.h / 2.;
                }
            }
            if flipped {
                importer.issue(
                    slide,
                    Some(&el.id),
                    n_("mirrored text/shape"),
                    "unsupported-mirror",
                    IssueDisposition::Approximated,
                    n_("Mirroring of text or vector shapes is not retained."),
                );
            }
            if n.descendants()
                .any(|p| matches!(p.tag_name().name(), "effectLst" | "prstTxWarp" | "gradFill" | "pattFill"))
            {
                importer.issue(
                    slide,
                    Some(&el.id),
                    n_("drawing effects"),
                    "drawing-effect-loss",
                    IssueDisposition::Approximated,
                    n_("Some fill, text warp or drawing effects are not supported."),
                );
            }
            if n.descendants().any(|v| {
                v.is_element()
                    && (matches!(v.tag_name().name(), "lnSpc" | "spcBef" | "spcAft" | "tabLst")
                        || v.attribute("lvl").is_some_and(|v| v != "0")
                        || v.attribute("vert").is_some_and(|v| v != "horz"))
            }) {
                importer.issue(
                    slide,
                    Some(&el.id),
                    n_("text layout"),
                    "text-layout",
                    IssueDisposition::Approximated,
                    n_("Paragraph spacing, nested list indentation, tabs or vertical text are approximated by Cadenza's text layout."),
                );
            }
        } else if named(n, "pic") {
            let blip = desc(n, "blip");
            let relation = blip
                .and_then(|b| rel_attr(b, "embed").or_else(|| rel_attr(b, "link")))
                .and_then(|id| rels.iter().find(|r| r.id == id));
            let Some(relation) = relation.filter(|r| !r.external) else {
                out.push(placeholder(
                    importer,
                    el,
                    slide,
                    n_("external or missing image"),
                    n_("External images are not fetched; no embedded image was available."),
                ));
                continue;
            };
            let Some((width, height)) = importer.image(&relation.target)? else {
                out.push(placeholder(
                    importer,
                    el,
                    slide,
                    n_("image format"),
                    n_("The embedded image format is unsupported or unsafe."),
                ));
                continue;
            };
            let crop = desc(n, "srcRect").and_then(|c| {
                let x = num(c, "l", 0.) / 100000.;
                let y = num(c, "t", 0.) / 100000.;
                let crop = CropRect {
                    x,
                    y,
                    w: 1. - x - num(c, "r", 0.) / 100000.,
                    h: 1. - y - num(c, "b", 0.) / 100000.,
                };
                crop.valid().then_some(crop)
            });
            if desc(n, "srcRect").is_some() && crop.is_none() {
                importer.issue(
                    slide,
                    Some(&el.id),
                    n_("image crop"),
                    "unsupported-crop",
                    IssueDisposition::Approximated,
                    n_("An out-of-bounds image crop was not retained."),
                );
            }
            el.opacity = blip
                .and_then(|b| child(b, "alphaModFix"))
                .map(|n| num(n, "amt", 100000.) / 100000.)
                .unwrap_or(1.)
                .clamp(0., 1.);
            el.content = Content::Image(ImageContent {
                src: relation.target.clone(),
                alt: desc(n, "cNvPr").and_then(|n| n.attribute("descr")).unwrap_or_default().into(),
                fit: "fill".into(),
                natural_width: width,
                natural_height: height,
                crop,
                flip_y: flipped,
                ..ImageContent::default()
            });
            el.style.fill = "transparent".into();
        } else if named(n, "graphicFrame") {
            if let Some(table) = desc(n, "tbl") {
                parse_table(importer, &mut el, table, theme, slide, scale, &rels)?;
            } else if let Some(chart) = desc(n, "chart") {
                let rel = rel_attr(chart, "id").and_then(|id| rels.iter().find(|r| r.id == id && !r.external));
                let content = match rel {
                    Some(rel) => {
                        // Repeated chart references must spend the same shared
                        // budget before reparsing or copying the cache again.
                        if let Some(bytes) = importer.archive.parts.get(&rel.target) {
                            importer.budget.charge(bytes.len())?;
                        }
                        charts::parse(importer.archive.xml(&rel.target)?.root_element(), theme, &mut importer.budget)?
                    }
                    None => None,
                };
                if let Some(content) = content {
                    el.content = Content::Chart(content);
                    importer.issue(
                        slide,
                        Some(&el.id),
                        n_("chart appearance"),
                        "chart-appearance",
                        IssueDisposition::Approximated,
                        n_("Cached chart data is editable; unsupported axis, label and theme formatting may differ."),
                    );
                } else {
                    el = placeholder(
                        importer,
                        el,
                        slide,
                        "chart",
                        n_("The chart type/configuration or its cached data is unsupported; workbook formulas are not evaluated."),
                    );
                }
            } else if let Some(preview) = desc(n, "pic") {
                draw_nodes(importer, &[preview], &[], defaults, part, theme, slide, parent, false, out)?;
                importer.issue(
                    slide,
                    out.last().map(|e| e.id.as_str()),
                    n_("embedded object"),
                    "embedded-preview",
                    IssueDisposition::Rasterized,
                    n_("The embedded object's supplied picture preview was retained; the object itself is not editable."),
                );
                continue;
            } else {
                el = placeholder(
                    importer,
                    el,
                    slide,
                    n_("SmartArt or embedded object"),
                    n_("No supported editable representation or embedded preview was available."),
                );
            }
        } else {
            el = placeholder(importer, el, slide, name, n_("This PowerPoint drawing object is not supported."));
        }
        if let Some(link) = desc(n, "cNvPr")
            .and_then(|n| child(n, "hlinkClick"))
            .and_then(|n| rel_attr(n, "id"))
            .and_then(|id| rels.iter().find(|r| r.id == id))
        {
            if link.external && ["http://", "https://", "mailto:"].iter().any(|s| link.target.starts_with(s)) {
                importer.budget.charge(link.target.len())?;
                el.action.kind = "url".into();
                el.action.url = link.target.clone();
            } else if let Some(id) = importer.slide_ids.get(&link.target) {
                el.action.kind = "slide".into();
                el.action.slide = id.clone();
            }
        }
        validate_geometry(&el)?;
        crate::sanitize::element(&mut el);
        out.push(el);
    }
    Ok(())
}

fn parse_table(
    importer: &mut Importer<'_>,
    el: &mut Element,
    table: Node<'_, '_>,
    theme: &Theme,
    slide: usize,
    scale: f64,
    rels: &[super::archive::Rel],
) -> anyhow::Result<()> {
    if table.descendants().any(|n| {
        n.is_element()
            && ["rowSpan", "gridSpan", "hMerge", "vMerge"].iter().any(|k| {
                n.attribute(*k)
                    .is_some_and(|v| v != "1" && v != "0" || (*k == "hMerge" || *k == "vMerge") && v == "1")
            })
    }) {
        *el = placeholder(
            importer,
            el.clone(),
            slide,
            n_("merged table"),
            n_("Merged cells cannot be represented by a simple editable grid."),
        );
        return Ok(());
    }
    let mut rows = vec![];
    let mut fills = vec![];
    let mut contrast_adjusted = false;
    for row in table.children().filter(|n| named(*n, "tr")) {
        let count = row.children().filter(|n| named(*n, "tc")).count();
        anyhow::ensure!(rows.len() < 500 && count <= 200, n_("PowerPoint table is too large"));
        importer.budget.array::<String>(count)?;
        importer.budget.array::<Option<String>>(count)?;
        importer.budget.array::<Node<'_, '_>>(count)?;
        importer.budget.array::<Vec<String>>(2)?;
        let cells: Vec<_> = row.children().filter(|n| named(*n, "tc")).collect();
        let colors: Vec<_> = cells.iter().map(|cell| child(*cell, "tcPr").and_then(|p| text::fill(p, theme))).collect();
        rows.push(
            cells
                .iter()
                .zip(&colors)
                .map(|(cell, fill)| -> anyhow::Result<String> {
                    let html = text::rich_text(*cell, &[], None, theme, scale, rels, &mut importer.budget)?.html;
                    if fill.is_none() {
                        importer.budget.charge(html.len())?;
                        let (html, changed) = text::legible_on_white(&html);
                        contrast_adjusted |= changed;
                        Ok(html)
                    } else {
                        Ok(html)
                    }
                })
                .collect::<anyhow::Result<Vec<_>>>()?,
        );
        fills.push(colors);
    }
    if rows.is_empty() {
        *el = placeholder(importer, el.clone(), slide, n_("empty table"), n_("The table contains no usable cells."));
        return Ok(());
    }
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    for row in &mut rows {
        importer.budget.array::<String>(width - row.len())?;
        row.resize(width, String::new());
    }
    let header = child(table, "tblPr").is_some_and(|n| n.attribute("firstRow") == Some("1"));
    if contrast_adjusted {
        importer.issue(
            slide,
            Some(&el.id),
            n_("table text contrast"),
            "table-text-contrast",
            IssueDisposition::Approximated,
            n_("An unresolved cell background was replaced by white. Light text colors were darkened to keep the table readable."),
        );
    }
    let uniform = |values: &[Option<String>]| -> Option<String> {
        let first = values.first()?.as_ref()?;
        values.iter().all(|v| v.as_ref() == Some(first)).then(|| first.clone())
    };
    let body_fills: Vec<_> = fills.iter().skip(usize::from(header)).flatten().cloned().collect();
    let body_fill = uniform(&body_fills);
    let head_fill = header.then(|| uniform(&fills[0])).flatten();
    // A uniform body/header fits the native table model exactly. Mixed cells
    // retain their background behind each editable paragraph, so light text
    // never loses its associated fill even though native cell boxes are uniform.
    el.style.fill = body_fill.clone().unwrap_or_else(|| {
        if fills.iter().flatten().all(Option::is_some) {
            "transparent"
        } else {
            "#ffffff"
        }
        .into()
    });
    let mixed = body_fill.is_none() || (header && head_fill.is_none());
    if mixed {
        for (row, colors) in rows.iter_mut().zip(&fills) {
            for (html, fill) in row.iter_mut().zip(colors) {
                if let Some(fill) = fill {
                    importer.budget.charge(html.len().saturating_mul(2))?;
                    *html = html.replace(
                        "style=\"text-align:",
                        &format!("style=\"background-color:{};text-align:", crate::util::esc(fill)),
                    );
                }
            }
        }
        if fills.iter().flatten().any(Option::is_some) {
            importer.issue(
                slide,
                Some(&el.id),
                n_("cell backgrounds"),
                "table-cell-backgrounds",
                IssueDisposition::Approximated,
                n_("Mixed cell fills are retained behind their text; the full cell background and spacing may differ."),
            );
        }
    }
    el.content = Content::Table(TableContent {
        rows,
        header_row: header,
        header_col: false,
        striped: false,
        font_size: 24. * scale,
        color: "#000000".into(),
        header_fill: head_fill.or(body_fill).unwrap_or_else(|| "#ffffff".into()),
        header_color: "#000000".into(),
        grid_color: "#777777".into(),
        ..TableContent::default()
    });
    importer.issue(
        slide,
        Some(&el.id),
        n_("table layout"),
        "table-appearance",
        IssueDisposition::Approximated,
        n_("Cell text and supported fills remain editable; row heights, column widths and borders use Cadenza's grid formatting."),
    );
    Ok(())
}
