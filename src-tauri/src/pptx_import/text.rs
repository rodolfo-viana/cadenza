use super::MaterializationBudget;
use super::archive::{Rel, attr, child, desc, named, num, rel_attr};
use crate::model::{RichText, TextStyle};
use crate::util::esc;
use roxmltree::Node;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct Theme {
    pub colors: BTreeMap<String, String>,
    pub major: String,
    pub minor: String,
}
impl Default for Theme {
    fn default() -> Self {
        Self {
            colors: [
                ("dk1", "000000"),
                ("lt1", "FFFFFF"),
                ("dk2", "1F497D"),
                ("lt2", "EEECE1"),
                ("accent1", "4472C4"),
                ("accent2", "ED7D31"),
                ("accent3", "A5A5A5"),
                ("accent4", "FFC000"),
                ("accent5", "5B9BD5"),
                ("accent6", "70AD47"),
                ("hlink", "0563C1"),
                ("folHlink", "954F72"),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect(),
            major: "Arial".into(),
            minor: "Arial".into(),
        }
    }
}
impl Theme {
    pub fn from_xml(root: Node<'_, '_>) -> Self {
        let mut t = Self::default();
        if let Some(scheme) = desc(root, "clrScheme") {
            for n in scheme.children().filter(Node::is_element) {
                if let Some(color) = n.children().find(Node::is_element)
                    && let Some(v) = attr(color, "val").filter(|s| s.len() == 6).or_else(|| attr(color, "lastClr"))
                {
                    t.colors.insert(n.tag_name().name().into(), v.into());
                }
            }
        }
        if let Some(n) = desc(root, "majorFont").and_then(|n| child(n, "latin")) {
            t.major = attr(n, "typeface").unwrap_or("Arial").into();
        }
        if let Some(n) = desc(root, "minorFont").and_then(|n| child(n, "latin")) {
            t.minor = attr(n, "typeface").unwrap_or("Arial").into();
        }
        t
    }
    pub fn font<'a>(&'a self, face: &'a str) -> &'a str {
        match face {
            "+mj-lt" | "+mj-ea" | "+mj-cs" => &self.major,
            "+mn-lt" | "+mn-ea" | "+mn-cs" => &self.minor,
            "" => &self.minor,
            x => x,
        }
    }
}

pub fn color(n: Node<'_, '_>, theme: &Theme) -> Option<String> {
    let value = match n.tag_name().name() {
        "srgbClr" => attr(n, "val")?.to_owned(),
        "sysClr" => attr(n, "lastClr")?.to_owned(),
        "schemeClr" => {
            let key = match attr(n, "val")? {
                "tx1" => "dk1",
                "tx2" => "dk2",
                "bg1" => "lt1",
                "bg2" => "lt2",
                x => x,
            };
            theme.colors.get(attr(n, "val")?).or_else(|| theme.colors.get(key))?.clone()
        }
        _ => return None,
    };
    if value.len() != 6 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut rgb = [0., 0., 0.];
    for (i, c) in rgb.iter_mut().enumerate() {
        *c = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16).ok()? as f64;
    }
    let mut alpha = 1.;
    for effect in n.children().filter(Node::is_element) {
        let v = num(effect, "val", 100000.) / 100000.;
        match effect.tag_name().name() {
            "tint" => rgb.iter_mut().for_each(|c| *c += (255. - *c) * v),
            "shade" => rgb.iter_mut().for_each(|c| *c *= v),
            "lumMod" => luminance(&mut rgb, v, 0.),
            "lumOff" => luminance(&mut rgb, 1., v),
            "alpha" => alpha = v,
            _ => {}
        }
    }
    let [r, g, b] = rgb.map(|v| v.round().clamp(0., 255.) as u8);
    Some(if alpha < 1. {
        format!("rgba({r},{g},{b},{})", alpha.clamp(0., 1.))
    } else {
        format!("#{r:02x}{g:02x}{b:02x}")
    })
}

/// DrawingML luminance transforms operate in HSL, retaining hue/saturation.
fn luminance(rgb: &mut [f64; 3], factor: f64, offset: f64) {
    let lo = rgb.iter().copied().fold(f64::INFINITY, f64::min) / 255.;
    let hi = rgb.iter().copied().fold(f64::NEG_INFINITY, f64::max) / 255.;
    let light = (hi + lo) / 2.;
    let delta = hi - lo;
    let saturation = if delta == 0. { 0. } else { delta / (1. - (2. * light - 1.).abs()) };
    let next = (light * factor + offset).clamp(0., 1.);
    let chroma = (1. - (2. * next - 1.).abs()) * saturation;
    let ratio = if delta == 0. { 0. } else { chroma / delta };
    for c in rgb {
        *c = 255. * ((*c / 255. - lo) * ratio + next - chroma / 2.);
    }
}

