//! Vector shapes rendered as inline SVG.
//!
//! Paths are generated for the element's actual pixel size (instead of being
//! stretched from a unit viewBox), so strokes keep a constant width and
//! rounded corners stay round no matter how a shape is resized.

use std::f64::consts::PI;
use std::fmt::Write as _;

use crate::model::BoxStyle;
use crate::util::{css_value, esc, num};

/// Every shape the editor offers, as `(id, label)`.
pub const SHAPES: &[(&str, &str)] = &[
    ("rect", "Rectangle"),
    ("rounded", "Rounded Rectangle"),
    ("ellipse", "Oval"),
    ("triangle", "Triangle"),
    ("right-triangle", "Right Triangle"),
    ("diamond", "Diamond"),
    ("pentagon", "Pentagon"),
    ("hexagon", "Hexagon"),
    ("octagon", "Octagon"),
    ("star", "Star"),
    ("burst", "Burst"),
    ("arrow", "Arrow"),
    ("double-arrow", "Double Arrow"),
    ("chevron", "Chevron"),
    ("parallelogram", "Parallelogram"),
    ("trapezoid", "Trapezoid"),
    ("plus", "Plus"),
    ("heart", "Heart"),
    ("bubble", "Speech Bubble"),
    ("cloud", "Cloud"),
    ("line", "Line"),
];

/// Shapes without an explicit fill use the theme's accent color.
pub const DEFAULT_FILL: &str = "var(--cz-accent, #6366f1)";
/// Lines without an explicit color use the theme's text color.
pub const DEFAULT_LINE: &str = "var(--cz-text, #94a3b8)";

