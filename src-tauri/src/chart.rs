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
    ("stacked-column", "Stacked column"),
    ("bar", "Bar"),
    ("stacked-bar", "Stacked bar"),
    ("line", "Line"),
    ("area", "Area"),
    ("stacked-area", "Stacked area"),
    ("streamgraph", "Streamgraph"),
    ("pie", "Pie"),
    ("donut", "Donut"),
    ("treemap", "Treemap"),
    ("parliament", "Parliament"),
    ("scatter", "Scatterplot"),
    ("heatmap", "Heatmap"),
    ("candlestick", "Candlestick"),
    ("sankey", "Sankey diagram"),
    ("network", "Network graph"),
    ("map", "Map"),
];

/// Column, bar and area charts: the base kind and whether it is stacked
/// (older files stack with a flag instead of a chart type).
fn axis_kind(chart: &ChartContent) -> (&str, bool) {
    match chart.chart.as_str() {
        "stacked-column" => ("column", true),
        "stacked-bar" => ("bar", true),
        "stacked-area" => ("area", true),
        k @ ("column" | "bar" | "area") => (k, chart.stacked),
        k => (k, false),
    }
}

fn positive_count(chart: &ChartContent) -> u32 {
    chart
        .series
        .first()
        .map_or(0, |s| s.values.iter().filter(|v| v.is_finite() && **v > 0.0).count() as u32)
}

/// Steps a chart takes when it builds by series: one per series, or per
/// slice (pie, donut), party (parliament), rectangle (treemap with one
/// series), point group (scatterplot), column (heatmap, Sankey diagram
/// after its first) or node (network).
pub fn series_steps(chart: &ChartContent) -> u32 {
    let n = chart.series.len() as u32;
    match chart.chart.as_str() {
        "pie" | "donut" | "parliament" => positive_count(chart),
        "treemap" if n == 1 => positive_count(chart),
        "scatter" => n.saturating_sub(1).max(n.min(1)),
        "candlestick" | "map" => 0,
        "sankey" => sankey_columns(&graph(chart, true)).into_iter().max().unwrap_or(0) as u32,
        "network" => graph(chart, false).names.len().min(300) as u32,
        _ => n,
    }
}

fn series_color(chart: &ChartContent, i: usize) -> String {
    match chart.series.get(i).map(|s| css_value(&s.color)).unwrap_or("") {
        "" => slice_color(i),
        c => c.to_string(),
    }
}

/// Palette color `i`: the theme's six chart colors, then lighter and
/// darker versions of them, so that up to 18 parts look different.
fn slice_color(i: usize) -> String {
    let base = format!("var(--cz-chart-{}, #6366f1)", i % 6 + 1);
    match (i / 6) % 3 {
        0 => base,
        1 => format!("color-mix(in srgb, {base} 55%, #fff)"),
        _ => format!("color-mix(in srgb, {base} 60%, #000)"),
    }
}

/// Picks "nice" axis bounds and step for a value range, from zero.
pub fn nice_scale(min: f64, max: f64, ticks: usize) -> (f64, f64, f64) {
    nice_range(min.min(0.0), max.max(0.0), ticks)
}

/// Picks "nice" axis bounds and step around a value range.
pub fn nice_range(min: f64, max: f64, ticks: usize) -> (f64, f64, f64) {
    let (mut lo, mut hi) = (min, max);
    if (hi - lo).abs() < f64::EPSILON {
        let pad = if lo == 0.0 { 1.0 } else { lo.abs() * 0.1 };
        lo -= if lo == 0.0 { 0.0 } else { pad };
        hi += pad;
    }
    let raw = (hi - lo) / ticks.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = mag
        * if norm <= 1.0 {
            1.0
        } else if norm <= 2.0 {
            2.0
        } else if norm <= 2.5 {
            2.5
        } else if norm <= 5.0 {
            5.0
        } else {
            10.0
        };
    lo = (lo / step).floor() * step;
    hi = (hi / step).ceil() * step;
    (lo, hi, step)
}

pub(crate) fn fmt_value(v: f64) -> String {
    if v.abs() >= 1_000_000.0 {
        format!("{}M", num(v / 1_000_000.0))
    } else if v.abs() >= 10_000.0 {
        format!("{}k", num(v / 1000.0))
    } else {
        num(v)
    }
}

/// Chart builds by series: the build-in effect and the step of the first
/// series. Each series (pie and donut: each slice) becomes its own reveal.js
/// fragment, one step after another.
#[derive(Clone, Copy)]
pub struct SeriesSteps<'a> {
    pub effect: &'a str,
    pub first: u32,
}

fn open_step(svg: &mut String, steps: Option<SeriesSteps>, k: u32) {
    if let Some(st) = steps {
        let _ = write!(svg, r#"<g class="fragment {}" data-fragment-index="{}">"#, esc(st.effect), st.first + k);
    }
}

fn close_step(svg: &mut String, steps: Option<SeriesSteps>) {
    if steps.is_some() {
        svg.push_str("</g>");
    }
}

pub fn chart_svg(chart: &ChartContent, w: f64, h: f64, steps: Option<SeriesSteps>) -> String {
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
    let kind = chart.chart.as_str();
    let by_label = matches!(kind, "pie" | "donut" | "parliament") || (kind == "treemap" && chart.series.len() <= 1);
    let legend_items: Vec<(String, String)> = match kind {
        "heatmap" | "candlestick" | "sankey" | "network" | "map" => Vec::new(),
        "parliament" if chart.show_values => {
            let seats = parliament_seats(chart);
            chart
                .labels
                .iter()
                .enumerate()
                .map(|(i, l)| (format!("{l} ({})", seats.get(i).unwrap_or(&0)), slice_color(i)))
                .collect()
        }
        _ if by_label => chart.labels.iter().enumerate().map(|(i, l)| (l.clone(), slice_color(i))).collect(),
        // A scatterplot's first series holds the X values.
        "scatter" if chart.series.len() > 1 => chart
            .series
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, s)| (s.name.clone(), series_color(chart, i)))
            .collect(),
        _ => chart.series.iter().enumerate().map(|(i, s)| (s.name.clone(), series_color(chart, i))).collect(),
    };
    let scale_legend = (kind == "heatmap" && value_range(chart).is_some()) || kind == "map";
    let legend_h = if chart.show_legend && (!legend_items.is_empty() || scale_legend) {
        f * 2.0
    } else {
        0.0
    };
    let plot_h = h - legend_h;
    let mut scale = if kind == "heatmap" { value_range(chart) } else { None };
    match kind {
        "pie" | "donut" => pie_chart(&mut svg, chart, w, plot_h, f, steps),
        "parliament" => parliament_chart(&mut svg, chart, w, plot_h, f, steps),
        "treemap" => treemap_chart(&mut svg, chart, w, plot_h, f, steps),
        "scatter" => scatter_chart(&mut svg, chart, w, plot_h, f, steps),
        "heatmap" => heatmap_chart(&mut svg, chart, w, plot_h, f, steps),
        "streamgraph" => stream_chart(&mut svg, chart, w, plot_h, f, steps),
        "candlestick" => candle_chart(&mut svg, chart, w, plot_h, f),
        "sankey" => sankey_chart(&mut svg, chart, w, plot_h, f, steps),
        "network" => network_chart(&mut svg, chart, w, plot_h, f, steps),
        "map" => {
            let values = chart.series.first().map_or(&[][..], |s| &s.values[..]);
            let color = scale_color(chart);
            scale = crate::maps::map_chart(&mut svg, &chart.map, &chart.labels, values, &color, chart.show_values, (w, plot_h, f));
        }
        _ => axis_chart(&mut svg, chart, w, plot_h, f, steps),
    }
    if legend_h > 0.0 {
        if scale_legend {
            if let Some(range) = scale {
                heat_legend(&mut svg, range, &scale_color(chart), w, h - legend_h * 0.5, f);
            }
        } else {
            legend(&mut svg, &legend_items, w, h - legend_h * 0.5, f);
        }
    }
    svg.push_str("</svg>");
    svg
}

