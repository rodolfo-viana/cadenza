//! Charts rendered to static SVG.
//!
//! Charts are drawn in Rust so that they look identical in the editor, in the
//! live presentation and in exported files, without any JavaScript library.
//! Colors default to the theme palette through CSS variables.

use std::fmt::Write as _;

use crate::model::ChartContent;
use crate::util::{css_value, esc, num};

pub const CHART_TYPES: &[(&str, &str)] = &[
    ("column", "Column"),
    ("bar", "Bar"),
    ("line", "Line"),
    ("area", "Area"),
    ("pie", "Pie"),
    ("donut", "Donut"),
];

fn series_color(chart: &ChartContent, i: usize) -> String {
    match chart.series.get(i).map(|s| css_value(&s.color)).unwrap_or("") {
        "" => format!("var(--cz-chart-{}, #6366f1)", i % 6 + 1),
        c => c.to_string(),
    }
}

fn slice_color(i: usize) -> String {
    format!("var(--cz-chart-{}, #6366f1)", i % 6 + 1)
}

/// Picks "nice" axis bounds and step for a value range.
pub fn nice_scale(min: f64, max: f64, ticks: usize) -> (f64, f64, f64) {
    let (mut lo, mut hi) = (min.min(0.0), max.max(0.0));
    if (hi - lo).abs() < f64::EPSILON {
        hi = lo + 1.0;
    }
    let raw = (hi - lo) / ticks.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = mag * if norm <= 1.0 { 1.0 } else if norm <= 2.0 { 2.0 } else if norm <= 2.5 { 2.5 } else if norm <= 5.0 { 5.0 } else { 10.0 };
    lo = (lo / step).floor() * step;
    hi = (hi / step).ceil() * step;
    (lo, hi, step)
}

fn fmt_value(v: f64) -> String {
    if v.abs() >= 1_000_000.0 {
        format!("{}M", num(v / 1_000_000.0))
    } else if v.abs() >= 10_000.0 {
        format!("{}k", num(v / 1000.0))
    } else {
        num(v)
    }
}

pub fn chart_svg(chart: &ChartContent, w: f64, h: f64) -> String {
    let w = w.max(40.0);
    let h = h.max(40.0);
    let f = chart.font_size.clamp(8.0, 96.0);
    let mut svg = String::with_capacity(4096);
    let _ = write!(
        svg,
        r#"<svg class="cz-chart-svg" xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" preserveAspectRatio="none" font-size="{f}" fill="currentColor">"#,
        w = num(w),
        h = num(h),
        f = num(f)
    );
    let pie = matches!(chart.chart.as_str(), "pie" | "donut");
    let legend_items: Vec<(String, String)> = if pie {
        chart.labels.iter().enumerate().map(|(i, l)| (l.clone(), slice_color(i))).collect()
    } else {
        chart.series.iter().enumerate().map(|(i, s)| (s.name.clone(), series_color(chart, i))).collect()
    };
    let legend_h = if chart.show_legend && !legend_items.is_empty() { f * 2.0 } else { 0.0 };
    let plot_h = h - legend_h;
    if pie {
        pie_chart(&mut svg, chart, w, plot_h, f);
    } else {
        axis_chart(&mut svg, chart, w, plot_h, f);
    }
    if legend_h > 0.0 {
        legend(&mut svg, &legend_items, w, h - legend_h * 0.5, f);
    }
    svg.push_str("</svg>");
    svg
}

fn legend(svg: &mut String, items: &[(String, String)], w: f64, cy: f64, f: f64) {
    // Approximate text width: 0.55em per character.
    let widths: Vec<f64> = items.iter().map(|(l, _)| f * 1.4 + l.chars().count() as f64 * f * 0.55 + f).collect();
    let total: f64 = widths.iter().sum();
    let mut x = ((w - total) / 2.0).max(0.0);
    for ((label, color), iw) in items.iter().zip(widths) {
        let s = f * 0.8;
        let _ = write!(
            svg,
            r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" style="fill:{}"/><text x="{}" y="{}" dominant-baseline="central">{}</text>"#,
            num(x),
            num(cy - s / 2.0),
            num(s),
            num(s),
            num(s * 0.25),
            esc(color),
            num(x + f * 1.2),
            num(cy),
            esc(label)
        );
        x += iw;
    }
}