pub fn fill(n: Node<'_, '_>, theme: &Theme) -> Option<String> {
    if child(n, "noFill").is_some() {
        return Some("transparent".into());
    }
    child(n, "solidFill").and_then(|f| f.children().find_map(|n| color(n, theme)))
}

fn inherited<'a, 'i>(sources: &[Node<'a, 'i>], key: &str) -> Option<&'a str> {
    sources.iter().find_map(|s| attr(*s, key))
}
fn flags(sources: &[Node<'_, '_>], key: &str) -> bool {
    matches!(inherited(sources, key), Some("1" | "true"))
}

/// Used only after substituting white for an unresolved table background.
/// Preserve dark colors; a reported approximation keeps light runs readable.
pub fn legible_on_white(html: &str) -> (String, bool) {
    let mut out = String::new();
    let mut remaining = html;
    let mut changed = false;
    while let Some((before, tail)) = remaining.split_once("style=\"") {
        out.push_str(before);
        out.push_str("style=\"");
        let Some((css, after)) = tail.split_once('"') else {
            out.push_str(tail);
            return (out, changed);
        };
        let mut parts = css.split("color:#");
        out.push_str(parts.next().unwrap_or_default());
        for part in parts {
            let color = part.get(..6).filter(|s| s.chars().all(|c| c.is_ascii_hexdigit()));
            let replace = color.is_some_and(|color| {
                let channel = |offset| {
                    let value = u8::from_str_radix(&color[offset..offset + 2], 16).unwrap_or(0) as f64 / 255.;
                    if value <= 0.04045 {
                        value / 12.92
                    } else {
                        ((value + 0.055) / 1.055).powf(2.4)
                    }
                };
                let luminance = 0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4);
                1.05 / (luminance + 0.05) < 3.
            });
            out.push_str("color:#");
            if replace {
                out.push_str("000000");
                out.push_str(&part[6..]);
                changed = true;
            } else {
                out.push_str(part);
            }
        }
        out.push('"');
        remaining = after;
    }
    out.push_str(remaining);
    (out, changed)
}