/// Rows of data: category labels, or the longest series.
fn rows(chart: &ChartContent) -> usize {
    chart.labels.len().max(chart.series.iter().map(|s| s.values.len()).max().unwrap_or(0))
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

fn axis_chart(svg: &mut String, chart: &ChartContent, w: f64, h: f64, f: f64, steps: Option<SeriesSteps>) {
    let (kind, stacked) = axis_kind(chart);
    let horizontal = kind == "bar";
    let n = rows(chart);
    if n == 0 || chart.series.is_empty() {
        return;
    }
    let value = |s: usize, i: usize| chart.series[s].values.get(i).copied().filter(|v| v.is_finite()).unwrap_or(0.0);
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
                let _ = write!(
                    svg,
                    r#"<line x1="{x}" y1="{}" x2="{x}" y2="{}" class="cz-grid" stroke="currentColor" stroke-opacity="0.18"/>"#,
                    num(py),
                    num(py + ph),
                    x = num(p)
                );
            }
            let _ = write!(
                svg,
                r#"<text x="{}" y="{}" text-anchor="middle" opacity="0.75">{}</text>"#,
                num(p),
                num(py + ph + f * 1.3),
                fmt_value(v)
            );
        } else {
            if chart.show_grid {
                let _ = write!(
                    svg,
                    r#"<line x1="{}" y1="{y}" x2="{}" y2="{y}" class="cz-grid" stroke="currentColor" stroke-opacity="0.18"/>"#,
                    num(px),
                    num(px + pw),
                    y = num(p)
                );
            }
            let _ = write!(
                svg,
                r#"<text x="{}" y="{}" text-anchor="end" dominant-baseline="central" opacity="0.75">{}</text>"#,
                num(px - f * 0.5),
                num(p),
                fmt_value(v)
            );
        }
    }
    // Baseline.
    let zero = vpos(0.0);
    if horizontal {
        let _ = write!(
            svg,
            r#"<line x1="{x}" y1="{}" x2="{x}" y2="{}" stroke="currentColor" stroke-opacity="0.5"/>"#,
            num(py),
            num(py + ph),
            x = num(zero)
        );
    } else {
        let _ = write!(
            svg,
            r#"<line x1="{}" y1="{y}" x2="{}" y2="{y}" stroke="currentColor" stroke-opacity="0.5"/>"#,
            num(px),
            num(px + pw),
            y = num(zero)
        );
    }

    let band = if horizontal { ph } else { pw } / n as f64;
    // Category labels.
    for i in 0..n {
        let label = chart.labels.get(i).map(String::as_str).unwrap_or("");
        let c = (i as f64 + 0.5) * band;
        if horizontal {
            let _ = write!(
                svg,
                r#"<text x="{}" y="{}" text-anchor="end" dominant-baseline="central">{}</text>"#,
                num(px - f * 0.5),
                num(py + c),
                esc(label)
            );
        } else {
            let _ = write!(
                svg,
                r#"<text x="{}" y="{}" text-anchor="middle">{}</text>"#,
                num(px + c),
                num(py + ph + f * 1.3),
                esc(label)
            );
        }
    }

    let ns = chart.series.len();
    match kind {
        "line" | "area" => {
            let area = kind == "area";
            let mut base = vec![0.0f64; n];
            for s in 0..ns {
                open_step(svg, steps, s as u32);
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
                let line: String = pts
                    .iter()
                    .enumerate()
                    .map(|(k, (x, y, _))| format!("{}{} {}", if k == 0 { "M" } else { "L" }, num(*x), num(*y)))
                    .collect();
                if area {
                    let back: String = pts.iter().rev().map(|(x, _, y0)| format!("L{} {}", num(*x), num(*y0))).collect();
                    let _ = write!(svg, r#"<path d="{line}{back}Z" style="fill:{c};fill-opacity:0.28"/>"#, c = esc(&color));
                }
                let _ = write!(
                    svg,
                    r#"<path d="{}" fill="none" style="stroke:{}" stroke-width="{}" stroke-linejoin="round" stroke-linecap="round"/>"#,
                    line,
                    esc(&color),
                    num((f * 0.18).max(2.0))
                );
                for (k, (x, y, _)) in pts.iter().enumerate() {
                    let _ = write!(
                        svg,
                        r#"<circle cx="{}" cy="{}" r="{}" style="fill:{}"/>"#,
                        num(*x),
                        num(*y),
                        num((f * 0.22).max(3.0)),
                        esc(&color)
                    );
                    if chart.show_values {
                        let _ = write!(
                            svg,
                            r#"<text x="{}" y="{}" text-anchor="middle" font-size="{}">{}</text>"#,
                            num(*x),
                            num(y - f * 0.6),
                            num(f * 0.8),
                            fmt_value(value(s, k))
                        );
                    }
                }
                close_step(svg, steps);
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
                open_step(svg, steps, s as u32);
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
                    let (x, y, rw, rh) = if horizontal {
                        (lo_p, py + offset, len, bw)
                    } else {
                        (px + offset, lo_p, bw, len)
                    };
                    let _ = write!(
                        svg,
                        r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" style="fill:{}"/>"#,
                        num(x),
                        num(y),
                        num(rw),
                        num(rh),
                        num(r),
                        esc(&color)
                    );
                    if chart.show_values && v != 0.0 {
                        let label = fmt_value(v);
                        if horizontal {
                            let _ = write!(
                                svg,
                                r#"<text x="{}" y="{}" dominant-baseline="central" font-size="{}">{}</text>"#,
                                num(x + rw + f * 0.3),
                                num(y + rh / 2.0),
                                num(f * 0.8),
                                label
                            );
                        } else {
                            let _ = write!(
                                svg,
                                r#"<text x="{}" y="{}" text-anchor="middle" font-size="{}">{}</text>"#,
                                num(x + rw / 2.0),
                                num(y - f * 0.35),
                                num(f * 0.8),
                                label
                            );
                        }
                    }
                }
                close_step(svg, steps);
            }
        }
    }
}

fn pie_chart(svg: &mut String, chart: &ChartContent, w: f64, h: f64, f: f64, steps: Option<SeriesSteps>) {
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
    let mut slice = 0;
    for (i, v) in values.iter().enumerate() {
        if *v <= 0.0 {
            continue;
        }
        open_step(svg, steps, slice);
        slice += 1;
        let sweep = v / total * std::f64::consts::TAU;
        let color = slice_color(i);
        if (sweep - std::f64::consts::TAU).abs() < 1e-9 {
            let _ = write!(
                svg,
                r#"<circle cx="{}" cy="{}" r="{}" style="fill:{}"/>"#,
                num(cx),
                num(cy),
                num(r),
                esc(&color)
            );
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
                    num(x1),
                    num(y1),
                    num(x2),
                    num(y2),
                    num(x3),
                    num(y3),
                    num(x4),
                    num(y4),
                    r = num(r),
                    h = num(hole),
                    large = large
                )
            } else {
                format!(
                    "M{} {}L{} {}A{r} {r} 0 {large} 1 {} {}Z",
                    num(cx),
                    num(cy),
                    num(x1),
                    num(y1),
                    num(x2),
                    num(y2),
                    r = num(r),
                    large = large
                )
            };
            let _ = write!(
                svg,
                r#"<path d="{}" style="fill:{}" stroke="rgba(0,0,0,0.15)" stroke-width="1"/>"#,
                d,
                esc(&color)
            );
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
        close_step(svg, steps);
        angle += sweep;
    }
}

// ---------------------------------------------------------------------------
// Shared pieces for the chart types below
// ---------------------------------------------------------------------------

/// Plot area of a chart with axes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Finite values of every series, as (min, max).
fn value_range(chart: &ChartContent) -> Option<(f64, f64)> {
    let n = rows(chart);
    chart
        .series
        .iter()
        .flat_map(|s| s.values.iter().take(n.max(1)))
        .filter(|v| v.is_finite())
        .fold(None, |r, &v| Some(r.map_or((v, v), |(lo, hi): (f64, f64)| (lo.min(v), hi.max(v)))))
}

/// Every how many category labels to print so that they do not overlap.
fn label_stride(chart: &ChartContent, n: usize, band: f64, f: f64) -> usize {
    let longest = chart.labels.iter().map(|l| l.chars().count()).max().unwrap_or(1).max(1) as f64;
    let need = longest * f * 0.6 + f * 0.6;
    ((need / band.max(1.0)).ceil() as usize).clamp(1, n.max(1))
}

/// Value grid and labels along the vertical axis of `plot`.
fn y_axis(svg: &mut String, chart: &ChartContent, plot: Rect, (lo, hi, step): (f64, f64, f64), f: f64) {
    let ticks = ((hi - lo) / step).round() as i64;
    for k in 0..=ticks {
        let v = lo + k as f64 * step;
        let y = plot.y + plot.h - (v - lo) / (hi - lo) * plot.h;
        if chart.show_grid {
            let _ = write!(
                svg,
                r#"<line x1="{}" y1="{y}" x2="{}" y2="{y}" class="cz-grid" stroke="currentColor" stroke-opacity="0.18"/>"#,
                num(plot.x),
                num(plot.x + plot.w),
                y = num(y)
            );
        }
        let _ = write!(
            svg,
            r#"<text x="{}" y="{}" text-anchor="end" dominant-baseline="central" opacity="0.75">{}</text>"#,
            num(plot.x - f * 0.5),
            num(y),
            fmt_value(v)
        );
    }
}