/// Renders the shape as an `<svg>` element sized `w`×`h`.
pub fn shape_svg(shape: &str, w: f64, h: f64, style: &BoxStyle, start_cap: &str, end_cap: &str) -> String {
    let w = w.max(1.0);
    let h = h.max(1.0);
    let sw = style.border_width.max(0.0);
    let stroke_color = css_value(&style.border_color);
    let dash = dash_array(&style.border_style, sw);
    let mut svg = String::with_capacity(512);
    let _ = write!(
        svg,
        r#"<svg class="cz-shape-svg" xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" preserveAspectRatio="none" overflow="visible">"#,
        w = num(w),
        h = num(h)
    );

    if shape == "line" {
        let lw = sw.max(1.0);
        let color = if stroke_color.is_empty() { DEFAULT_LINE } else { stroke_color };
        line_svg(&mut svg, w, h, lw, color, &dash, start_cap, end_cap);
    } else {
        let fill = match css_value(&style.fill) {
            "" => DEFAULT_FILL,
            f => f,
        };
        let d = shape_path(shape, w, h, sw, style.radius);
        // Colors go through `style` so CSS variables (theme colors) work.
        let mut paint = format!("fill:{fill};");
        if sw > 0.0 && !stroke_color.is_empty() {
            let _ = write!(paint, "stroke:{stroke_color};");
        }
        let _ = write!(svg, r#"<path d="{}" style="{}""#, d, esc(&paint));
        if sw > 0.0 && !stroke_color.is_empty() {
            let _ = write!(svg, r#" stroke-width="{}" stroke-linejoin="round"{}"#, num(sw), dash);
        }
        svg.push_str("/>");
    }
    svg.push_str("</svg>");
    svg
}

fn dash_array(style: &str, sw: f64) -> String {
    let unit = sw.max(1.0);
    match style {
        "dashed" => format!(r#" stroke-dasharray="{} {}""#, num(unit * 3.0), num(unit * 2.0)),
        "dotted" => format!(r#" stroke-dasharray="0 {}" stroke-linecap="round""#, num(unit * 2.0)),
        _ => String::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn line_svg(svg: &mut String, w: f64, h: f64, lw: f64, color: &str, dash: &str, start: &str, end: &str) {
    let cy = h / 2.0;
    let head = (lw * 3.5).max(12.0).min(w / 2.0);
    let x1 = if start == "arrow" { head * 0.8 } else { 0.0 };
    let x2 = if end == "arrow" { w - head * 0.8 } else { w };
    let linecap = if dash.contains("round") { "" } else { r#" stroke-linecap="round""# };
    let _ = write!(
        svg,
        r#"<path d="M{} {}H{}" style="stroke:{}" stroke-width="{}" fill="none"{}{}/>"#,
        num(x1),
        num(cy),
        num(x2),
        esc(color),
        num(lw),
        linecap,
        dash
    );
    let cap = |svg: &mut String, kind: &str, tip: f64, dir: f64| match kind {
        "arrow" => {
            let back = tip - dir * head;
            let _ = write!(
                svg,
                r#"<path d="M{} {}L{} {}L{} {}Z" style="fill:{}"/>"#,
                num(tip),
                num(cy),
                num(back),
                num(cy - head * 0.55),
                num(back),
                num(cy + head * 0.55),
                esc(color)
            );
        }
        "dot" => {
            let r = (lw * 1.8).max(5.0);
            let _ = write!(
                svg,
                r#"<circle cx="{}" cy="{}" r="{}" style="fill:{}"/>"#,
                num(tip - dir * r),
                num(cy),
                num(r),
                esc(color)
            );
        }
        _ => {}
    };
    cap(svg, start, 0.0, -1.0);
    cap(svg, end, w, 1.0);
}

/// Builds the SVG path data for a closed shape inset by half the stroke so
/// the outline is never clipped.
pub fn shape_path(shape: &str, w: f64, h: f64, stroke: f64, radius: f64) -> String {
    let i = stroke / 2.0;
    let (x0, y0, x1, y1) = (i, i, (w - i).max(i + 1.0), (h - i).max(i + 1.0));
    let (bw, bh) = (x1 - x0, y1 - y0);
    let (cx, cy) = (x0 + bw / 2.0, y0 + bh / 2.0);
    let poly = |pts: &[(f64, f64)]| -> String {
        let mut d = String::new();
        for (k, (x, y)) in pts.iter().enumerate() {
            let _ = write!(d, "{}{} {}", if k == 0 { "M" } else { "L" }, num(*x), num(*y));
        }
        d.push('Z');
        d
    };
    let regular = |n: usize, rot: f64| -> String {
        let pts: Vec<(f64, f64)> = (0..n)
            .map(|k| {
                let a = rot + 2.0 * PI * k as f64 / n as f64;
                (cx + bw / 2.0 * a.cos(), cy + bh / 2.0 * a.sin())
            })
            .collect();
        poly(&pts)
    };
    match shape {
        "rounded" => rounded_rect(x0, y0, bw, bh, if radius > 0.0 { radius } else { bw.min(bh) * 0.18 }),
        "ellipse" => {
            let (rx, ry) = (bw / 2.0, bh / 2.0);
            format!(
                "M{} {}A{} {} 0 1 0 {} {}A{} {} 0 1 0 {} {}Z",
                num(x0),
                num(cy),
                num(rx),
                num(ry),
                num(x1),
                num(cy),
                num(rx),
                num(ry),
                num(x0),
                num(cy)
            )
        }
        "triangle" => poly(&[(cx, y0), (x1, y1), (x0, y1)]),
        "right-triangle" => poly(&[(x0, y0), (x1, y1), (x0, y1)]),
        "diamond" => poly(&[(cx, y0), (x1, cy), (cx, y1), (x0, cy)]),
        "pentagon" => regular(5, -PI / 2.0),
        "hexagon" => poly(&[
            (x0 + bw * 0.25, y0),
            (x0 + bw * 0.75, y0),
            (x1, cy),
            (x0 + bw * 0.75, y1),
            (x0 + bw * 0.25, y1),
            (x0, cy),
        ]),
        "octagon" => {
            let (dx, dy) = (bw * 0.293, bh * 0.293);
            poly(&[
                (x0 + dx, y0),
                (x1 - dx, y0),
                (x1, y0 + dy),
                (x1, y1 - dy),
                (x1 - dx, y1),
                (x0 + dx, y1),
                (x0, y1 - dy),
                (x0, y0 + dy),
            ])
        }
        "star" => star(cx, cy, bw / 2.0, bh / 2.0, 5, 0.42),
        "burst" => star(cx, cy, bw / 2.0, bh / 2.0, 12, 0.72),
        "arrow" => {
            let head = (bw * 0.4).min(bh * 1.2);
            let shaft = bh * 0.25;
            poly(&[
                (x0, cy - shaft),
                (x1 - head, cy - shaft),
                (x1 - head, y0),
                (x1, cy),
                (x1 - head, y1),
                (x1 - head, cy + shaft),
                (x0, cy + shaft),
            ])
        }
        "double-arrow" => {
            let head = (bw * 0.3).min(bh * 1.2);
            let shaft = bh * 0.25;
            poly(&[
                (x0, cy),
                (x0 + head, y0),
                (x0 + head, cy - shaft),
                (x1 - head, cy - shaft),
                (x1 - head, y0),
                (x1, cy),
                (x1 - head, y1),
                (x1 - head, cy + shaft),
                (x0 + head, cy + shaft),
                (x0 + head, y1),
            ])
        }
        "chevron" => {
            let d = (bw * 0.35).min(bh);
            poly(&[(x0, y0), (x1 - d, y0), (x1, cy), (x1 - d, y1), (x0, y1), (x0 + d, cy)])
        }
        "parallelogram" => {
            let d = bw * 0.22;
            poly(&[(x0 + d, y0), (x1, y0), (x1 - d, y1), (x0, y1)])
        }
        "trapezoid" => {
            let d = bw * 0.2;
            poly(&[(x0 + d, y0), (x1 - d, y0), (x1, y1), (x0, y1)])
        }
        "plus" => {
            let (tx, ty) = (bw / 3.0, bh / 3.0);
            poly(&[
                (x0 + tx, y0),
                (x1 - tx, y0),
                (x1 - tx, y0 + ty),
                (x1, y0 + ty),
                (x1, y1 - ty),
                (x1 - tx, y1 - ty),
                (x1 - tx, y1),
                (x0 + tx, y1),
                (x0 + tx, y1 - ty),
                (x0, y1 - ty),
                (x0, y0 + ty),
                (x0 + tx, y0 + ty),
            ])
        }
        "heart" => {
            let p = |fx: f64, fy: f64| format!("{} {}", num(x0 + bw * fx), num(y0 + bh * fy));
            format!(
                "M{}C{} {} {}C{} {} {}C{} {} {}C{} {} {}Z",
                p(0.5, 0.28),
                p(0.5, 0.05),
                p(0.0, 0.0),
                p(0.0, 0.32),
                p(0.0, 0.62),
                p(0.35, 0.78),
                p(0.5, 1.0),
                p(0.65, 0.78),
                p(1.0, 0.62),
                p(1.0, 0.32),
                p(1.0, 0.0),
                p(0.5, 0.05),
                p(0.5, 0.28)
            )
        }
        "bubble" => {
            let body = bh * 0.78;
            let r = (bw.min(body) * 0.16).max(2.0);
            let by = y0 + body;
            format!(
                "M{} {}H{}Q{} {} {} {}V{}Q{} {} {} {}H{}L{} {}L{} {}H{}Q{} {} {} {}V{}Q{} {} {} {}Z",
                num(x0 + r),
                num(y0),
                num(x1 - r),
                num(x1),
                num(y0),
                num(x1),
                num(y0 + r),
                num(by - r),
                num(x1),
                num(by),
                num(x1 - r),
                num(by),
                num(x0 + bw * 0.42),
                num(x0 + bw * 0.22),
                num(y1),
                num(x0 + bw * 0.26),
                num(by),
                num(x0 + r),
                num(x0),
                num(by),
                num(x0),
                num(by - r),
                num(y0 + r),
                num(x0),
                num(y0),
                num(x0 + r),
                num(y0)
            )
        }
        "cloud" => {
            let p = |fx: f64, fy: f64| format!("{} {}", num(x0 + bw * fx), num(y0 + bh * fy));
            let a = |rx: f64, ry: f64, fx: f64, fy: f64| format!("A{} {} 0 0 1 {}", num(bw * rx), num(bh * ry), p(fx, fy));
            format!(
                "M{}{}{}{}{}{}{}Z",
                p(0.2, 0.92),
                a(0.2, 0.22, 0.12, 0.5),
                a(0.2, 0.24, 0.36, 0.2),
                a(0.2, 0.26, 0.72, 0.18),
                a(0.18, 0.22, 0.9, 0.48),
                a(0.16, 0.22, 0.8, 0.92),
                format_args!("L{}", p(0.2, 0.92)),
            )
        }
        _ => rounded_rect(x0, y0, bw, bh, radius),
    }
}

fn rounded_rect(x: f64, y: f64, w: f64, h: f64, r: f64) -> String {
    let r = r.max(0.0).min(w / 2.0).min(h / 2.0);
    if r <= 0.0 {
        return format!("M{} {}H{}V{}H{}Z", num(x), num(y), num(x + w), num(y + h), num(x));
    }
    format!(
        "M{} {}H{}A{r} {r} 0 0 1 {} {}V{}A{r} {r} 0 0 1 {} {}H{}A{r} {r} 0 0 1 {} {}V{}A{r} {r} 0 0 1 {} {}Z",
        num(x + r),
        num(y),
        num(x + w - r),
        num(x + w),
        num(y + r),
        num(y + h - r),
        num(x + w - r),
        num(y + h),
        num(x + r),
        num(x),
        num(y + h - r),
        num(y + r),
        num(x + r),
        num(y),
        r = num(r)
    )
}

fn star(cx: f64, cy: f64, rx: f64, ry: f64, points: usize, inner: f64) -> String {
    let mut d = String::new();
    for k in 0..points * 2 {
        let a = -PI / 2.0 + PI * k as f64 / points as f64;
        let f = if k % 2 == 0 { 1.0 } else { inner };
        let _ = write!(
            d,
            "{}{} {}",
            if k == 0 { "M" } else { "L" },
            num(cx + rx * f * a.cos()),
            num(cy + ry * f * a.sin())
        );
    }
    d.push('Z');
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_produces_a_path() {
        let style = BoxStyle {
            border_width: 4.0,
            ..Default::default()
        };
        for (id, _) in SHAPES {
            let svg = shape_svg(id, 300.0, 200.0, &style, "none", "arrow");
            assert!(svg.starts_with("<svg"), "{id}");
            assert!(svg.contains("<path"), "{id}");
            assert!(!svg.contains("NaN"), "{id}");
        }
    }

    #[test]
    fn rect_is_inset_by_half_stroke() {
        assert_eq!(shape_path("rect", 100.0, 50.0, 4.0, 0.0), "M2 2H98V48H2Z");
    }

    #[test]
    fn fill_is_escaped_and_validated() {
        let style = BoxStyle {
            fill: "red\"/><script>".into(),
            ..Default::default()
        };
        let svg = shape_svg("rect", 10.0, 10.0, &style, "none", "none");
        assert!(!svg.contains("<script>"));
    }
}