/// Resolve effective run defaults, then serialize supported formatting to the
/// same sanitized rich-text surface as native editing. No source XML is used as HTML.
pub fn rich_text(
    shape: Node<'_, '_>,
    inherited_shapes: &[Node<'_, '_>],
    defaults: Option<Node<'_, '_>>,
    theme: &Theme,
    scale: f64,
    rels: &[Rel],
    budget: &mut MaterializationBudget,
) -> anyhow::Result<RichText> {
    let role = desc(shape, "ph").and_then(|n| attr(n, "type")).unwrap_or("body");
    let title = matches!(role, "title" | "ctrTitle");
    let defaults = defaults.and_then(|n| {
        if named(n, "txStyles") {
            child(
                n,
                if title {
                    "titleStyle"
                } else if matches!(role, "body" | "obj") {
                    "bodyStyle"
                } else {
                    "otherStyle"
                },
            )
        } else {
            Some(n)
        }
    });
    budget.array::<RichText>(1)?;
    budget.charge(if title { theme.major.len() } else { theme.minor.len() })?;
    let mut style = TextStyle {
        font_family: if title { theme.major.clone() } else { theme.minor.clone() },
        font_size: 24. * scale,
        color: "#000000".into(),
        align: "left".into(),
        auto_height: false,
        ..TextStyle::default()
    };
    let body = child(shape, "txBody");
    let Some(body) = body else {
        return Ok(RichText {
            html: String::new(),
            placeholder: String::new(),
            style,
        });
    };
    let bodies: Vec<_> = std::iter::once(body)
        .chain(inherited_shapes.iter().filter_map(|s| child(*s, "txBody")))
        .collect();
    let body_props: Vec<_> = bodies.iter().filter_map(|b| child(*b, "bodyPr")).collect();
    if !body_props.is_empty() {
        style.valign = match inherited(&body_props, "anchor").unwrap_or("t") {
            "ctr" => "middle",
            "b" => "bottom",
            _ => "top",
        }
        .into();
    }
    let mut html = String::new();
    for p in body.children().filter(|n| named(*n, "p")) {
        budget.charge(128)?;
        let ppr = child(p, "pPr");
        let level = ppr.map(|p| num(p, "lvl", 0.) as usize).unwrap_or(0).min(8);
        let tag = format!("lvl{}pPr", level + 1);
        let mut pstyles: Vec<_> = ppr
            .into_iter()
            .chain(bodies.iter().filter_map(|b| child(*b, "lstStyle").and_then(|s| child(s, &tag))))
            .collect();
        pstyles.extend(bodies.iter().skip(1).filter_map(|b| child(*b, "p").and_then(|p| child(p, "pPr"))));
        if let Some(d) = defaults.and_then(|d| child(d, &tag)) {
            pstyles.push(d);
        }
        let align = match inherited(&pstyles, "algn").unwrap_or("l") {
            "ctr" => "center",
            "r" => "right",
            "just" => "justify",
            _ => "left",
        };
        let mut runs_default: Vec<_> = pstyles.iter().filter_map(|p| child(*p, "defRPr")).collect();
        if let Some(end) = child(p, "endParaRPr") {
            runs_default.push(end);
        }
        let bullet = pstyles.iter().find_map(|p| child(*p, "buChar").and_then(|n| attr(n, "char")));
        let numbered = pstyles.iter().any(|p| child(*p, "buAutoNum").is_some());
        let no_bullet = pstyles.first().is_some_and(|p| child(*p, "buNone").is_some());
        let list = !no_bullet && (bullet.is_some() || numbered);
        if list {
            html.push_str(if numbered { "<ol>" } else { "<ul>" });
        }
        html.push_str(&format!("<{} style=\"text-align:{align}\">", if list { "li" } else { "p" }));
        for r in p.children().filter(Node::is_element) {
            if named(r, "br") {
                budget.charge(4)?;
                html.push_str("<br>");
                continue;
            }
            if !named(r, "r") && !named(r, "fld") {
                continue;
            }
            let sources: Vec<_> = child(r, "rPr").into_iter().chain(runs_default.iter().copied()).collect();
            let size = inherited(&sources, "sz")
                .and_then(|s| s.parse::<f64>().ok())
                .map(|v| v / 100. * 4. / 3. * scale)
                .unwrap_or(style.font_size)
                .clamp(1., 1600.);
            let face = sources
                .iter()
                .find_map(|s| child(*s, "latin").and_then(|n| attr(n, "typeface")))
                .map(|f| theme.font(f))
                .unwrap_or(&style.font_family);
            let ink = sources.iter().find_map(|s| fill(*s, theme)).unwrap_or_else(|| style.color.clone());
            let text = child(r, "t").and_then(|n| n.text()).unwrap_or_default();
            budget.charge(256)?;
            budget.escaped(face)?;
            budget.escaped(text)?;
            let mut css = format!("font-size:{size}px;font-family:{};color:{}", esc(face), esc(&ink));
            if flags(&sources, "b") {
                css.push_str(";font-weight:bold");
            }
            if flags(&sources, "i") {
                css.push_str(";font-style:italic");
            }
            if inherited(&sources, "u").is_some_and(|v| v != "none") {
                css.push_str(";text-decoration:underline");
            }
            if inherited(&sources, "strike").is_some_and(|v| v != "noStrike") {
                css.push_str(";text-decoration:line-through");
            }
            if let Some(baseline) = inherited(&sources, "baseline").and_then(|s| s.parse::<i32>().ok()) {
                css.push_str(if baseline > 0 {
                    ";vertical-align:super"
                } else if baseline < 0 {
                    ";vertical-align:sub"
                } else {
                    ""
                });
            }
            let link = sources
                .iter()
                .find_map(|s| child(*s, "hlinkClick"))
                .and_then(|n| rel_attr(n, "id"))
                .and_then(|id| rels.iter().find(|r| r.id == id && r.external))
                .filter(|r| ["https://", "http://", "mailto:"].iter().any(|s| r.target.starts_with(s)));
            if let Some(link) = link {
                budget.escaped(&link.target)?;
                html.push_str(&format!("<a href=\"{}\">", esc(&link.target)));
            }
            html.push_str(&format!("<span style=\"{css}\">{}</span>", esc(text)));
            if link.is_some() {
                html.push_str("</a>");
            }
        }
        html.push_str(if list { "</li>" } else { "</p>" });
        if list {
            html.push_str(if numbered { "</ol>" } else { "</ul>" });
        }
    }
    budget.charge(html.len())?; // sanitizer output can coexist with its input
    Ok(RichText {
        html: crate::sanitize::rich_text(&html),
        placeholder: String::new(),
        style,
    })
}