/// Room for the labels of a value axis.
fn y_label_room(range: (f64, f64, f64), f: f64) -> f64 {
    let (lo, hi, step) = range;
    let longest = (0..=((hi - lo) / step).round() as i64)
        .map(|k| fmt_value(lo + k as f64 * step).len())
        .max()
        .unwrap_or(1) as f64;
    longest * f * 0.6 + f * 0.8
}

/// Category labels under `plot`, one per band (thinned out when crowded).
fn x_categories(svg: &mut String, chart: &ChartContent, plot: Rect, n: usize, x_of: impl Fn(usize) -> f64, band: f64, f: f64) {
    let stride = label_stride(chart, n, band, f);
    for i in (0..n).step_by(stride) {
        let label = chart.labels.get(i).map(String::as_str).unwrap_or("");
        let _ = write!(
            svg,
            r#"<text x="{}" y="{}" text-anchor="middle">{}</text>"#,
            num(x_of(i)),
            num(plot.y + plot.h + f * 1.3),
            esc(label)
        );
    }
}

/// Bézier segments of a smooth curve through points with increasing x that
/// never overshoots them (monotone cubic interpolation, like d3's
/// curveMonotoneX): (first control, second control, end) per segment.
type Point = (f64, f64);
fn monotone_segments(p: &[Point]) -> Vec<(Point, Point, Point)> {
    let n = p.len();
    if n < 2 {
        return Vec::new();
    }
    let h: Vec<f64> = (0..n - 1).map(|i| p[i + 1].0 - p[i].0).collect();
    let s: Vec<f64> = (0..n - 1).map(|i| if h[i] != 0.0 { (p[i + 1].1 - p[i].1) / h[i] } else { 0.0 }).collect();
    let sign = |x: f64| if x < 0.0 { -1.0 } else { 1.0 };
    let mut t = vec![0.0; n];
    for i in 1..n - 1 {
        let q = (s[i - 1] * h[i] + s[i] * h[i - 1]) / (h[i - 1] + h[i]);
        let v = (sign(s[i - 1]) + sign(s[i])) * s[i - 1].abs().min(s[i].abs()).min(0.5 * q.abs());
        t[i] = if v.is_finite() { v } else { 0.0 };
    }
    if n == 2 {
        t[0] = s[0];
        t[1] = s[0];
    } else {
        t[0] = (3.0 * s[0] - t[1]) / 2.0;
        t[n - 1] = (3.0 * s[n - 2] - t[n - 2]) / 2.0;
    }
    (0..n - 1)
        .map(|i| {
            let dx = h[i] / 3.0;
            ((p[i].0 + dx, p[i].1 + dx * t[i]), (p[i + 1].0 - dx, p[i + 1].1 - dx * t[i + 1]), p[i + 1])
        })
        .collect()
}

fn curve_to(out: &mut String, (c1, c2, end): (Point, Point, Point)) {
    let _ = write!(out, "C{} {} {} {} {} {}", num(c1.0), num(c1.1), num(c2.0), num(c2.1), num(end.0), num(end.1));
}

// ---------------------------------------------------------------------------
// Streamgraph: stacked series flowing around a center line
// ---------------------------------------------------------------------------

fn stream_chart(svg: &mut String, chart: &ChartContent, w: f64, h: f64, f: f64, steps: Option<SeriesSteps>) {
    let n = rows(chart);
    let m = chart.series.len();
    if n == 0 || m == 0 {
        return;
    }
    let v = |s: usize, i: usize| chart.series[s].values.get(i).copied().filter(|v| v.is_finite()).unwrap_or(0.0).max(0.0);
    let totals: Vec<f64> = (0..n).map(|i| (0..m).map(|s| v(s, i)).sum()).collect();
    let top = totals.iter().cloned().fold(0.0, f64::max);
    if top <= 0.0 {
        return;
    }
    // Room for half of the first and last labels, centered on the edges.
    let side = (chart.labels.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64 * f * 0.3).max(f * 0.8);
    let plot = Rect {
        x: side,
        y: f * 0.5,
        w: (w - side * 2.0).max(10.0),
        h: (h - f * 2.3).max(10.0),
    };
    // One category still spans the width.
    let (cols, xs): (Vec<usize>, Vec<f64>) = if n == 1 {
        (vec![0, 0], vec![plot.x, plot.x + plot.w])
    } else {
        ((0..n).collect(), (0..n).map(|i| plot.x + i as f64 * plot.w / (n - 1) as f64).collect())
    };
    let scale = plot.h / top;
    let mid = plot.y + plot.h / 2.0;
    if chart.show_grid {
        for x in &xs {
            let _ = write!(
                svg,
                r#"<line x1="{x}" y1="{}" x2="{x}" y2="{}" class="cz-grid" stroke="currentColor" stroke-opacity="0.18"/>"#,
                num(plot.y),
                num(plot.y + plot.h),
                x = num(*x)
            );
        }
    }
    let mut cum = vec![0.0f64; n];
    for s in 0..m {
        open_step(svg, steps, s as u32);
        let y = |val: f64| mid - val * scale;
        let lower: Vec<Point> = cols.iter().zip(&xs).map(|(&i, &x)| (x, y(cum[i] - totals[i] / 2.0))).collect();
        for (i, c) in cum.iter_mut().enumerate() {
            *c += v(s, i);
        }
        let upper: Vec<Point> = cols.iter().zip(&xs).map(|(&i, &x)| (x, y(cum[i] - totals[i] / 2.0))).collect();
        let mut d = format!("M{} {}", num(upper[0].0), num(upper[0].1));
        for seg in monotone_segments(&upper) {
            curve_to(&mut d, seg);
        }
        let last = lower[lower.len() - 1];
        let _ = write!(d, "L{} {}", num(last.0), num(last.1));
        // The lower edge backwards: same curve, controls swapped.
        let segs = monotone_segments(&lower);
        for (k, (c1, c2, _)) in segs.iter().enumerate().rev() {
            curve_to(&mut d, (*c2, *c1, lower[k]));
        }
        let _ = write!(
            svg,
            r#"<path d="{d}Z" style="fill:{}" fill-opacity="0.88" stroke="rgba(0,0,0,0.12)" stroke-width="1"/>"#,
            esc(&series_color(chart, s))
        );
        close_step(svg, steps);
    }
    let band = if n > 1 { plot.w / (n - 1) as f64 } else { plot.w };
    x_categories(svg, chart, plot, n, |i| if n == 1 { plot.x + plot.w / 2.0 } else { xs[i] }, band, f);
}

// ---------------------------------------------------------------------------
// Scatterplot: the first series holds X, each other series is a group (Y)
// ---------------------------------------------------------------------------

fn scatter_chart(svg: &mut String, chart: &ChartContent, w: f64, h: f64, f: f64, steps: Option<SeriesSteps>) {
    let ns = chart.series.len();
    let n = rows(chart);
    if ns == 0 || n == 0 {
        return;
    }
    let get = |s: usize, i: usize| chart.series[s].values.get(i).copied().filter(|v| v.is_finite());
    // With a single series, X is the row number.
    let x_of = |i: usize| if ns > 1 { get(0, i) } else { Some(i as f64 + 1.0) };
    let groups: Vec<usize> = if ns > 1 { (1..ns).collect() } else { vec![0] };
    let points: Vec<(usize, usize, f64, f64)> = groups
        .iter()
        .flat_map(|&g| (0..n).filter_map(move |i| Some((g, i, x_of(i)?, get(g, i)?))))
        .collect();
    if points.is_empty() {
        return;
    }
    let (x0, x1) = points.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.2), b.max(p.2)));
    let (y0, y1) = points.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.3), b.max(p.3)));
    let xr = nice_range(x0, x1, 5);
    let yr = nice_range(y0, y1, 5);
    let left = y_label_room(yr, f);
    let plot = Rect {
        x: left,
        y: f * 0.9,
        w: (w - left - f * 1.4).max(10.0),
        h: (h - f * 2.7).max(10.0),
    };
    y_axis(svg, chart, plot, yr, f);
    let px = |x: f64| plot.x + (x - xr.0) / (xr.1 - xr.0) * plot.w;
    let py = |y: f64| plot.y + plot.h - (y - yr.0) / (yr.1 - yr.0) * plot.h;
    let ticks = ((xr.1 - xr.0) / xr.2).round() as i64;
    for k in 0..=ticks {
        let x = px(xr.0 + k as f64 * xr.2);
        if chart.show_grid {
            let _ = write!(
                svg,
                r#"<line x1="{x}" y1="{}" x2="{x}" y2="{}" class="cz-grid" stroke="currentColor" stroke-opacity="0.18"/>"#,
                num(plot.y),
                num(plot.y + plot.h),
                x = num(x)
            );
        }
        let _ = write!(
            svg,
            r#"<text x="{}" y="{}" text-anchor="middle" opacity="0.75">{}</text>"#,
            num(x),
            num(plot.y + plot.h + f * 1.3),
            fmt_value(xr.0 + k as f64 * xr.2)
        );
    }
    let _ = write!(
        svg,
        r#"<path d="M{} {}V{}H{}" fill="none" stroke="currentColor" stroke-opacity="0.5"/>"#,
        num(plot.x),
        num(plot.y),
        num(plot.y + plot.h),
        num(plot.x + plot.w)
    );
    let r = (f * 0.32).max(3.0);
    for (k, &g) in groups.iter().enumerate() {
        open_step(svg, steps, k as u32);
        let color = series_color(chart, g);
        for &(_, i, x, y) in points.iter().filter(|p| p.0 == g) {
            let _ = write!(
                svg,
                r#"<circle cx="{}" cy="{}" r="{}" style="fill:{}" fill-opacity="0.85"/>"#,
                num(px(x)),
                num(py(y)),
                num(r),
                esc(&color)
            );
            if chart.show_values
                && let Some(label) = chart.labels.get(i).filter(|l| !l.is_empty())
            {
                let _ = write!(
                    svg,
                    r#"<text x="{}" y="{}" dominant-baseline="central" font-size="{}">{}</text>"#,
                    num(px(x) + r + f * 0.2),
                    num(py(y)),
                    num(f * 0.7),
                    esc(label)
                );
            }
        }
        close_step(svg, steps);
    }
}