fn axis_chart(svg: &mut String, chart: &ChartContent, w: f64, h: f64, f: f64) {
    let horizontal = chart.chart == "bar";
    let n = chart.labels.len().max(chart.series.iter().map(|s| s.values.len()).max().unwrap_or(0));
    if n == 0 || chart.series.is_empty() {
        return;
    }
    let value = |s: usize, i: usize| chart.series[s].values.get(i).copied().filter(|v| v.is_finite()).unwrap_or(0.0);
    let stacked = chart.stacked && matches!(chart.chart.as_str(), "column" | "bar" | "area");
    let (mut vmin, mut vmax) = (0.0f64, 0.0f64);
    for i in 0..n {
        if stacked {
            let (mut pos, mut neg) = (0.0, 0.0);
            for s in 0..chart.series.len() {
                let v = value(s, i);
                if v >= 0.0 { pos += v } else { neg += v }
            }
            vmax = vmax.max(pos);
            vmin = vmin.min(neg);
        } else {
            for s in 0..chart.series.len() {
                vmax = vmax.max(value(s, i));
                vmin = vmin.min(value(s, i));
            }
        }
    }
    let (lo, hi, step) = nice_scale(vmin, vmax, 5);
    let max_label = (0..=((hi - lo) / step).round() as i64)
        .map(|k| fmt_value(lo + k as f64 * step).len())
        .max()
        .unwrap_or(1) as f64;
    let max_cat = chart.labels.iter().map(|l| l.chars().count()).max().unwrap_or(1) as f64;
    let (left, bottom, top, right) = if horizontal {
        ((max_cat * f * 0.58 + f * 0.8).min(w * 0.35), f * 1.8, f * 0.8, f * 1.2)
    } else {
        (max_label * f * 0.6 + f * 0.8, f * 1.8, f * 0.9, f * 0.6)
    };
    let (px, py, pw, ph) = (left, top, (w - left - right).max(10.0), (h - top - bottom).max(10.0));
    // Position along the value axis.
    let vpos = |v: f64| -> f64 {
        let t = (v - lo) / (hi - lo);
        if horizontal { px + t * pw } else { py + ph - t * ph }
    };

    // Grid + value labels.
    let ticks = ((hi - lo) / step).round() as i64;
    for k in 0..=ticks {
        let v = lo + k as f64 * step;
        let p = vpos(v);
        if horizontal {
            if chart.show_grid {
                let _ = write!(svg, r#"<line x1="{x}" y1="{}" x2="{x}" y2="{}" class="cz-grid" stroke="currentColor" stroke-opacity="0.18"/>"#, num(py), num(py + ph), x = num(p));
            }
            let _ = write!(svg, r#"<text x="{}" y="{}" text-anchor="middle" opacity="0.75">{}</text>"#, num(p), num(py + ph + f * 1.3), fmt_value(v));
        } else {
            if chart.show_grid {
                let _ = write!(svg, r#"<line x1="{}" y1="{y}" x2="{}" y2="{y}" class="cz-grid" stroke="currentColor" stroke-opacity="0.18"/>"#, num(px), num(px + pw), y = num(p));
            }
            let _ = write!(svg, r#"<text x="{}" y="{}" text-anchor="end" dominant-baseline="central" opacity="0.75">{}</text>"#, num(px - f * 0.5), num(p), fmt_value(v));
        }
    }
    // Baseline.
    let zero = vpos(0.0);
    if horizontal {
        let _ = write!(svg, r#"<line x1="{x}" y1="{}" x2="{x}" y2="{}" stroke="currentColor" stroke-opacity="0.5"/>"#, num(py), num(py + ph), x = num(zero));
    } else {
        let _ = write!(svg, r#"<line x1="{}" y1="{y}" x2="{}" y2="{y}" stroke="currentColor" stroke-opacity="0.5"/>"#, num(px), num(px + pw), y = num(zero));
    }

    let band = if horizontal { ph } else { pw } / n as f64;
    // Category labels.
    for i in 0..n {
        let label = chart.labels.get(i).map(String::as_str).unwrap_or("");
        let c = (i as f64 + 0.5) * band;
        if horizontal {
            let _ = write!(svg, r#"<text x="{}" y="{}" text-anchor="end" dominant-baseline="central">{}</text>"#, num(px - f * 0.5), num(py + c), esc(label));
        } else {
            let _ = write!(svg, r#"<text x="{}" y="{}" text-anchor="middle">{}</text>"#, num(px + c), num(py + ph + f * 1.3), esc(label));
        }
    }

    let ns = chart.series.len();
    match chart.chart.as_str() {
        "line" | "area" => {
            let area = chart.chart == "area";
            let mut base = vec![0.0f64; n];
            for s in 0..ns {
                let color = series_color(chart, s);
                let pts: Vec<(f64, f64, f64)> = (0..n)
                    .map(|i| {
                        let v = value(s, i);
                        let start = if stacked { base[i] } else { 0.0 };
                        let end = start + v;
                        (px + (i as f64 + 0.5) * band, vpos(end), vpos(start))
                    })
                    .collect();
                if stacked {
                    for (i, b) in base.iter_mut().enumerate() {
                        *b += value(s, i);
                    }
                }
                let line: String = pts.iter().enumerate().map(|(k, (x, y, _))| format!("{}{} {}", if k == 0 { "M" } else { "L" }, num(*x), num(*y))).collect();
                if area {
                    let back: String = pts.iter().rev().map(|(x, _, y0)| format!("L{} {}", num(*x), num(*y0))).collect();
                    let _ = write!(svg, r#"<path d="{line}{back}Z" style="fill:{c};fill-opacity:0.28"/>"#, c = esc(&color));
                }
                let _ = write!(svg, r#"<path d="{}" fill="none" style="stroke:{}" stroke-width="{}" stroke-linejoin="round" stroke-linecap="round"/>"#, line, esc(&color), num((f * 0.18).max(2.0)));
                for (k, (x, y, _)) in pts.iter().enumerate() {
                    let _ = write!(svg, r#"<circle cx="{}" cy="{}" r="{}" style="fill:{}"/>"#, num(*x), num(*y), num((f * 0.22).max(3.0)), esc(&color));
                    if chart.show_values {
                        let _ = write!(svg, r#"<text x="{}" y="{}" text-anchor="middle" font-size="{}">{}</text>"#, num(*x), num(y - f * 0.6), num(f * 0.8), fmt_value(value(s, k)));
                    }
                }
            }
        }
        _ => {
            // Column / bar.
            let gap = band * 0.22;
            let inner = band - gap;
            let bw = if stacked { inner } else { inner / ns as f64 };
            let mut pos_base = vec![0.0f64; n];
            let mut neg_base = vec![0.0f64; n];
            for s in 0..ns {
                let color = series_color(chart, s);
                for i in 0..n {
                    let v = value(s, i);
                    let (start, end) = if stacked {
                        let b = if v >= 0.0 { &mut pos_base[i] } else { &mut neg_base[i] };
                        let st = *b;
                        *b += v;
                        (st, st + v)
                    } else {
                        (0.0, v)
                    };
                    let offset = i as f64 * band + gap / 2.0 + if stacked { 0.0 } else { s as f64 * bw };
                    let (a, b) = (vpos(start), vpos(end));
                    let (lo_p, len) = (a.min(b), (a - b).abs());
                    let r = (bw * 0.12).min(6.0);
                    let (x, y, rw, rh) = if horizontal { (lo_p, py + offset, len, bw) } else { (px + offset, lo_p, bw, len) };
                    let _ = write!(svg, r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" style="fill:{}"/>"#, num(x), num(y), num(rw), num(rh), num(r), esc(&color));
                    if chart.show_values && v != 0.0 {
                        let label = fmt_value(v);
                        if horizontal {
                            let _ = write!(svg, r#"<text x="{}" y="{}" dominant-baseline="central" font-size="{}">{}</text>"#, num(x + rw + f * 0.3), num(y + rh / 2.0), num(f * 0.8), label);
                        } else {
                            let _ = write!(svg, r#"<text x="{}" y="{}" text-anchor="middle" font-size="{}">{}</text>"#, num(x + rw / 2.0), num(y - f * 0.35), num(f * 0.8), label);
                        }
                    }
                }
            }
        }
    }
}

fn pie_chart(svg: &mut String, chart: &ChartContent, w: f64, h: f64, f: f64) {
    let values: Vec<f64> = chart
        .series
        .first()
        .map(|s| s.values.iter().map(|v| if v.is_finite() { v.max(0.0) } else { 0.0 }).collect())
        .unwrap_or_default();
    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        return;
    }
    let (cx, cy) = (w / 2.0, h / 2.0);
    let r = (w.min(h) / 2.0 - f * 0.5).max(4.0);
    let hole = if chart.chart == "donut" { r * 0.58 } else { 0.0 };
    let mut angle = -std::f64::consts::FRAC_PI_2;
    for (i, v) in values.iter().enumerate() {
        if *v <= 0.0 {
            continue;
        }
        let sweep = v / total * std::f64::consts::TAU;
        let color = slice_color(i);
        if (sweep - std::f64::consts::TAU).abs() < 1e-9 {
            let _ = write!(svg, r#"<circle cx="{}" cy="{}" r="{}" style="fill:{}"/>"#, num(cx), num(cy), num(r), esc(&color));
        } else {
            let a2 = angle + sweep;
            let large = if sweep > std::f64::consts::PI { 1 } else { 0 };
            let p = |a: f64, rr: f64| (cx + rr * a.cos(), cy + rr * a.sin());
            let (x1, y1) = p(angle, r);
            let (x2, y2) = p(a2, r);
            let d = if hole > 0.0 {
                let (x3, y3) = p(a2, hole);
                let (x4, y4) = p(angle, hole);
                format!(
                    "M{} {}A{r} {r} 0 {large} 1 {} {}L{} {}A{h} {h} 0 {large} 0 {} {}Z",
                    num(x1), num(y1), num(x2), num(y2), num(x3), num(y3), num(x4), num(y4),
                    r = num(r), h = num(hole), large = large
                )
            } else {
                format!("M{} {}L{} {}A{r} {r} 0 {large} 1 {} {}Z", num(cx), num(cy), num(x1), num(y1), num(x2), num(y2), r = num(r), large = large)
            };
            let _ = write!(svg, r#"<path d="{}" style="fill:{}" stroke="rgba(0,0,0,0.15)" stroke-width="1"/>"#, d, esc(&color));
        }
        if chart.show_values {
            let mid = angle + sweep / 2.0;
            let lr = if hole > 0.0 { (r + hole) / 2.0 } else { r * 0.62 };
            let pct = v / total * 100.0;
            let _ = write!(
                svg,
                r#"<text x="{}" y="{}" text-anchor="middle" dominant-baseline="central" font-weight="600" style="fill:#fff" font-size="{}">{}%</text>"#,
                num(cx + lr * mid.cos()),
                num(cy + lr * mid.sin()),
                num(f * 0.85),
                num(pct.round())
            );
        }
        angle += sweep;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_scale_rounds_outward() {
        for (min, max) in [(0.0, 31.0), (-12.0, 40.0), (0.0, 0.0), (3.0, 7.5), (0.0, 123456.0)] {
            let (lo, hi, step) = nice_scale(min, max, 5);
            assert!(lo <= min.min(0.0) && hi >= max, "{min}..{max}");
            let ticks = ((hi - lo) / step).round();
            assert!((2.0..=10.0).contains(&ticks), "{min}..{max}: {ticks} ticks");
        }
    }

    #[test]
    fn every_chart_type_renders() {
        for (kind, _) in CHART_TYPES {
            for stacked in [false, true] {
                let c = ChartContent { chart: kind.to_string(), stacked, show_values: true, ..Default::default() };
                let svg = chart_svg(&c, 800.0, 450.0);
                assert!(svg.contains("<svg") && svg.ends_with("</svg>"), "{kind}");
                assert!(!svg.contains("NaN"), "{kind}");
            }
        }
    }

    #[test]
    fn empty_chart_does_not_panic() {
        let c = ChartContent { labels: vec![], series: vec![], ..Default::default() };
        assert!(chart_svg(&c, 100.0, 100.0).contains("</svg>"));
    }
}