// ---------------------------------------------------------------------------
// Heatmap: rows are the categories, columns the series
// ---------------------------------------------------------------------------

fn scale_color(chart: &ChartContent) -> String {
    match css_value(&chart.scale_color) {
        "" => "var(--cz-chart-1, #6366f1)".into(),
        c => c.to_string(),
    }
}

/// How strongly a value is colored: 0.1 (lowest) to 1 (highest); halfway
/// when all values are alike.
pub(crate) fn heat(v: f64, (lo, hi): (f64, f64)) -> f64 {
    let t = if hi > lo { (v - lo) / (hi - lo) } else { 0.5 };
    0.1 + 0.9 * t.clamp(0.0, 1.0)
}

fn heatmap_chart(svg: &mut String, chart: &ChartContent, w: f64, h: f64, f: f64, steps: Option<SeriesSteps>) {
    let n = rows(chart);
    let m = chart.series.len();
    let Some(range) = value_range(chart) else { return };
    if n == 0 || m == 0 {
        return;
    }
    let longest = chart.labels.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64;
    let left = if longest > 0.0 {
        (longest * f * 0.58 + f * 0.6).min(w * 0.35)
    } else {
        f * 0.3
    };
    let plot = Rect {
        x: left,
        y: f * 1.6,
        w: (w - left - f * 0.3).max(10.0),
        h: (h - f * 1.9).max(10.0),
    };
    let (cw, ch) = (plot.w / m as f64, plot.h / n as f64);
    let gap = (cw.min(ch) * 0.06).min(3.0);
    let color = scale_color(chart);
    for i in 0..n {
        let label = chart.labels.get(i).map(String::as_str).unwrap_or("");
        let _ = write!(
            svg,
            r#"<text x="{}" y="{}" text-anchor="end" dominant-baseline="central">{}</text>"#,
            num(plot.x - f * 0.4),
            num(plot.y + (i as f64 + 0.5) * ch),
            esc(label)
        );
    }
    for (c, s) in chart.series.iter().enumerate() {
        let _ = write!(
            svg,
            r#"<text x="{}" y="{}" text-anchor="middle">{}</text>"#,
            num(plot.x + (c as f64 + 0.5) * cw),
            num(plot.y - f * 0.5),
            esc(&s.name)
        );
    }
    for (c, s) in chart.series.iter().enumerate() {
        open_step(svg, steps, c as u32);
        for i in 0..n {
            let Some(v) = s.values.get(i).copied().filter(|v| v.is_finite()) else {
                continue;
            };
            let o = heat(v, range);
            let (x, y) = (plot.x + c as f64 * cw + gap / 2.0, plot.y + i as f64 * ch + gap / 2.0);
            let _ = write!(
                svg,
                r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" style="fill:{}" fill-opacity="{}"/>"#,
                num(x),
                num(y),
                num((cw - gap).max(0.5)),
                num((ch - gap).max(0.5)),
                num((cw.min(ch) * 0.08).min(4.0)),
                esc(&color),
                num(o)
            );
            if chart.show_values {
                let _ = write!(
                    svg,
                    r#"<text x="{}" y="{}" text-anchor="middle" dominant-baseline="central" font-size="{}"{}>{}</text>"#,
                    num(x + (cw - gap) / 2.0),
                    num(y + (ch - gap) / 2.0),
                    num(f * 0.8),
                    if o > 0.55 { r#" style="fill:#fff""# } else { "" },
                    fmt_value(v)
                );
            }
        }
        close_step(svg, steps);
    }
}

/// The color scale of heatmaps and maps: lowest value, steps of color,
/// highest value.
fn heat_legend(svg: &mut String, (lo, hi): (f64, f64), color: &str, w: f64, cy: f64, f: f64) {
    let (a, b) = (fmt_value(lo), fmt_value(hi));
    let cell = f * 0.9;
    let total = (a.len() + b.len()) as f64 * f * 0.6 + f * 1.2 + cell * 10.0;
    let mut x = ((w - total) / 2.0).max(0.0);
    let _ = write!(
        svg,
        r#"<text x="{}" y="{}" dominant-baseline="central" opacity="0.75">{}</text>"#,
        num(x),
        num(cy),
        a
    );
    x += a.len() as f64 * f * 0.6 + f * 0.6;
    for k in 0..10 {
        let _ = write!(
            svg,
            r#"<rect x="{}" y="{}" width="{}" height="{}" style="fill:{}" fill-opacity="{}"/>"#,
            num(x + k as f64 * cell),
            num(cy - f * 0.4),
            num(cell + 0.5),
            num(f * 0.8),
            esc(color),
            num(0.1 + 0.9 * k as f64 / 9.0)
        );
    }
    let _ = write!(
        svg,
        r#"<text x="{}" y="{}" dominant-baseline="central" opacity="0.75">{}</text>"#,
        num(x + cell * 10.0 + f * 0.6),
        num(cy),
        b
    );
}

// ---------------------------------------------------------------------------
// Treemap: one rectangle per row; with several series, one group per series
// ---------------------------------------------------------------------------

/// Squarified treemap layout (Bruls, Huizing and van Wijk): one rectangle
/// per value, in the order given; zero and negative values get none.
fn squarify(values: &[f64], area: Rect) -> Vec<Option<Rect>> {
    let mut out = vec![None; values.len()];
    let mut order: Vec<usize> = (0..values.len()).filter(|&i| values[i].is_finite() && values[i] > 0.0).collect();
    let total: f64 = order.iter().map(|&i| values[i]).sum();
    if total <= 0.0 || area.w <= 0.0 || area.h <= 0.0 {
        return out;
    }
    order.sort_by(|a, b| values[*b].total_cmp(&values[*a]));
    let scale = area.w * area.h / total;
    let size = |k: usize| values[order[k]] * scale;
    let mut rest = area;
    let mut i = 0;
    while i < order.len() {
        let side = rest.w.min(rest.h).max(1e-9);
        // The worst aspect ratio of a row made of order[i..j].
        let worst = |j: usize| {
            let (sum, lo, hi) = (i..j).fold((0.0, f64::MAX, 0.0f64), |(s, lo, hi), k| (s + size(k), lo.min(size(k)), hi.max(size(k))));
            (side * side * hi / (sum * sum)).max(sum * sum / (side * side * lo))
        };
        let mut j = i + 1;
        while j < order.len() && worst(j + 1) <= worst(j) {
            j += 1;
        }
        let sum: f64 = (i..j).map(size).sum();
        if rest.w >= rest.h {
            let rw = (sum / rest.h.max(1e-9)).min(rest.w);
            let mut y = rest.y;
            for k in i..j {
                let rh = size(k) / rw.max(1e-9);
                out[order[k]] = Some(Rect { x: rest.x, y, w: rw, h: rh });
                y += rh;
            }
            rest.x += rw;
            rest.w = (rest.w - rw).max(0.0);
        } else {
            let rh = (sum / rest.w.max(1e-9)).min(rest.h);
            let mut x = rest.x;
            for k in i..j {
                let rw = size(k) / rh.max(1e-9);
                out[order[k]] = Some(Rect { x, y: rest.y, w: rw, h: rh });
                x += rw;
            }
            rest.y += rh;
            rest.h = (rest.h - rh).max(0.0);
        }
        i = j;
    }
    out
}

/// A label cut to fit `width` (about 0.55em per character).
fn fit_label(label: &str, width: f64, size: f64) -> Option<String> {
    let max = (width / (size * 0.55)).floor() as usize;
    let count = label.chars().count();
    if count == 0 || max < 2 {
        return None;
    }
    Some(if count <= max {
        label.to_string()
    } else {
        format!("{}…", label.chars().take(max - 1).collect::<String>())
    })
}

fn treemap_cell(svg: &mut String, r: Rect, color: &str, label: &str, value: Option<f64>, f: f64) {
    let g = (r.w.min(r.h) * 0.04).clamp(0.5, 2.0);
    let (x, y, w, h) = (r.x + g / 2.0, r.y + g / 2.0, (r.w - g).max(0.0), (r.h - g).max(0.0));
    let _ = write!(
        svg,
        r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" style="fill:{}"/>"#,
        num(x),
        num(y),
        num(w),
        num(h),
        num((w.min(h) * 0.06).min(4.0)),
        esc(color)
    );
    let size = f * 0.85;
    if h < size * 1.5 {
        return;
    }
    let Some(text) = fit_label(label, w - f * 0.6, size) else { return };
    let _ = write!(
        svg,
        r#"<text x="{}" y="{}" font-size="{}" font-weight="600" style="fill:#fff">{}</text>"#,
        num(x + f * 0.3),
        num(y + size * 1.15),
        num(size),
        esc(&text)
    );
    if let Some(v) = value.filter(|_| h >= size * 2.7) {
        let _ = write!(
            svg,
            r#"<text x="{}" y="{}" font-size="{}" style="fill:#fff" opacity="0.85">{}</text>"#,
            num(x + f * 0.3),
            num(y + size * 2.3),
            num(size * 0.9),
            fmt_value(v)
        );
    }
}

fn treemap_chart(svg: &mut String, chart: &ChartContent, w: f64, h: f64, f: f64, steps: Option<SeriesSteps>) {
    let area = Rect { x: 0.0, y: 0.0, w, h };
    let label = |i: usize| chart.labels.get(i).map(String::as_str).unwrap_or("");
    let shown = |v: f64| chart.show_values.then_some(v);
    if chart.series.len() <= 1 {
        let Some(s) = chart.series.first() else { return };
        let mut step = 0;
        for (i, r) in squarify(&s.values, area).into_iter().enumerate() {
            let Some(r) = r else { continue };
            open_step(svg, steps, step);
            step += 1;
            treemap_cell(svg, r, &slice_color(i), label(i), shown(s.values[i]), f);
            close_step(svg, steps);
        }
        return;
    }
    // Groups: one per series, sized by their totals.
    let totals: Vec<f64> = chart
        .series
        .iter()
        .map(|s| s.values.iter().filter(|v| v.is_finite() && **v > 0.0).sum())
        .collect();
    for (g, r) in squarify(&totals, area).into_iter().enumerate() {
        open_step(svg, steps, g as u32);
        if let Some(r) = r {
            let color = series_color(chart, g);
            let head = if r.h > f * 3.5 && r.w > f * 3.0 { f * 1.4 } else { 0.0 };
            if head > 0.0
                && let Some(text) = fit_label(&chart.series[g].name, r.w - f * 0.4, f * 0.8)
            {
                let _ = write!(
                    svg,
                    r#"<text x="{}" y="{}" font-size="{}" font-weight="700" style="fill:{}">{}</text>"#,
                    num(r.x + f * 0.2),
                    num(r.y + f * 1.0),
                    num(f * 0.8),
                    esc(&color),
                    esc(&text)
                );
            }
            let inner = Rect {
                x: r.x,
                y: r.y + head,
                w: r.w,
                h: r.h - head,
            };
            let values = &chart.series[g].values;
            for (i, c) in squarify(values, inner).into_iter().enumerate() {
                if let Some(c) = c {
                    treemap_cell(svg, c, &color, label(i), shown(values[i]), f);
                }
            }
        }
        close_step(svg, steps);
    }
}

// ---------------------------------------------------------------------------
// Parliament: seats as dots in a half circle, party by party
// ---------------------------------------------------------------------------

/// Seats per party (row) of the first series; at most 2,000 in all.
fn parliament_seats(chart: &ChartContent) -> Vec<u32> {
    let raw: Vec<f64> = chart.series.first().map_or_else(Vec::new, |s| {
        s.values.iter().map(|v| if v.is_finite() && *v > 0.0 { v.round() } else { 0.0 }).collect()
    });
    let total: f64 = raw.iter().sum();
    let k = if total > 2000.0 { 2000.0 / total } else { 1.0 };
    raw.iter().map(|v| (v * k).round() as u32).collect()
}

/// Seat positions (angle, radius as a share of the outer radius) for
/// `total` seats, ordered from left to right, and the dot spacing.
fn parliament_layout(total: u32) -> (Vec<(f64, f64)>, f64) {
    const INNER: f64 = 0.38;
    let row_caps = |rows: u32| -> (f64, Vec<u32>) {
        let d = (1.0 - INNER) / rows as f64;
        let caps = (0..rows)
            .map(|i| (std::f64::consts::PI * (INNER + d * (i as f64 + 0.5)) / d).floor() as u32 + 1)
            .collect();
        (d, caps)
    };
    let mut rows = 1;
    while rows < 80 && row_caps(rows).1.iter().sum::<u32>() < total {
        rows += 1;
    }
    let (d, caps) = row_caps(rows);
    let cap_total: u32 = caps.iter().sum::<u32>().max(1);
    // Seats per row in proportion to each row's room, largest remainders first.
    let mut per: Vec<u32> = caps.iter().map(|c| (total as u64 * *c as u64 / cap_total as u64) as u32).collect();
    let mut left = total.saturating_sub(per.iter().sum());
    let mut by_rest: Vec<usize> = (0..caps.len()).collect();
    by_rest.sort_by_key(|&i| std::cmp::Reverse((total as u64 * caps[i] as u64 % cap_total as u64, i)));
    for &i in by_rest.iter().cycle().take(caps.len() * 4) {
        if left == 0 {
            break;
        }
        if per[i] < caps[i] {
            per[i] += 1;
            left -= 1;
        }
    }
    let mut seats: Vec<(f64, f64)> = Vec::with_capacity(total as usize);
    for (i, &k) in per.iter().enumerate() {
        let r = INNER + d * (i as f64 + 0.5);
        for j in 0..k {
            let a = if k > 1 {
                std::f64::consts::PI * (1.0 - j as f64 / (k - 1) as f64)
            } else {
                std::f64::consts::FRAC_PI_2
            };
            seats.push((a, r));
        }
    }
    seats.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.total_cmp(&b.1)));
    (seats, d)
}

fn parliament_chart(svg: &mut String, chart: &ChartContent, w: f64, h: f64, f: f64, steps: Option<SeriesSteps>) {
    let seats = parliament_seats(chart);
    let total: u32 = seats.iter().sum();
    if total == 0 {
        return;
    }
    let (layout, d) = parliament_layout(total);
    let margin = f * 0.5;
    let radius = ((w / 2.0) - margin).min(h - margin * 2.0).max(4.0);
    let (cx, cy) = (w / 2.0, (h + radius) / 2.0);
    let dot = radius * d * 0.4;
    let mut next = layout.iter();
    let mut step = 0;
    for (i, &n) in seats.iter().enumerate() {
        if n == 0 {
            continue;
        }
        open_step(svg, steps, step);
        step += 1;
        let color = slice_color(i);
        for &(a, r) in next.by_ref().take(n as usize) {
            let _ = write!(
                svg,
                r#"<circle cx="{}" cy="{}" r="{}" style="fill:{}"/>"#,
                num(cx + radius * r * a.cos()),
                num(cy - radius * r * a.sin()),
                num(dot),
                esc(&color)
            );
        }
        close_step(svg, steps);
    }
    if chart.show_values {
        let _ = write!(
            svg,
            r#"<text x="{}" y="{}" text-anchor="middle" font-size="{}" font-weight="700">{}</text>"#,
            num(cx),
            num(cy - dot),
            num(f * 1.6),
            total
        );
    }
}

// ---------------------------------------------------------------------------
// Candlestick: open, high, low and close (the first four series) per row
// ---------------------------------------------------------------------------

const RISING: &str = "var(--cz-chart-up, #16a34a)";
const FALLING: &str = "var(--cz-chart-down, #dc2626)";

fn candle_chart(svg: &mut String, chart: &ChartContent, w: f64, h: f64, f: f64) {
    let n = rows(chart);
    let get = |s: usize, i: usize| chart.series.get(s).and_then(|x| x.values.get(i)).copied().filter(|v| v.is_finite());
    let candles: Vec<Option<[f64; 4]>> = (0..n).map(|i| Some([get(0, i)?, get(1, i)?, get(2, i)?, get(3, i)?])).collect();
    let all = candles.iter().flatten().flat_map(|c| c.iter().copied());
    let Some((lo, hi)) = all.fold(None, |r, v| Some(r.map_or((v, v), |(a, b): (f64, f64)| (a.min(v), b.max(v))))) else {
        return;
    };
    let range = nice_range(lo, hi, 5);
    let left = y_label_room(range, f);
    let plot = Rect {
        x: left,
        y: f * 0.9,
        w: (w - left - f * 0.6).max(10.0),
        h: (h - f * 2.7).max(10.0),
    };
    y_axis(svg, chart, plot, range, f);
    let band = plot.w / n as f64;
    let center = |i: usize| plot.x + (i as f64 + 0.5) * band;
    x_categories(svg, chart, plot, n, center, band, f);
    let y = |v: f64| plot.y + plot.h - (v - range.0) / (range.1 - range.0) * plot.h;
    for (i, c) in candles.iter().enumerate() {
        let Some([open, high, low, close]) = *c else { continue };
        let color = if close >= open { RISING } else { FALLING };
        let (top, bottom) = (high.max(open).max(close), low.min(open).min(close));
        let x = center(i);
        let _ = write!(
            svg,
            r#"<line x1="{x}" y1="{}" x2="{x}" y2="{}" style="stroke:{}" stroke-width="{}"/>"#,
            num(y(top)),
            num(y(bottom)),
            color,
            num((f * 0.08).max(1.5)),
            x = num(x)
        );
        let bw = band * 0.55;
        let (b0, b1) = (y(open.max(close)), y(open.min(close)));
        let _ = write!(
            svg,
            r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" style="fill:{}"/>"#,
            num(x - bw / 2.0),
            num(b0),
            num(bw),
            num((b1 - b0).max(1.5)),
            num((bw * 0.08).min(3.0)),
            color
        );
        if chart.show_values {
            let _ = write!(
                svg,
                r#"<text x="{}" y="{}" text-anchor="middle" font-size="{}">{}</text>"#,
                num(x),
                num(y(top) - f * 0.4),
                num(f * 0.7),
                fmt_value(close)
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Sankey diagrams and networks: links between named nodes
// ---------------------------------------------------------------------------

/// Nodes (named in order of first appearance) and the links between two
/// different nodes. Sankey flows need a positive value; network links
/// without one weigh 1.
struct Graph {
    names: Vec<String>,
    links: Vec<(usize, usize, f64)>,
}

fn graph(chart: &ChartContent, flows: bool) -> Graph {
    let mut names: Vec<String> = Vec::new();
    let mut links = Vec::new();
    let node = |name: &str, names: &mut Vec<String>| match names.iter().position(|n| n == name) {
        Some(i) => i,
        None => {
            names.push(name.to_string());
            names.len() - 1
        }
    };
    for l in &chart.links {
        let (a, b) = (l.from.trim(), l.to.trim());
        let v = if l.value.is_finite() && l.value > 0.0 {
            l.value
        } else if flows {
            continue;
        } else {
            1.0
        };
        if a.is_empty() || b.is_empty() || a == b {
            continue;
        }
        let (s, t) = (node(a, &mut names), node(b, &mut names));
        links.push((s, t, v));
    }
    Graph { names, links }
}

/// The column of each node of a Sankey diagram: one after the nodes that
/// flow into it, and nodes nothing flows out of in the last column.
/// (Mirrored in ui/js/charts.js to count build steps.)
fn sankey_columns(g: &Graph) -> Vec<usize> {
    let n = g.names.len();
    let mut col = vec![0usize; n];
    // A cycle stops growing at the last possible column.
    for _ in 0..n {
        let mut changed = false;
        for &(s, t, _) in &g.links {
            if col[t] < col[s] + 1 && col[s] + 1 < n {
                col[t] = col[s] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let last = col.iter().copied().max().unwrap_or(0);
    for (i, c) in col.iter_mut().enumerate() {
        if !g.links.iter().any(|l| l.0 == i) {
            *c = last;
        }
    }
    col
}

fn sankey_chart(svg: &mut String, chart: &ChartContent, w: f64, h: f64, f: f64, steps: Option<SeriesSteps>) {
    let g = graph(chart, true);
    let n = g.names.len();
    if g.links.is_empty() {
        return;
    }
    let col = sankey_columns(&g);
    let last = col.iter().copied().max().unwrap_or(0).max(1);
    let inflow: Vec<f64> = (0..n).map(|i| g.links.iter().filter(|l| l.1 == i).map(|l| l.2).sum()).collect();
    let outflow: Vec<f64> = (0..n).map(|i| g.links.iter().filter(|l| l.0 == i).map(|l| l.2).sum()).collect();
    let value: Vec<f64> = (0..n).map(|i| inflow[i].max(outflow[i])).collect();
    let columns: Vec<Vec<usize>> = (0..=last).map(|c| (0..n).filter(|&i| col[i] == c).collect()).collect();
    let pad = (f * 0.6).min(h * 0.08);
    let nw = (f * 0.7).min(w * 0.04).max(4.0);
    let top = f * 0.2;
    let height = (h - top * 2.0).max(10.0);
    // One scale for all columns: the fullest column fills the height.
    let ky = columns
        .iter()
        .filter(|c| !c.is_empty())
        .map(|c| (height - pad * (c.len() - 1) as f64) / c.iter().map(|&i| value[i]).sum::<f64>())
        .fold(f64::MAX, f64::min)
        .max(0.0);
    let x = |c: usize| f * 0.2 + c as f64 * (w - f * 0.4 - nw) / last as f64;
    let hgt: Vec<f64> = value.iter().map(|v| (v * ky).max(1.0)).collect();
    let mut y = vec![top; n];
    for c in &columns {
        let used: f64 = c.iter().map(|&i| hgt[i]).sum::<f64>() + pad * c.len().saturating_sub(1) as f64;
        let mut at = top + (height - used) / 2.0;
        for &i in c {
            y[i] = at;
            at += hgt[i] + pad;
        }
    }
    // Move nodes toward the nodes they are linked with, then keep them
    // apart and inside the chart (as d3-sankey does).
    let settle = |y: &mut Vec<f64>, c: &[usize]| {
        let mut order = c.to_vec();
        order.sort_by(|a, b| y[*a].total_cmp(&y[*b]));
        let mut at = top;
        for &i in &order {
            y[i] = y[i].max(at);
            at = y[i] + hgt[i] + pad;
        }
        let mut at = top + height;
        for &i in order.iter().rev() {
            y[i] = y[i].min(at - hgt[i]).max(top);
            at = y[i] - pad;
        }
    };
    for round in 0..8 {
        let alpha = 0.99f64.powi(round);
        for (k, c) in columns.iter().enumerate().skip(1).chain(columns.iter().enumerate().rev().skip(1)) {
            for &i in c {
                let (mut sum, mut wt) = (0.0, 0.0);
                for &(s, t, v) in &g.links {
                    let other = if t == i && col[s] < k {
                        s
                    } else if s == i && col[t] > k {
                        t
                    } else {
                        continue;
                    };
                    sum += (y[other] + hgt[other] / 2.0) * v;
                    wt += v;
                }
                if wt > 0.0 {
                    y[i] += (sum / wt - (y[i] + hgt[i] / 2.0)) * alpha;
                }
            }
            settle(&mut y, c);
        }
    }
    // Where each link leaves its source and enters its target, ordered by
    // the other end's position so that bands do not cross at the nodes.
    let mut out_at = vec![0.0f64; g.links.len()];
    let mut in_at = vec![0.0f64; g.links.len()];
    for i in 0..n {
        let mut outs: Vec<usize> = (0..g.links.len()).filter(|&k| g.links[k].0 == i).collect();
        outs.sort_by(|a, b| y[g.links[*a].1].total_cmp(&y[g.links[*b].1]));
        let mut at = y[i];
        for k in outs {
            out_at[k] = at;
            at += g.links[k].2 * ky;
        }
        let mut ins: Vec<usize> = (0..g.links.len()).filter(|&k| g.links[k].1 == i).collect();
        ins.sort_by(|a, b| y[g.links[*a].0].total_cmp(&y[g.links[*b].0]));
        let mut at = y[i];
        for k in ins {
            in_at[k] = at;
            at += g.links[k].2 * ky;
        }
    }
    let node = |svg: &mut String, i: usize| {
        let _ = write!(
            svg,
            r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" style="fill:{}"/>"#,
            num(x(col[i])),
            num(y[i]),
            num(nw),
            num(hgt[i]),
            num((nw * 0.2).min(3.0)),
            esc(&slice_color(i))
        );
        // Labels face inward: after the node, before it in the last column.
        let right = col[i] < last;
        let _ = write!(
            svg,
            r#"<text x="{}" y="{}" text-anchor="{}" dominant-baseline="central" font-size="{}">{}"#,
            num(if right { x(col[i]) + nw + f * 0.35 } else { x(col[i]) - f * 0.35 }),
            num(y[i] + hgt[i] / 2.0),
            if right { "start" } else { "end" },
            num(f * 0.8),
            esc(&g.names[i])
        );
        if chart.show_values {
            let _ = write!(svg, r#" <tspan opacity="0.7">{}</tspan>"#, fmt_value(value[i]));
        }
        svg.push_str("</text>");
    };
    for &i in &columns[0] {
        node(svg, i);
    }
    // Build by column: each column arrives with the flows that reach it.
    for (c, nodes) in columns.iter().enumerate().skip(1) {
        open_step(svg, steps, c as u32 - 1);
        for (k, &(s, t, v)) in g.links.iter().enumerate() {
            if col[t] != c {
                continue;
            }
            let (x0, x1) = (x(col[s]) + nw, x(col[t]));
            let xm = (x0 + x1) / 2.0;
            let (a0, a1, b) = (out_at[k], in_at[k], v * ky);
            let _ = write!(
                svg,
                r#"<path d="M{x0} {a0}C{xm} {a0} {xm} {a1} {x1} {a1}L{x1} {b1}C{xm} {b1} {xm} {b0} {x0} {b0}Z" style="fill:{}" fill-opacity="0.35"/>"#,
                esc(&slice_color(s)),
                x0 = num(x0),
                x1 = num(x1),
                xm = num(xm),
                a0 = num(a0),
                a1 = num(a1),
                b0 = num(a0 + b),
                b1 = num(a1 + b)
            );
        }
        for &i in nodes {
            node(svg, i);
        }
        close_step(svg, steps);
    }
}

/// Node positions of a network: a force-directed layout (Fruchterman and
/// Reingold) that starts from a circle, so the same data always gives the
/// same picture. Positions stay within ±`aspect` across and ±1 down.
fn network_layout(g: &Graph, aspect: f64) -> Vec<Point> {
    let n = g.names.len();
    let mut p: Vec<Point> = (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            (a.cos() * aspect, a.sin())
        })
        .collect();
    if n < 2 {
        return vec![(0.0, 0.0); n];
    }
    let k = (4.0 * aspect / n as f64).sqrt();
    let rounds = 300;
    for round in 0..rounds {
        let heat = 0.1 * (1.0 - round as f64 / rounds as f64) + 0.002;
        let mut d = vec![(0.0f64, 0.0f64); n];
        for i in 0..n {
            for j in i + 1..n {
                let (dx, dy) = (p[i].0 - p[j].0, p[i].1 - p[j].1);
                let dist = (dx * dx + dy * dy).sqrt().max(0.01);
                let push = k * k / dist;
                d[i].0 += dx / dist * push;
                d[i].1 += dy / dist * push;
                d[j].0 -= dx / dist * push;
                d[j].1 -= dy / dist * push;
            }
        }
        for &(i, j, _) in &g.links {
            let (dx, dy) = (p[i].0 - p[j].0, p[i].1 - p[j].1);
            let dist = (dx * dx + dy * dy).sqrt().max(0.01);
            let pull = dist * dist / k;
            d[i].0 -= dx / dist * pull;
            d[i].1 -= dy / dist * pull;
            d[j].0 += dx / dist * pull;
            d[j].1 += dy / dist * pull;
        }
        for i in 0..n {
            // A little gravity keeps separate groups in view.
            d[i].0 -= p[i].0 * k;
            d[i].1 -= p[i].1 * k;
            let len = (d[i].0 * d[i].0 + d[i].1 * d[i].1).sqrt().max(1e-9);
            let step = len.min(heat);
            // Nodes stay in a frame shaped like the chart, so lone nodes
            // cannot drift off and leave the rest small.
            p[i].0 = (p[i].0 + d[i].0 / len * step).clamp(-aspect, aspect);
            p[i].1 = (p[i].1 + d[i].1 / len * step).clamp(-1.0, 1.0);
        }
    }
    p
}

fn network_chart(svg: &mut String, chart: &ChartContent, w: f64, h: f64, f: f64, steps: Option<SeriesSteps>) {
    let mut g = graph(chart, false);
    g.names.truncate(300);
    g.links.retain(|l| l.0 < 300 && l.1 < 300);
    let n = g.names.len();
    if n == 0 {
        return;
    }
    let weight: Vec<f64> = (0..n).map(|i| g.links.iter().filter(|l| l.0 == i || l.1 == i).map(|l| l.2).sum()).collect();
    let heaviest = weight.iter().copied().fold(0.0, f64::max).max(1e-9);
    let radius: Vec<f64> = weight.iter().map(|wt| f * 0.35 + f * 0.6 * (wt / heaviest).sqrt()).collect();
    let rmax = radius.iter().copied().fold(0.0, f64::max);
    let layout = network_layout(&g, (w / h.max(1.0)).clamp(0.5, 3.0));
    let (x0, x1) = layout.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
    let (y0, y1) = layout.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
    // Room for the largest node and its label all around.
    let (mx, my) = (rmax + f * 2.5, rmax + f * 0.3);
    let span = |a: f64, b: f64| if b - a > 1e-9 { b - a } else { 1.0 };
    let sx = (w - mx * 2.0).max(1.0) / span(x0, x1);
    let sy = (h - my * 2.0 - f * 1.2).max(1.0) / span(y0, y1);
    let s = sx.min(sy);
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let pos: Vec<Point> = layout
        .iter()
        .map(|p| (w / 2.0 + (p.0 - cx) * s, (h - f * 1.2) / 2.0 + (p.1 - cy) * s))
        .collect();
    let thickest = g.links.iter().map(|l| l.2).fold(0.0, f64::max).max(1e-9);
    // Build node by node: each node arrives with its links to earlier ones.
    for i in 0..n {
        open_step(svg, steps, i as u32);
        for &(a, b, v) in g.links.iter().filter(|l| l.0.max(l.1) == i) {
            let (p, q) = (pos[a], pos[b]);
            let (dx, dy) = (q.0 - p.0, q.1 - p.1);
            let len = (dx * dx + dy * dy).sqrt().max(1e-9);
            // Lines stop at the circles, so later lines never cross nodes.
            let (ux, uy) = (dx / len, dy / len);
            let _ = write!(
                svg,
                r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="currentColor" stroke-opacity="0.4" stroke-width="{}" stroke-linecap="round"/>"#,
                num(p.0 + ux * radius[a]),
                num(p.1 + uy * radius[a]),
                num(q.0 - ux * radius[b]),
                num(q.1 - uy * radius[b]),
                num(1.0 + f * 0.25 * v / thickest)
            );
            if chart.show_values {
                let _ = write!(
                    svg,
                    r#"<text x="{}" y="{}" text-anchor="middle" dominant-baseline="central" font-size="{}" opacity="0.8">{}</text>"#,
                    num((p.0 + q.0) / 2.0),
                    num((p.1 + q.1) / 2.0),
                    num(f * 0.65),
                    fmt_value(v)
                );
            }
        }
        let _ = write!(
            svg,
            r#"<circle cx="{}" cy="{}" r="{}" style="fill:{}"/><text x="{}" y="{}" text-anchor="middle" font-size="{}">{}</text>"#,
            num(pos[i].0),
            num(pos[i].1),
            num(radius[i]),
            esc(&slice_color(i)),
            num(pos[i].0),
            num(pos[i].1 + radius[i] + f * 0.85),
            num(f * 0.75),
            esc(&g.names[i])
        );
        close_step(svg, steps);
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
                let c = ChartContent {
                    chart: kind.to_string(),
                    stacked,
                    show_values: true,
                    ..Default::default()
                };
                let svg = chart_svg(&c, 800.0, 450.0, None);
                assert!(svg.contains("<svg") && svg.ends_with("</svg>"), "{kind}");
                assert!(!svg.contains("NaN"), "{kind}");
            }
        }
    }

    fn series(values: &[f64]) -> crate::model::Series {
        crate::model::Series {
            values: values.to_vec(),
            ..Default::default()
        }
    }

    #[test]
    fn treemap_rectangles_tile_the_area() {
        let area = Rect {
            x: 10.0,
            y: 20.0,
            w: 800.0,
            h: 450.0,
        };
        let values = [6.0, 0.0, 3.0, 2.0, 2.0, 1.0, -4.0, 1.0];
        let rects = squarify(&values, area);
        assert!(rects[1].is_none() && rects[6].is_none(), "zero and negative values take no room");
        let sum: f64 = rects.iter().flatten().map(|r| r.w * r.h).sum();
        assert!((sum - area.w * area.h).abs() < 1e-6, "{sum}");
        let total: f64 = values.iter().filter(|v| **v > 0.0).sum();
        for (v, r) in values.iter().zip(&rects) {
            if let Some(r) = r {
                assert!((r.w * r.h / (area.w * area.h) - v / total).abs() < 1e-9, "areas follow the values");
                assert!(r.x >= area.x - 1e-9 && r.y >= area.y - 1e-9);
                assert!(r.x + r.w <= area.x + area.w + 1e-6 && r.y + r.h <= area.y + area.h + 1e-6);
            }
        }
    }

    #[test]
    fn parliament_draws_one_dot_per_seat() {
        for seats in [vec![1.0], vec![120.0, 95.0, 60.0, 40.0, 18.0], vec![3.4, 0.0, 2.6], vec![300.0, 213.0]] {
            let total: f64 = seats.iter().map(|v: &f64| v.round()).sum();
            let c = ChartContent {
                chart: "parliament".into(),
                labels: (0..seats.len()).map(|i| format!("P{i}")).collect(),
                series: vec![series(&seats)],
                ..Default::default()
            };
            let svg = chart_svg(&c, 800.0, 450.0, None);
            assert_eq!(svg.matches("<circle").count() as f64, total, "{seats:?}");
            let (layout, _) = parliament_layout(total as u32);
            assert!(layout.windows(2).all(|p| p[0].0 >= p[1].0), "seats run from left to right");
        }
    }

    #[test]
    fn smooth_curves_stay_within_their_points() {
        let pts = [(0.0, 0.0), (1.0, 10.0), (2.0, 10.0), (3.0, 2.0), (4.0, 2.5)];
        for (k, (c1, c2, end)) in monotone_segments(&pts).iter().enumerate() {
            let (lo, hi) = (pts[k].1.min(pts[k + 1].1), pts[k].1.max(pts[k + 1].1));
            for y in [c1.1, c2.1] {
                assert!(y >= lo - 1e-9 && y <= hi + 1e-9, "segment {k} overshoots: {y}");
            }
            assert_eq!(*end, pts[k + 1]);
        }
    }

    #[test]
    fn steps_follow_what_each_chart_builds_by() {
        let c = |chart: &str, s: Vec<crate::model::Series>| ChartContent {
            chart: chart.into(),
            series: s,
            ..Default::default()
        };
        let one = || vec![series(&[3.0, 0.0, 2.0, 1.0])];
        let three = || vec![series(&[1.0]), series(&[2.0]), series(&[3.0])];
        assert_eq!(series_steps(&c("parliament", one())), 3, "parties with seats");
        assert_eq!(series_steps(&c("treemap", one())), 3, "rectangles");
        assert_eq!(series_steps(&c("treemap", three())), 3, "groups");
        assert_eq!(series_steps(&c("scatter", three())), 2, "the first series is X");
        assert_eq!(series_steps(&c("scatter", one())), 1);
        assert_eq!(series_steps(&c("heatmap", three())), 3, "columns");
        assert_eq!(series_steps(&c("stacked-bar", three())), 3);
        assert_eq!(series_steps(&c("candlestick", three())), 0);
    }

    #[test]
    fn stacked_types_and_the_older_flag_draw_alike() {
        let new = chart_svg(
            &ChartContent {
                chart: "stacked-column".into(),
                ..Default::default()
            },
            800.0,
            450.0,
            None,
        );
        let old = chart_svg(
            &ChartContent {
                chart: "column".into(),
                stacked: true,
                ..Default::default()
            },
            800.0,
            450.0,
            None,
        );
        assert_eq!(new, old);
        assert_ne!(new, chart_svg(&ChartContent::default(), 800.0, 450.0, None));
    }

    fn flows(links: &[(&str, &str, f64)]) -> Vec<crate::model::Link> {
        links
            .iter()
            .map(|(a, b, v)| crate::model::Link {
                from: a.to_string(),
                to: b.to_string(),
                value: *v,
            })
            .collect()
    }

    #[test]
    fn sankey_nodes_follow_their_flows() {
        let c = ChartContent {
            chart: "sankey".into(),
            links: flows(&[
                ("S1", "M1", 30.0),
                ("S2", "M1", 20.0),
                ("S2", "M2", 15.0),
                ("S3", "M2", 25.0),
                ("M1", "T1", 35.0),
                ("M1", "T2", 15.0),
                ("M2", "T2", 30.0),
                ("S3", "T3", 10.0),
                ("", "X", 5.0),
                ("Y", "Y", 5.0),
                ("Z", "W", 0.0),
            ]),
            ..Default::default()
        };
        let g = graph(&c, true);
        assert_eq!(
            g.names,
            ["S1", "M1", "S2", "M2", "S3", "T1", "T2", "T3"],
            "blank, looping and empty flows are left out"
        );
        // T3 is fed straight from a source but still ends in the last column.
        assert_eq!(sankey_columns(&g), [0, 1, 0, 1, 0, 2, 2, 2]);
        assert_eq!(series_steps(&c), 2);
        let svg = chart_svg(&c, 800.0, 450.0, Some(SeriesSteps { effect: "fade-in", first: 1 }));
        assert_eq!(svg.matches("class=\"fragment").count(), 2, "one step per column after the first");
        assert_eq!(svg.matches("<rect").count(), 8, "one bar per node");
        assert!(!svg.contains("NaN") && !svg.contains("inf"));

        let cycle = ChartContent {
            chart: "sankey".into(),
            links: flows(&[("A", "B", 1.0), ("B", "C", 1.0), ("C", "A", 1.0)]),
            ..Default::default()
        };
        assert!(sankey_columns(&graph(&cycle, true)).iter().all(|c| *c < 3), "a cycle stops growing");
        assert!(chart_svg(&cycle, 400.0, 300.0, None).ends_with("</svg>"));
    }

    #[test]
    fn networks_lay_out_the_same_way_every_time() {
        let c = ChartContent {
            chart: "network".into(),
            links: flows(&[("A", "B", 3.0), ("A", "C", 0.0), ("B", "C", 2.0), ("C", "D", 1.0), ("E", "F", 1.0)]),
            ..Default::default()
        };
        let svg = chart_svg(&c, 800.0, 450.0, None);
        assert_eq!(svg, chart_svg(&c, 800.0, 450.0, None));
        assert_eq!(svg.matches("<circle").count(), 6, "links without a value still connect");
        assert_eq!(svg.matches("<line").count(), 5);
        assert!(!svg.contains("NaN") && !svg.contains("inf"));
        assert_eq!(series_steps(&c), 6, "node by node");
        let layout = network_layout(&graph(&c, false), 16.0 / 9.0);
        for (i, p) in layout.iter().enumerate() {
            for q in &layout[i + 1..] {
                assert!((p.0 - q.0).hypot(p.1 - q.1) > 0.05, "nodes keep apart");
            }
        }
    }

    #[test]
    fn empty_chart_does_not_panic() {
        for (kind, _) in CHART_TYPES {
            for (labels, series_) in [
                (vec![], vec![]),
                (vec!["A".to_string()], vec![series(&[5.0])]),
                (vec!["A".to_string()], vec![series(&[f64::NAN])]),
            ] {
                let c = ChartContent {
                    chart: kind.to_string(),
                    labels,
                    series: series_,
                    ..Default::default()
                };
                let svg = chart_svg(&c, 100.0, 100.0, None);
                assert!(svg.ends_with("</svg>") && !svg.contains("NaN") && !svg.contains("inf"), "{kind}: {svg}");
            }
        }
    }

    #[test]
    fn builds_one_series_or_slice_per_step() {
        let steps = Some(SeriesSteps {
            effect: "cz-wipe-up",
            first: 3,
        });
        for kind in ["column", "bar", "line", "area"] {
            let c = ChartContent {
                chart: kind.into(),
                ..Default::default()
            };
            let svg = chart_svg(&c, 800.0, 450.0, steps);
            for k in 0..c.series.len() {
                assert!(
                    svg.contains(&format!(r#"<g class="fragment cz-wipe-up" data-fragment-index="{}">"#, 3 + k)),
                    "{kind}"
                );
            }
            assert_eq!(svg.matches("<g ").count(), svg.matches("</g>").count(), "{kind}");
            assert!(!chart_svg(&c, 800.0, 450.0, None).contains("fragment"), "{kind}");
        }
        let pie = ChartContent {
            chart: "pie".into(),
            series: vec![crate::model::Series {
                values: vec![3.0, 0.0, 2.0],
                ..Default::default()
            }],
            ..Default::default()
        };
        let svg = chart_svg(&pie, 400.0, 400.0, steps);
        assert_eq!(svg.matches("class=\"fragment").count(), 2, "zero slices take no step");
        assert!(svg.contains(r#"data-fragment-index="4""#));
    }
}
