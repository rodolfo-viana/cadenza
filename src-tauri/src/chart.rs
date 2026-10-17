//! Charts rendered to static SVG.
//!
//! Charts are drawn in Rust so that they look identical in the editor, in the
//! live presentation and in exported files, without any JavaScript library.
//! Colors default to the theme palette through CSS variables.

use std::fmt::Write as _;

use crate::model::ChartContent;
use crate::util::{css_value, esc};

/// A number of the SVG. `util::num` writes what is not finite as 0, which
/// would hide broken geometry: debug builds (and tests) stop at it instead.
fn num(v: f64) -> String {
    debug_assert!(v.is_finite(), "a chart computed {v}");
    crate::util::num(v)
}

/// The largest size charts compute with: larger values are drawn as if they
/// were this large, so that sums, spans and rounding out to nice ends stay
/// finite.
const LIMIT: f64 = 1e300;

fn capped(v: f64) -> f64 {
    v.clamp(-LIMIT, LIMIT)
}

pub const CHART_TYPES: &[(&str, &str)] = &[
    ("column", "Column"),
    ("stacked-column", "Stacked column"),
    ("bar", "Bar"),
    ("stacked-bar", "Stacked bar"),
    ("line", "Line"),
    ("dot", "Dot plot"),
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
    chart.series.first().map_or(0, |s| s.numbers().filter(|v| *v > 0.0).count() as u32)
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
    // Ends too small to tell from zero count as zero, and ends too close
    // together for their size get room around them: the step stays a
    // normal number that splits the span into a few parts.
    let end = |v: f64| if v.abs() < 1.0 / LIMIT { 0.0 } else { capped(v) };
    let (mut lo, mut hi) = (end(min), end(max));
    if (hi - lo).abs() < f64::EPSILON.max(lo.abs().max(hi.abs()) * 1e-12) {
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

/// A data value as labels show it: two decimals at most (in thousands from
/// 10,000, in millions from a million), but never fewer than two
/// significant digits, and in scientific notation when tiny or huge.
pub(crate) fn fmt_value(v: f64) -> String {
    let a = v.abs();
    if a == 0.0 {
        "0".into()
    } else if !(1e-4..1e15).contains(&a) {
        sci(v, 2)
    } else if a >= 1_000_000.0 {
        format!("{}M", num(v / 1_000_000.0))
    } else if a >= 10_000.0 {
        format!("{}k", num(v / 1000.0))
    } else if a >= 0.1 {
        num(v)
    } else {
        let decimals = (1.0 - a.log10().floor()) as usize;
        let text = format!("{v:.decimals$}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// `v` in scientific notation, with at most `decimals` decimals ("2.5e-7").
fn sci(v: f64, decimals: usize) -> String {
    let text = format!("{v:.decimals$e}");
    let (mantissa, exp) = text.split_once('e').unwrap_or((&text, "0"));
    let mantissa = if mantissa.contains('.') {
        mantissa.trim_end_matches('0').trim_end_matches('.')
    } else {
        mantissa
    };
    format!("{mantissa}e{exp}")
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
    // Lines, dot plots and scatterplots show their series' points.
    let points = matches!(kind, "line" | "dot" | "scatter");
    let legend_items: Vec<(String, String)> = match kind {
        "heatmap" | "candlestick" | "sankey" | "network" | "map" => Vec::new(),
        "parliament" if chart.show_values => {
            let seats = parliament_seats(chart);
            chart
                .labels
                .iter()
                .enumerate()
                .map(|(i, l)| (format!("{l} ({})", fmt_value(seats.get(i).copied().unwrap_or(0.0))), slice_color(i)))
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
        "dot" => dot_chart(&mut svg, chart, w, plot_h, f, steps),
        "heatmap" => heatmap_chart(&mut svg, chart, w, plot_h, f, steps),
        "streamgraph" => stream_chart(&mut svg, chart, w, plot_h, f, steps),
        "candlestick" => candle_chart(&mut svg, chart, w, plot_h, f),
        "sankey" => sankey_chart(&mut svg, chart, w, plot_h, f, steps),
        "network" => network_chart(&mut svg, chart, w, plot_h, f, steps),
        "map" => {
            let values = chart.series.first().map(|s| s.plain()).unwrap_or_default();
            let color = scale_color(chart);
            scale = crate::maps::map_chart(&mut svg, &chart.map, &chart.labels, &values, &color, chart.show_values, (w, plot_h, f));
        }
        _ => axis_chart(&mut svg, chart, w, plot_h, f, steps),
    }
    if legend_h > 0.0 {
        if scale_legend {
            if let Some(range) = scale {
                heat_legend(&mut svg, range, &scale_color(chart), w, h - legend_h * 0.5, f);
            }
        } else {
            let first = usize::from(kind == "scatter" && chart.series.len() > 1);
            let shapes: Vec<Option<&str>> = (0..legend_items.len())
                .map(|k| points.then(|| chart.series.get(k + first).map_or("", |s| s.marker.as_str())))
                .collect();
            legend(&mut svg, &legend_items, &shapes, w, h - legend_h * 0.5, f);
        }
    }
    svg.push_str("</svg>");
    svg
}

/// Rows of data: category labels, or the longest series.
fn rows(chart: &ChartContent) -> usize {
    chart.labels.len().max(chart.series.iter().map(|s| s.values.len()).max().unwrap_or(0))
}

/// Legend items in a row: a swatch (or the series' point `shapes`) and a name.
fn legend(svg: &mut String, items: &[(String, String)], shapes: &[Option<&str>], w: f64, cy: f64, f: f64) {
    // Approximate text width: 0.55em per character.
    let widths: Vec<f64> = items.iter().map(|(l, _)| f * 1.4 + l.chars().count() as f64 * f * 0.55 + f).collect();
    let total: f64 = widths.iter().sum();
    let mut x = ((w - total) / 2.0).max(0.0);
    for (k, ((label, color), iw)) in items.iter().zip(widths).enumerate() {
        let s = f * 0.8;
        match shapes.get(k).copied().flatten() {
            Some(shape) => marker(svg, shape, (x + s / 2.0, cy), s * 0.45, color, ""),
            None => {
                let _ = write!(
                    svg,
                    r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" style="fill:{}"/>"#,
                    num(x),
                    num(cy - s / 2.0),
                    num(s),
                    num(s),
                    num(s * 0.25),
                    esc(color)
                );
            }
        }
        let _ = write!(
            svg,
            r#"<text x="{}" y="{}" dominant-baseline="central">{}</text>"#,
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
    let ns = chart.series.len();
    let label = |s: usize, i: usize| fmt_value(chart.series[s].value(i).unwrap_or(0.0));
    let value = |s: usize, i: usize| capped(chart.series[s].value(i).unwrap_or(0.0));
    // Rows without a value get no point, bar or error bar (areas still
    // pass through zero there).
    let has = |s: usize, i: usize| chart.series[s].value(i).is_some();
    // Where each value starts and ends along the value axis. Stacked columns
    // and bars pile positive and negative values apart; areas pile them all.
    let mut seg = vec![vec![(0.0f64, 0.0f64); n]; ns];
    let (mut pos, mut neg) = (vec![0.0f64; n], vec![0.0f64; n]);
    for (s, row) in seg.iter_mut().enumerate() {
        for (i, cell) in row.iter_mut().enumerate() {
            let v = value(s, i);
            *cell = if !stacked {
                (0.0, v)
            } else if kind == "area" || v >= 0.0 {
                let st = pos[i];
                pos[i] += v;
                (st, st + v)
            } else {
                let st = neg[i];
                neg[i] += v;
                (st, st + v)
            };
        }
    }
    let errors = kind != "area";
    // Error bars around where a value ends (a stacked value's top).
    let error_at = |s: usize, i: usize| -> Option<(f64, f64)> {
        if !errors {
            return None;
        }
        if !has(s, i) {
            return None;
        }
        let v = value(s, i);
        let (a, b) = error_span(chart, s, i, v)?;
        let end = seg[s][i].1;
        Some((a - v + end, b - v + end))
    };
    let mut range = (0.0f64, 0.0f64);
    for (s, row) in seg.iter().enumerate() {
        for (i, &(st, en)) in row.iter().enumerate() {
            range = (range.0.min(st).min(en), range.1.max(st).max(en));
            if let Some((a, b)) = error_at(s, i) {
                range = (range.0.min(a), range.1.max(b));
            }
        }
    }
    let axis = value_axis(chart, Some(range), true);
    let max_label = axis.ticks().map(|v| fmt_tick(v, axis.step).chars().count()).max().unwrap_or(1) as f64;
    let max_cat = chart.labels.iter().map(|l| l.chars().count()).max().unwrap_or(1) as f64;
    let (title_left, title_bottom) = title_room(chart, f);
    let (left, top, right) = if horizontal {
        (title_left + (max_cat * f * 0.58 + f * 0.8).min(w * 0.35), f * 0.8, f * 1.2)
    } else {
        (title_left + max_label * f * 0.6 + f * 0.8, f * 0.9, f * 0.6)
    };
    let pw = (w - left - right).max(10.0);
    // Labels too long for their room wrap onto more lines.
    let (cat_chars, cat_lines) = if horizontal {
        let chars = ((left - title_left - f * 0.8) / (f * 0.58)).floor().max(1.0) as usize;
        let fit = ((h - top - f * 1.8 - title_bottom) / n as f64 / (f * 1.1)).floor().clamp(1.0, 3.0) as usize;
        (chars, fit)
    } else {
        let chars = (pw / n as f64 / (f * 0.6)).floor().max(1.0) as usize;
        (chars, category_lines(chart, chars, 2))
    };
    let below = if horizontal { 0.0 } else { (cat_lines - 1) as f64 * f * 1.1 };
    let bottom = f * 1.8 + below + title_bottom;
    let plot = Rect {
        x: left,
        y: top,
        w: pw,
        h: (h - top - bottom).max(10.0),
    };
    let (px, py, pw, ph) = (plot.x, plot.y, plot.w, plot.h);
    // Position along the value axis.
    let vpos = |v: f64| -> f64 {
        let t = axis.t(v);
        if horizontal { px + t * pw } else { py + ph - t * ph }
    };

    // Grid + value labels.
    for v in axis.ticks() {
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
                fmt_tick(v, axis.step)
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
                fmt_tick(v, axis.step)
            );
        }
    }
    // Baseline: zero, or the end of the axis nearest to it.
    let zero = vpos(0.0f64.max(axis.lo).min(axis.hi));
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
        let lines = wrap_label(label, cat_chars, cat_lines);
        let c = (i as f64 + 0.5) * band;
        if horizontal {
            label_text(svg, (px - f * 0.5, py + c), &lines, "end", true);
        } else {
            label_text(svg, (px + c, py + ph + f * 1.3), &lines, "middle", false);
        }
    }

    let clip = open_clip(svg, chart, plot);
    match kind {
        "line" | "area" => {
            let area = kind == "area";
            let r = (f * 0.22).max(3.0);
            for (s, row) in seg.iter().enumerate() {
                open_step(svg, steps, s as u32);
                let color = series_color(chart, s);
                let pts: Vec<(f64, f64, f64)> = row
                    .iter()
                    .enumerate()
                    .map(|(i, &(start, end))| (px + (i as f64 + 0.5) * band, vpos(end), vpos(start)))
                    .collect();
                // Lines and areas break where a value is missing; stacked
                // areas pass through zero there, under the layers above.
                let mut runs: Vec<Vec<usize>> = Vec::new();
                for k in (0..pts.len()).filter(|&k| stacked || has(s, k)) {
                    match runs.last_mut() {
                        Some(run) if run.last().is_some_and(|&j| j + 1 == k) => run.push(k),
                        _ => runs.push(vec![k]),
                    }
                }
                let trace = |run: &[usize]| -> String {
                    run.iter()
                        .enumerate()
                        .map(|(n, &k)| format!("{}{} {}", if n == 0 { "M" } else { "L" }, num(pts[k].0), num(pts[k].1)))
                        .collect()
                };
                if area {
                    let fill: String = runs
                        .iter()
                        .map(|run| {
                            let back: String = run.iter().rev().map(|&k| format!("L{} {}", num(pts[k].0), num(pts[k].2))).collect();
                            format!("{}{back}Z", trace(run))
                        })
                        .collect();
                    let _ = write!(svg, r#"<path d="{fill}" style="fill:{c};fill-opacity:0.28"/>"#, c = esc(&color));
                }
                let line: String = runs.iter().map(|run| trace(run)).collect();
                let _ = write!(
                    svg,
                    r#"<path d="{}" fill="none" style="stroke:{}" stroke-width="{}" stroke-linejoin="round" stroke-linecap="round"/>"#,
                    line,
                    esc(&color),
                    num((f * 0.18).max(2.0))
                );
                for (k, (x, y, _)) in pts.iter().enumerate() {
                    if !has(s, k) {
                        continue;
                    }
                    if let Some((a, b)) = error_at(s, k) {
                        error_bar(svg, false, *x, (vpos(a), vpos(b)), r * 1.4, f);
                    }
                    marker(svg, &chart.series[s].marker, (*x, *y), r, &color, "");
                    if chart.show_values {
                        let _ = write!(
                            svg,
                            r#"<text x="{}" y="{}" text-anchor="middle" font-size="{}">{}</text>"#,
                            num(*x),
                            num(y - f * 0.6),
                            num(f * 0.8),
                            label(s, k)
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
            for (s, row) in seg.iter().enumerate() {
                open_step(svg, steps, s as u32);
                let color = series_color(chart, s);
                for (i, &(start, end)) in row.iter().enumerate() {
                    if !has(s, i) {
                        continue;
                    }
                    let v = value(s, i);
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
                    let across = if horizontal { py } else { px } + offset + bw / 2.0;
                    if let Some((ea, eb)) = error_at(s, i) {
                        error_bar(svg, horizontal, across, (vpos(ea), vpos(eb)), (bw * 0.25).min(f * 0.5), f);
                    }
                    if chart.show_values && v != 0.0 {
                        let text = label(s, i);
                        if horizontal {
                            let _ = write!(
                                svg,
                                r#"<text x="{}" y="{}" dominant-baseline="central" font-size="{}">{}</text>"#,
                                num(x + rw + f * 0.3),
                                num(y + rh / 2.0),
                                num(f * 0.8),
                                text
                            );
                        } else {
                            let _ = write!(
                                svg,
                                r#"<text x="{}" y="{}" text-anchor="middle" font-size="{}">{}</text>"#,
                                num(x + rw / 2.0),
                                num(y - f * 0.35),
                                num(f * 0.8),
                                text
                            );
                        }
                    }
                }
                close_step(svg, steps);
            }
        }
    }
    close_clip(svg, clip);
    ref_lines(svg, chart, plot, axis, horizontal, f);
    axis_titles(svg, chart, plot, below, f);
}

// ---------------------------------------------------------------------------
// Dot plot: each series a point per category, side by side
// ---------------------------------------------------------------------------

fn dot_chart(svg: &mut String, chart: &ChartContent, w: f64, h: f64, f: f64, steps: Option<SeriesSteps>) {
    let n = rows(chart);
    let ns = chart.series.len();
    if n == 0 || ns == 0 {
        return;
    }
    let get = |s: usize, i: usize| chart.series[s].value(i);
    let mut range: Option<(f64, f64)> = None;
    let mut extend = |a: f64, b: f64| range = Some(range.map_or((a, b), |(lo, hi)| (lo.min(a), hi.max(b))));
    for s in 0..ns {
        for i in 0..n {
            if let Some(v) = get(s, i) {
                let (a, b) = error_span(chart, s, i, v).unwrap_or((v, v));
                extend(a, b);
            }
        }
    }
    let axis = value_axis(chart, range, false);
    let (title_left, title_bottom) = title_room(chart, f);
    let left = title_left + y_label_room(axis, f);
    let pw = (w - left - f * 0.6).max(10.0);
    let chars = (pw / n as f64 / (f * 0.6)).floor().max(1.0) as usize;
    let lines = category_lines(chart, chars, 2);
    let below = (lines - 1) as f64 * f * 1.1;
    let plot = Rect {
        x: left,
        y: f * 0.9,
        w: pw,
        h: (h - f * 2.7 - below - title_bottom).max(10.0),
    };
    y_axis(svg, chart, plot, axis, f);
    plot_frame(svg, plot);
    let band = plot.w / n as f64;
    let center = |i: usize| plot.x + (i as f64 + 0.5) * band;
    for i in 0..n {
        let label = chart.labels.get(i).map(String::as_str).unwrap_or("");
        label_text(svg, (center(i), plot.y + plot.h + f * 1.3), &wrap_label(label, chars, lines), "middle", false);
    }
    let y = |v: f64| plot.y + plot.h - axis.t(v) * plot.h;
    // Series side by side within a category, so that equal values stay apart;
    // only those with a value there take room.
    let spacing = if ns > 1 { (band * 0.6 / ns as f64).min(f * 1.4) } else { 0.0 };
    let dx = |s: usize, i: usize| {
        let shown = (0..ns).filter(|&k| get(k, i).is_some());
        let (place, count) = shown.fold((0, 0), |(p, c), k| (p + usize::from(k < s), c + 1));
        (place as f64 - (count as f64 - 1.0) / 2.0) * spacing
    };
    let r = (f * 0.3).max(3.0);
    let clip = open_clip(svg, chart, plot);
    for s in 0..ns {
        open_step(svg, steps, s as u32);
        let color = series_color(chart, s);
        for i in 0..n {
            let Some(v) = get(s, i) else { continue };
            let x = center(i) + dx(s, i);
            if let Some((a, b)) = error_span(chart, s, i, v) {
                error_bar(svg, false, x, (y(a), y(b)), r * 1.1, f);
            }
            marker(svg, &chart.series[s].marker, (x, y(v)), r, &color, "");
            if chart.show_values {
                let _ = write!(
                    svg,
                    r#"<text x="{}" y="{}" dominant-baseline="central" font-size="{}">{}</text>"#,
                    num(x + r + f * 0.2),
                    num(y(v)),
                    num(f * 0.7),
                    fmt_value(v)
                );
            }
        }
        close_step(svg, steps);
    }
    close_clip(svg, clip);
    ref_lines(svg, chart, plot, axis, false, f);
    axis_titles(svg, chart, plot, below, f);
}

// ---------------------------------------------------------------------------
// Parts shared by charts with axes
// ---------------------------------------------------------------------------

/// A value axis from `lo` to `hi`, with labels every `step`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Axis {
    lo: f64,
    hi: f64,
    step: f64,
}

impl Axis {
    fn of((lo, hi, step): (f64, f64, f64)) -> Axis {
        Axis { lo, hi, step }
    }

    /// Where `v` falls between the ends: 0 at `lo`, 1 at `hi`. Values far
    /// beyond ends of its own (cut off anyway) stop a million spans out.
    fn t(self, v: f64) -> f64 {
        ((capped(v) - self.lo) / (self.hi - self.lo)).clamp(-1e6, 1e6)
    }

    /// The multiples of the step from one end to the other.
    fn ticks(self) -> impl Iterator<Item = f64> {
        let first = (self.lo / self.step - 1e-6).ceil() as i64;
        let last = (self.hi / self.step + 1e-6).floor() as i64;
        let step = self.step;
        (first..=last.min(first.saturating_add(100))).map(move |k| k as f64 * step)
    }
}

/// The value axis of a chart: the ends it was given, and otherwise "nice"
/// ends around the data and its reference lines (from zero for columns,
/// bars and areas).
fn value_axis(chart: &ChartContent, data: Option<(f64, f64)>, from_zero: bool) -> Axis {
    let (mut lo, mut hi) = data.unwrap_or((0.0, 1.0));
    for l in chart.ref_lines.iter().filter(|l| l.value.is_finite()) {
        lo = lo.min(l.value);
        hi = hi.max(l.value);
    }
    if from_zero {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
    }
    let min = chart.axis_min.filter(|v| v.is_finite()).map(capped);
    let max = chart.axis_max.filter(|v| v.is_finite()).map(capped);
    let (a, b) = (min.unwrap_or(lo), max.unwrap_or(hi));
    if b <= a {
        // Ends that leave no room: the data decides.
        return Axis::of(nice_range(lo, hi, 5));
    }
    let (nlo, nhi, step) = nice_range(a, b, 5);
    Axis {
        lo: if min.is_some() { a } else { nlo },
        hi: if max.is_some() { b } else { nhi },
        step,
    }
}

/// An axis label, with as many decimals as the step between labels needs.
fn fmt_tick(v: f64, step: f64) -> String {
    let a = v.abs();
    if a >= 1e15 || (a > 0.0 && a < 1e-4 && step < 1e-6) {
        // Scientific notation, with the digits that tell labels apart.
        let decimals = a.log10().floor() - (step / 4.0).log10().floor();
        return sci(v, decimals.clamp(0.0, 15.0) as usize);
    }
    if a >= 10_000.0 {
        return fmt_value(v);
    }
    let mut decimals = 0;
    while decimals < 6 {
        let scaled = step * 10f64.powi(decimals);
        if (scaled - scaled.round()).abs() < 1e-6 {
            break;
        }
        decimals += 1;
    }
    let text = format!("{:.*}", decimals as usize, v);
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        &text
    };
    if text == "-0" { "0".into() } else { text.to_string() }
}

/// Where the error bar of value `i` of series `s` (worth `v`) ends, as
/// (low, high); `None` when it has none.
fn error_span(chart: &ChartContent, s: usize, i: usize, v: f64) -> Option<(f64, f64)> {
    let series = chart.series.get(s)?;
    let at = |list: &[Option<f64>]| list.get(i).copied().flatten().filter(|x| x.is_finite()).map(capped);
    let (low, high) = (at(&series.low), at(&series.high));
    let v = capped(v);
    if low.is_none() && high.is_none() {
        return None;
    }
    let (a, b) = match chart.error_bars.as_str() {
        "pm" => {
            let e = high.or(low)?.abs();
            (v - e, v + e)
        }
        "amounts" => (v - low.unwrap_or(0.0).abs(), v + high.unwrap_or(0.0).abs()),
        "range" => {
            let (a, b) = (low.unwrap_or(v), high.unwrap_or(v));
            (a.min(b), a.max(b))
        }
        _ => return None,
    };
    (b > a).then_some((a, b))
}

/// An error bar from `a` to `b` (positions along the value axis), `at`
/// across it, with caps `cap` long on each side.
fn error_bar(svg: &mut String, horizontal: bool, at: f64, (a, b): (f64, f64), cap: f64, f: f64) {
    let (c0, c1) = (num(at - cap), num(at + cap));
    let (at, a, b) = (num(at), num(a), num(b));
    let d = if horizontal {
        format!("M{a} {at}H{b}M{a} {c0}V{c1}M{b} {c0}V{c1}")
    } else {
        format!("M{at} {a}V{b}M{c0} {a}H{c1}M{c0} {b}H{c1}")
    };
    let _ = write!(
        svg,
        r#"<path class="cz-error" d="{d}" fill="none" stroke="currentColor" stroke-width="{}" stroke-opacity="0.9"/>"#,
        num((f * 0.07).max(1.2))
    );
}

/// Shapes for the points of lines, dot plots and scatterplots.
pub const MARKERS: &[&str] = &[
    "circle",
    "circle-open",
    "square",
    "square-open",
    "triangle",
    "triangle-open",
    "diamond",
    "diamond-open",
];

/// A point of `shape` (see `MARKERS`; anything else is a dot) centered on
/// `p`, about `r` in radius.
fn marker(svg: &mut String, shape: &str, (x, y): Point, r: f64, color: &str, extra: &str) {
    let open = shape.ends_with("-open");
    let stroke = (r * 0.4).max(1.2);
    let r = if open { r - stroke / 2.0 } else { r };
    let paint = if open {
        format!(r#"fill="none" style="stroke:{}" stroke-width="{}"{extra}"#, esc(color), num(stroke))
    } else {
        format!(r#"style="fill:{}"{extra}"#, esc(color))
    };
    let polygon = |pts: &[Point]| pts.iter().map(|(a, b)| format!("{},{}", num(*a), num(*b))).collect::<Vec<_>>().join(" ");
    let _ = match shape.trim_end_matches("-open") {
        "square" => {
            let s = r * 0.88;
            write!(
                svg,
                r#"<rect x="{}" y="{}" width="{}" height="{}" {paint}/>"#,
                num(x - s),
                num(y - s),
                num(s * 2.0),
                num(s * 2.0)
            )
        }
        "triangle" => {
            let s = r * 1.3;
            let pts = polygon(&[(x, y - s), (x + s * 0.866, y + s * 0.5), (x - s * 0.866, y + s * 0.5)]);
            write!(svg, r#"<polygon points="{pts}" {paint}/>"#)
        }
        "diamond" => {
            let s = r * 1.25;
            let pts = polygon(&[(x, y - s), (x + s, y), (x, y + s), (x - s, y)]);
            write!(svg, r#"<polygon points="{pts}" {paint}/>"#)
        }
        _ => write!(svg, r#"<circle cx="{}" cy="{}" r="{}" {paint}/>"#, num(x), num(y), num(r)),
    };
}

/// With ends of its own, the value axis cuts off what lies beyond them:
/// the plot becomes a viewport that hides what overflows it.
fn open_clip(svg: &mut String, chart: &ChartContent, plot: Rect) -> bool {
    let clip = chart.axis_min.is_some_and(f64::is_finite) || chart.axis_max.is_some_and(f64::is_finite);
    if clip {
        let (x, y, w, h) = (num(plot.x), num(plot.y), num(plot.w), num(plot.h));
        let _ = write!(
            svg,
            r#"<svg x="{x}" y="{y}" width="{w}" height="{h}" viewBox="{x} {y} {w} {h}" style="overflow:hidden">"#
        );
    }
    clip
}

fn close_clip(svg: &mut String, clip: bool) {
    if clip {
        svg.push_str("</svg>");
    }
}

/// The left and bottom edges of a plot.
fn plot_frame(svg: &mut String, plot: Rect) {
    let _ = write!(
        svg,
        r#"<path d="M{} {}V{}H{}" fill="none" stroke="currentColor" stroke-opacity="0.5"/>"#,
        num(plot.x),
        num(plot.y),
        num(plot.y + plot.h),
        num(plot.x + plot.w)
    );
}

/// Dashed lines across `plot` at values of the value axis (vertical lines
/// when the axis runs across), each with its label at the far end.
fn ref_lines(svg: &mut String, chart: &ChartContent, plot: Rect, axis: Axis, horizontal: bool, f: f64) {
    for l in chart
        .ref_lines
        .iter()
        .filter(|l| l.value.is_finite() && l.value >= axis.lo && l.value <= axis.hi)
    {
        let t = axis.t(l.value);
        let (x1, y1, x2, y2, lx, ly, anchor) = if horizontal {
            let x = plot.x + t * plot.w;
            (x, plot.y, x, plot.y + plot.h, x + f * 0.3, plot.y + f * 0.7, "start")
        } else {
            let y = plot.y + plot.h - t * plot.h;
            (plot.x, y, plot.x + plot.w, y, plot.x + plot.w - f * 0.2, y - f * 0.35, "end")
        };
        let _ = write!(
            svg,
            r#"<line class="cz-ref" x1="{}" y1="{}" x2="{}" y2="{}" stroke="currentColor" stroke-width="{}" stroke-dasharray="{} {}" stroke-opacity="0.85"/>"#,
            num(x1),
            num(y1),
            num(x2),
            num(y2),
            num((f * 0.08).max(1.2)),
            num(f * 0.4),
            num(f * 0.3)
        );
        if !l.label.trim().is_empty() {
            let _ = write!(
                svg,
                r#"<text x="{}" y="{}" text-anchor="{anchor}" font-size="{}">{}</text>"#,
                num(lx),
                num(ly),
                num(f * 0.75),
                esc(l.label.trim())
            );
        }
    }
}

/// A label in lines of at most `chars` characters, broken between words,
/// `lines` at most: what does not fit ends in "…".
fn wrap_label(label: &str, chars: usize, lines: usize) -> Vec<String> {
    let chars = chars.max(3);
    let cut = |s: &str| {
        if s.chars().count() <= chars {
            s.to_string()
        } else {
            format!("{}…", s.chars().take(chars - 1).collect::<String>().trim_end())
        }
    };
    let mut out: Vec<String> = Vec::new();
    for word in label.split_whitespace() {
        match out.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= chars => {
                line.push(' ');
                line.push_str(word);
            }
            _ => out.push(word.to_string()),
        }
    }
    if out.len() > lines.max(1) {
        let rest = out.split_off(lines.max(1) - 1).join(" ");
        out.push(rest);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out.iter().map(|l| cut(l)).collect()
}

/// How many lines the category labels take, wrapped to `chars` characters.
fn category_lines(chart: &ChartContent, chars: usize, max: usize) -> usize {
    chart.labels.iter().map(|l| wrap_label(l, chars, max).len()).max().unwrap_or(1).max(1)
}

/// A label of one or more lines at `at`: the first line there, or the lines
/// centered on it (`centered`).
fn label_text(svg: &mut String, (x, y): Point, lines: &[String], anchor: &str, centered: bool) {
    let shift = if centered { (lines.len() as f64 - 1.0) * -0.55 } else { 0.0 };
    let baseline = if centered { r#" dominant-baseline="central""# } else { "" };
    if lines.len() <= 1 {
        let text = lines.first().map(String::as_str).unwrap_or("");
        let _ = write!(
            svg,
            r#"<text x="{}" y="{}" text-anchor="{anchor}"{baseline}>{}</text>"#,
            num(x),
            num(y),
            esc(text)
        );
        return;
    }
    let _ = write!(svg, r#"<text x="{}" y="{}" text-anchor="{anchor}"{baseline}>"#, num(x), num(y));
    for (k, line) in lines.iter().enumerate() {
        let dy = if k == 0 { shift } else { 1.1 };
        let _ = write!(svg, r#"<tspan x="{}" dy="{}em">{}</tspan>"#, num(x), num(dy), esc(line));
    }
    svg.push_str("</text>");
}

/// Room for the axis titles: (left, bottom).
fn title_room(chart: &ChartContent, f: f64) -> (f64, f64) {
    let room = |t: &str| if t.trim().is_empty() { 0.0 } else { f * 1.5 };
    (room(&chart.y_title), room(&chart.x_title))
}

/// The title of the horizontal axis under the plot's labels (`below` = the
/// height of their lines after the first), and the one of the vertical axis
/// along its left edge, turned.
fn axis_titles(svg: &mut String, chart: &ChartContent, plot: Rect, below: f64, f: f64) {
    let size = num(f * 0.9);
    let x_title = chart.x_title.trim();
    if !x_title.is_empty() {
        let _ = write!(
            svg,
            r#"<text class="cz-axis-title" x="{}" y="{}" text-anchor="middle" font-size="{size}">{}</text>"#,
            num(plot.x + plot.w / 2.0),
            num(plot.y + plot.h + below + f * 2.9),
            esc(x_title)
        );
    }
    let y_title = chart.y_title.trim();
    if !y_title.is_empty() {
        let (x, y) = (num(f * 0.75), num(plot.y + plot.h / 2.0));
        let _ = write!(
            svg,
            r#"<text class="cz-axis-title" x="{x}" y="{y}" transform="rotate(-90 {x} {y})" text-anchor="middle" dominant-baseline="central" font-size="{size}">{}</text>"#,
            esc(y_title)
        );
    }
}

fn pie_chart(svg: &mut String, chart: &ChartContent, w: f64, h: f64, f: f64, steps: Option<SeriesSteps>) {
    let values: Vec<f64> = chart
        .series
        .first()
        .map(|s| (0..s.values.len()).map(|i| s.value(i).map_or(0.0, |v| capped(v).max(0.0))).collect())
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
        if (sweep - std::f64::consts::TAU).abs() < 1e-9 && hole > 0.0 {
            // A donut's only slice: a full ring, the hole cut out.
            let circle = |rr: f64| {
                format!(
                    "M{} {}A{r} {r} 0 1 1 {} {}A{r} {r} 0 1 1 {} {}Z",
                    num(cx - rr),
                    num(cy),
                    num(cx + rr),
                    num(cy),
                    num(cx - rr),
                    num(cy),
                    r = num(rr)
                )
            };
            let _ = write!(
                svg,
                r#"<path d="{}{}" fill-rule="evenodd" style="fill:{}"/>"#,
                circle(r),
                circle(hole),
                esc(&color)
            );
        } else if (sweep - std::f64::consts::TAU).abs() < 1e-9 {
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
            // Whole percents, but a slice too thin for one still shows its share.
            let text = if pct < 0.5 { fmt_value(pct) } else { num(pct.round()) };
            let _ = write!(
                svg,
                r#"<text x="{}" y="{}" text-anchor="middle" dominant-baseline="central" font-weight="600" style="fill:#fff" font-size="{}">{}%</text>"#,
                num(cx + lr * mid.cos()),
                num(cy + lr * mid.sin()),
                num(f * 0.85),
                text
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
        .flat_map(|s| s.values.iter().take(n.max(1)).flatten().copied())
        .filter(|v| v.is_finite())
        .fold(None, |r, v| Some(r.map_or((v, v), |(lo, hi): (f64, f64)| (lo.min(v), hi.max(v)))))
}

/// Every how many category labels to print so that they do not overlap.
fn label_stride(chart: &ChartContent, n: usize, band: f64, f: f64) -> usize {
    let longest = chart.labels.iter().map(|l| l.chars().count()).max().unwrap_or(1).max(1) as f64;
    let need = longest * f * 0.6 + f * 0.6;
    ((need / band.max(1.0)).ceil() as usize).clamp(1, n.max(1))
}

/// Value grid and labels along the vertical axis of `plot`.
fn y_axis(svg: &mut String, chart: &ChartContent, plot: Rect, axis: Axis, f: f64) {
    for v in axis.ticks() {
        let y = plot.y + plot.h - axis.t(v) * plot.h;
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
            fmt_tick(v, axis.step)
        );
    }
}

/// Room for the labels of a value axis.
fn y_label_room(axis: Axis, f: f64) -> f64 {
    let longest = axis.ticks().map(|v| fmt_tick(v, axis.step).chars().count()).max().unwrap_or(1) as f64;
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
    let v = |s: usize, i: usize| capped(chart.series[s].value(i).unwrap_or(0.0)).max(0.0);
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
        // Shares of the fullest column first: tiny totals cannot overflow.
        let y = |val: f64| mid - val / top * plot.h;
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
    let get = |s: usize, i: usize| chart.series[s].value(i);
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
    let xr = Axis::of(nice_range(x0, x1, 5));
    let yr = value_axis(chart, Some((y0, y1)), false);
    let (title_left, title_bottom) = title_room(chart, f);
    let left = title_left + y_label_room(yr, f);
    let plot = Rect {
        x: left,
        y: f * 0.9,
        w: (w - left - f * 1.4).max(10.0),
        h: (h - f * 2.7 - title_bottom).max(10.0),
    };
    y_axis(svg, chart, plot, yr, f);
    let px = |x: f64| plot.x + xr.t(x) * plot.w;
    let py = |y: f64| plot.y + plot.h - yr.t(y) * plot.h;
    for tick in xr.ticks() {
        let x = px(tick);
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
            fmt_tick(tick, xr.step)
        );
    }
    plot_frame(svg, plot);
    let r = (f * 0.32).max(3.0);
    let clip = open_clip(svg, chart, plot);
    for (k, &g) in groups.iter().enumerate() {
        open_step(svg, steps, k as u32);
        let color = series_color(chart, g);
        for &(_, i, x, y) in points.iter().filter(|p| p.0 == g) {
            marker(svg, &chart.series[g].marker, (px(x), py(y)), r, &color, r#" fill-opacity="0.85""#);
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
    close_clip(svg, clip);
    ref_lines(svg, chart, plot, yr, false, f);
    axis_titles(svg, chart, plot, 0.0, f);
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
    // Halved first, so that the span of extreme values stays finite.
    let t = (v / 2.0 - lo / 2.0) / (hi / 2.0 - lo / 2.0);
    0.1 + 0.9 * if t.is_nan() { 0.5 } else { t.clamp(0.0, 1.0) }
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
            let Some(v) = s.value(i) else {
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
    let value = |i: usize| values[i].min(LIMIT);
    let mut order: Vec<usize> = (0..values.len()).filter(|&i| values[i] > 0.0).collect();
    let total: f64 = order.iter().map(|&i| value(i)).sum();
    if total <= 0.0 || area.w <= 0.0 || area.h <= 0.0 {
        return out;
    }
    order.sort_by(|a, b| values[*b].total_cmp(&values[*a]));
    // Shares first: tiny totals cannot overflow.
    let size = |k: usize| value(order[k]) / total * (area.w * area.h);
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
        let values = s.plain();
        for (i, r) in squarify(&values, area).into_iter().enumerate() {
            let Some(r) = r else { continue };
            open_step(svg, steps, step);
            step += 1;
            treemap_cell(svg, r, &slice_color(i), label(i), shown(values[i]), f);
            close_step(svg, steps);
        }
        return;
    }
    // Groups: one per series, sized by their totals.
    let totals: Vec<f64> = chart.series.iter().map(|s| s.numbers().filter(|v| *v > 0.0).sum()).collect();
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
            let values = chart.series[g].plain();
            for (i, c) in squarify(&values, inner).into_iter().enumerate() {
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

/// Seats per party (row) of the first series, in whole seats.
fn parliament_seats(chart: &ChartContent) -> Vec<f64> {
    chart.series.first().map_or_else(Vec::new, |s| {
        (0..s.values.len()).map(|i| s.value(i).filter(|v| *v > 0.0).map_or(0.0, f64::round)).collect()
    })
}

/// Dots per party: one per seat, or past 2,000 seats in all, 2,000 shared
/// out in proportion (largest remainders first). A party too small for a
/// dot of its own still gets one, from the party with the most.
fn parliament_dots(seats: &[f64]) -> Vec<u32> {
    const MOST: u32 = 2000;
    let total: f64 = seats.iter().map(|v| capped(*v)).sum();
    if total <= MOST as f64 {
        return seats.iter().map(|v| *v as u32).collect();
    }
    let share: Vec<f64> = seats.iter().map(|v| capped(*v) / total * MOST as f64).collect();
    let mut dots: Vec<u32> = share.iter().map(|s| s.floor() as u32).collect();
    let mut order: Vec<usize> = (0..seats.len()).collect();
    order.sort_by(|a, b| (share[*b] - share[*b].floor()).total_cmp(&(share[*a] - share[*a].floor())).then(a.cmp(b)));
    let short = MOST.saturating_sub(dots.iter().sum());
    for &i in order.iter().take(short as usize) {
        dots[i] += 1;
    }
    let mut most: std::collections::BinaryHeap<(u32, usize)> = dots.iter().copied().zip(0..).collect();
    for i in 0..seats.len() {
        if seats[i] > 0.0 && dots[i] == 0 {
            let Some((n, j)) = most.pop().filter(|(n, _)| *n > 1) else { break };
            dots[j] -= 1;
            dots[i] = 1;
            most.push((n - 1, j));
        }
    }
    dots
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
    let dots = parliament_dots(&seats);
    let total: u32 = dots.iter().sum();
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
    for (i, &n) in dots.iter().enumerate() {
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
            fmt_value(seats.iter().map(|v| capped(*v)).sum())
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
    let get = |s: usize, i: usize| chart.series.get(s).and_then(|x| x.value(i));
    let candles: Vec<Option<[f64; 4]>> = (0..n).map(|i| Some([get(0, i)?, get(1, i)?, get(2, i)?, get(3, i)?])).collect();
    let all = candles.iter().flatten().flat_map(|c| c.iter().copied());
    let Some((lo, hi)) = all.fold(None, |r, v| Some(r.map_or((v, v), |(a, b): (f64, f64)| (a.min(v), b.max(v))))) else {
        return;
    };
    let range = value_axis(chart, Some((lo, hi)), false);
    let (title_left, title_bottom) = title_room(chart, f);
    let left = title_left + y_label_room(range, f);
    let plot = Rect {
        x: left,
        y: f * 0.9,
        w: (w - left - f * 0.6).max(10.0),
        h: (h - f * 2.7 - title_bottom).max(10.0),
    };
    y_axis(svg, chart, plot, range, f);
    let band = plot.w / n as f64;
    let center = |i: usize| plot.x + (i as f64 + 0.5) * band;
    x_categories(svg, chart, plot, n, center, band, f);
    let y = |v: f64| plot.y + plot.h - range.t(v) * plot.h;
    let clip = open_clip(svg, chart, plot);
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
    close_clip(svg, clip);
    ref_lines(svg, chart, plot, range, false, f);
    axis_titles(svg, chart, plot, 0.0, f);
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
            l.value.clamp(1.0 / LIMIT, LIMIT)
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

    /// A chart with everything the axis charts can add: error bars, ends of
    /// its own, titles, a reference line and point shapes.
    fn decorated(kind: &str) -> ChartContent {
        let mut c = ChartContent {
            chart: kind.into(),
            error_bars: "amounts".into(),
            axis_min: Some(-5.0),
            axis_max: Some(28.0),
            x_title: "Quarter".into(),
            y_title: "Sales <€>".into(),
            ref_lines: vec![crate::model::RefLine {
                value: 20.0,
                label: "Target & goal".into(),
            }],
            show_values: true,
            ..Default::default()
        };
        for (k, s) in c.series.iter_mut().enumerate() {
            s.marker = MARKERS[k * 3 % MARKERS.len()].into();
            s.low = vec![Some(1.0), None, Some(2.0), Some(0.5)];
            s.high = vec![Some(2.0), None, Some(2.0), None];
        }
        c
    }

    #[test]
    fn every_chart_type_renders_with_every_option() {
        for (kind, _) in CHART_TYPES {
            let svg = chart_svg(&decorated(kind), 800.0, 450.0, None);
            assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"), "{kind}");
            assert_eq!(svg.matches("<svg").count(), svg.matches("</svg>").count(), "{kind}");
            assert!(!svg.contains("NaN") && !svg.contains("inf"), "{kind}");
            assert!(
                !svg.contains("Sales <€>") && !svg.contains("Target & goal"),
                "{kind}: titles and labels are escaped"
            );
        }
    }

    #[test]
    fn error_bars_follow_their_mode() {
        let at = |mode: &str, low: Option<f64>, high: Option<f64>| {
            let mut c = decorated("column");
            c.error_bars = mode.into();
            c.series[0].low = vec![low];
            c.series[0].high = vec![high];
            error_span(&c, 0, 0, 10.0)
        };
        assert_eq!(at("pm", None, Some(2.0)), Some((8.0, 12.0)), "± an amount");
        assert_eq!(at("pm", Some(-3.0), None), Some((7.0, 13.0)), "± from the other column");
        assert_eq!(at("amounts", Some(1.0), Some(4.0)), Some((9.0, 14.0)), "below and above");
        assert_eq!(at("amounts", None, Some(4.0)), Some((10.0, 14.0)), "one side only");
        assert_eq!(at("range", Some(12.0), Some(7.0)), Some((7.0, 12.0)), "ends, in any order");
        assert_eq!(at("range", Some(8.0), None), Some((8.0, 10.0)));
        assert_eq!(at("", Some(1.0), Some(1.0)), None, "turned off");
        assert_eq!(at("pm", None, None), None, "no numbers");
        assert_eq!(at("pm", None, Some(0.0)), None, "nothing to draw");
        assert_eq!(at("range", Some(f64::NAN), None), None);
    }

    #[test]
    fn error_bars_are_drawn_where_they_belong() {
        let count = |kind: &str| chart_svg(&decorated(kind), 800.0, 450.0, None).matches("cz-error").count();
        // Three values of each of the two series have numbers.
        for kind in ["column", "bar", "stacked-column", "stacked-bar", "line", "dot"] {
            assert_eq!(count(kind), 6, "{kind}");
        }
        for kind in ["area", "stacked-area", "pie", "treemap", "scatter", "heatmap"] {
            assert_eq!(count(kind), 0, "{kind}");
        }
        let mut c = decorated("column");
        c.error_bars.clear();
        assert_eq!(chart_svg(&c, 800.0, 450.0, None).matches("cz-error").count(), 0, "turned off");
        // Built by series, a series' error bars come with it.
        let steps = Some(SeriesSteps { effect: "fade-in", first: 1 });
        let svg = chart_svg(&decorated("column"), 800.0, 450.0, steps);
        let second = &svg[svg.find(r#"data-fragment-index="2""#).unwrap()..];
        assert_eq!(second.matches("cz-error").count(), 3);
    }

    #[test]
    fn the_value_axis_takes_the_ends_it_is_given() {
        let mut c = ChartContent::default();
        assert_eq!(
            value_axis(&c, Some((3.0, 31.0)), true),
            Axis::of(nice_scale(3.0, 31.0, 5)),
            "from zero by default"
        );
        assert_eq!(value_axis(&c, Some((0.55, 0.92)), false), Axis::of(nice_range(0.55, 0.92, 5)));
        c.axis_min = Some(0.55);
        c.axis_max = Some(0.92);
        let axis = value_axis(&c, Some((0.59, 0.9)), true);
        assert_eq!((axis.lo, axis.hi), (0.55, 0.92));
        let ticks: Vec<String> = axis.ticks().map(|v| fmt_tick(v, axis.step)).collect();
        assert_eq!(ticks, ["0.6", "0.7", "0.8", "0.9"], "labels only where they fit");
        c.axis_max = None;
        let axis = value_axis(&c, Some((0.59, 0.9)), true);
        assert_eq!((axis.lo, axis.hi), (0.55, 0.9), "the other end from the data");
        c.axis_max = Some(0.1);
        assert_eq!(
            value_axis(&c, Some((0.0, 31.0)), true),
            Axis::of(nice_scale(0.0, 31.0, 5)),
            "ends without room are ignored"
        );
        let lines = ChartContent {
            ref_lines: vec![crate::model::RefLine {
                value: 50.0,
                label: String::new(),
            }],
            ..Default::default()
        };
        assert!(value_axis(&lines, Some((0.0, 31.0)), true).hi >= 50.0, "reference lines stay in view");
    }

    #[test]
    fn axis_labels_have_the_decimals_their_step_needs() {
        assert_eq!(fmt_tick(0.6000000000000001, 0.1), "0.6");
        assert_eq!(fmt_tick(0.55, 0.05), "0.55");
        assert_eq!(fmt_tick(7.5, 2.5), "7.5");
        assert_eq!(fmt_tick(5.0, 2.5), "5");
        assert_eq!(fmt_tick(0.005, 0.005), "0.005");
        assert_eq!(fmt_tick(-0.0000001, 0.2), "0");
        assert_eq!(fmt_tick(250_000.0, 50_000.0), "250k");
    }

    #[test]
    fn ends_of_its_own_clip_the_plot() {
        let mut c = ChartContent::default();
        assert_eq!(chart_svg(&c, 800.0, 450.0, None).matches("<svg").count(), 1);
        c.axis_max = Some(10.0);
        let svg = chart_svg(&c, 800.0, 450.0, None);
        assert_eq!(svg.matches("<svg").count(), 2, "the plot is a viewport");
        assert!(svg.contains("overflow:hidden"));
    }

    #[test]
    fn reference_lines_titles_and_shapes() {
        let mut c = decorated("dot");
        c.ref_lines.push(crate::model::RefLine {
            value: 99.0,
            label: "Out of view".into(),
        });
        let svg = chart_svg(&c, 800.0, 450.0, None);
        assert_eq!(svg.matches("cz-ref").count(), 1, "only lines inside the axis");
        assert!(svg.contains("Target &amp; goal") && !svg.contains("Out of view"));
        assert_eq!(svg.matches("cz-axis-title").count(), 2);
        assert!(svg.contains("rotate(-90"), "the vertical title is turned");
        // circle, then square-open: four points each, and one in the legend.
        assert_eq!(svg.matches("<circle").count(), 5);
        assert_eq!(svg.matches(r#"fill="none" style="stroke"#).count(), 5, "open squares are outlined");
        // Horizontal bars get a vertical reference line, labeled at the top.
        let bar = chart_svg(&decorated("bar"), 800.0, 450.0, None);
        let line = &bar[bar.find("cz-ref").unwrap()..];
        let x = |name: &str| {
            line[line.find(&format!("{name}=\"")).unwrap() + name.len() + 2..]
                .split('"')
                .next()
                .unwrap()
                .to_string()
        };
        assert_eq!(x("x1"), x("x2"));
    }

    #[test]
    fn long_labels_wrap_between_words() {
        assert_eq!(
            wrap_label("regressão com perda de decisão (r9decx)", 18, 3),
            ["regressão com", "perda de decisão", "(r9decx)"]
        );
        assert_eq!(
            wrap_label("one two three four five", 9, 2),
            ["one two", "three fo…"],
            "the rest ends in an ellipsis"
        );
        assert_eq!(wrap_label("Supercalifragilistic", 8, 2), ["Superca…"]);
        assert_eq!(wrap_label("", 8, 2), [""]);
        // Horizontal bars give long names more lines instead of cutting them off.
        let c = ChartContent {
            chart: "bar".into(),
            labels: vec!["regra com detecção perfeita, confirmação".into(), "r9decx".into()],
            series: vec![series(&[5.0, 3.6])],
            ..Default::default()
        };
        let svg = chart_svg(&c, 800.0, 450.0, None);
        assert!(svg.contains("<tspan") && svg.contains("confirmação</tspan>"), "{svg}");
    }

    #[test]
    fn a_dot_plot_keeps_its_series_apart_and_off_zero() {
        let c = ChartContent {
            chart: "dot".into(),
            labels: vec!["a".into(), "b".into()],
            series: vec![series(&[0.85, 0.71]), series(&[0.85, 0.73])],
            show_legend: false,
            ..Default::default()
        };
        let svg = chart_svg(&c, 800.0, 450.0, None);
        let xs: Vec<f64> = svg
            .match_indices("<circle cx=\"")
            .map(|(i, _)| svg[i + 12..].split('"').next().unwrap().parse().unwrap())
            .collect();
        assert_eq!(xs.len(), 4);
        assert!(xs[0] < xs[2] && xs[1] < xs[3], "the second series sits right of the first");
        assert!(!svg.contains(">0</text>"), "the axis follows the data, not zero");
    }

    /// A series of `values`; NaN stands for a missing value.
    fn series(values: &[f64]) -> crate::model::Series {
        crate::model::Series {
            values: values.iter().map(|&v| (!v.is_nan()).then_some(v)).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn missing_values_are_left_out() {
        let gap = f64::NAN;
        let mut c = ChartContent {
            chart: "dot".into(),
            labels: ["Alpha", "Beta", "Gamma", "Delta"].map(String::from).to_vec(),
            series: vec![series(&[10.0, 18.0, gap, gap]), series(&[gap, gap, 13.0, 22.0])],
            show_legend: false,
            show_values: true,
            ..Default::default()
        };
        let cxs = |svg: &str| -> Vec<f64> {
            svg.match_indices("<circle cx=\"")
                .map(|(i, _)| svg[i + 12..].split('"').next().unwrap().parse().unwrap())
                .collect()
        };
        let svg = chart_svg(&c, 800.0, 450.0, None);
        let xs = cxs(&svg);
        assert_eq!(xs.len(), 4, "one dot per value, none at zero: {svg}");
        // Alone in their categories, the dots sit in the middle of each.
        let band = (xs[3] - xs[0]) / 3.0;
        assert!((xs[1] - xs[0] - band).abs() < 1e-6 && (xs[2] - xs[1] - band).abs() < 1e-6, "{xs:?}");
        assert!(!svg.contains(">0</text>"), "no value label for missing values");

        c.chart = "line".into();
        let svg = chart_svg(&c, 800.0, 450.0, None);
        assert_eq!(cxs(&svg).len(), 4, "line points only where there are values");
        let lines = |svg: &str| -> Vec<String> {
            svg.match_indices(r#"<path d=""#)
                .map(|(i, _)| svg[i + 9..].split('"').next().unwrap().to_string())
                .collect()
        };
        assert!(lines(&svg).iter().all(|d| d.matches('M').count() == 1 && d.matches('L').count() == 1), "{svg}");
        c.series[1].values = vec![Some(5.0), None, Some(13.0), Some(22.0)];
        let svg = chart_svg(&c, 800.0, 450.0, None);
        assert_eq!(lines(&svg)[1].matches('M').count(), 2, "the line breaks at the gap");

        c.chart = "area".into();
        let svg = chart_svg(&c, 800.0, 450.0, None);
        assert_eq!(lines(&svg)[2].matches('Z').count(), 2, "areas break too: {svg}");
        c.chart = "stacked-area".into();
        let svg = chart_svg(&c, 800.0, 450.0, None);
        assert!(lines(&svg).iter().all(|d| d.matches('M').count() == 1), "stacked areas don't: {svg}");

        c.chart = "column".into();
        let svg = chart_svg(&c, 800.0, 450.0, None);
        assert_eq!(svg.matches("<rect").count(), 5, "a bar per value: {svg}");
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
    fn a_donut_with_one_slice_keeps_its_hole() {
        let one = |chart: &str| ChartContent {
            chart: chart.into(),
            labels: vec!["A".into(), "B".into(), "C".into()],
            series: vec![series(&[100.0, 0.0, 0.0])],
            show_legend: false,
            ..Default::default()
        };
        let donut = chart_svg(&one("donut"), 800.0, 450.0, None);
        assert!(!donut.contains("<circle"), "{donut}");
        assert_eq!(donut.matches("fill-rule=\"evenodd\"").count(), 1, "one ring");
        // The ring is the outer circle and the hole, drawn as two subpaths.
        let d = donut.split("<path d=\"").nth(1).and_then(|p| p.split('"').next()).unwrap_or_default();
        assert_eq!(d.matches('M').count(), 2, "{d}");
        let pie = chart_svg(&one("pie"), 800.0, 450.0, None);
        assert_eq!(pie.matches("<circle").count(), 1, "a pie's only slice is a full disc");
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
            series: vec![series(&[3.0, 0.0, 2.0])],
            ..Default::default()
        };
        let svg = chart_svg(&pie, 400.0, 400.0, steps);
        assert_eq!(svg.matches("class=\"fragment").count(), 2, "zero slices take no step");
        assert!(svg.contains(r#"data-fragment-index="4""#));
    }

    #[test]
    fn extreme_values_draw_finite_charts() {
        const EPS: f64 = f64::EPSILON;
        // Values of the series, and the ends given to the value axis.
        type Case = (&'static [&'static [f64]], Option<f64>, Option<f64>);
        let cases: &[Case] = &[
            (&[&[-1e308, 1e308]], None, None),
            (&[&[-9e307, 9e307], &[f64::MAX, -f64::MAX]], None, None),
            // Stacked, these add up past the largest number there is.
            (&[&[1e308, 1e308], &[1e308, 1e308], &[1e308, 1e308], &[1e308, 1e308]], None, None),
            (&[&[-1e308, 1e308], &[1e308, 1e308], &[-1e308, -1e308], &[1e308, -1e308]], None, None),
            (&[&[1.0, 1.0 + EPS], &[1.0, 1.0]], None, None),
            (&[&[1e300, 1e300 * (1.0 + 4.0 * EPS)]], None, None),
            (&[&[5e-324, 1e-323], &[-5e-324, 0.0]], None, None),
            (&[&[1e-310, 1e-310]], None, None),
            (&[&[1.0, 2.0], &[-1e308, 1e308]], Some(-1e308), Some(1e308)),
            (&[&[1e308, -1e308]], Some(0.0), Some(1e-300)),
            (&[&[3.0, 4.0]], Some(1.0), Some(1.0 + EPS)),
            (&[&[3.0, 4.0]], Some(1e308), None),
            (&[&[3.0, 4.0]], None, Some(-1e308)),
        ];
        for (k, (values, min, max)) in cases.iter().enumerate() {
            for (kind, _) in CHART_TYPES {
                let mut c = ChartContent {
                    chart: kind.to_string(),
                    labels: vec!["Brazil".into(), "France".into()],
                    series: values.iter().map(|v| series(v)).collect(),
                    axis_min: *min,
                    axis_max: *max,
                    error_bars: "amounts".into(),
                    ref_lines: vec![crate::model::RefLine {
                        value: -f64::MAX,
                        label: "far".into(),
                    }],
                    links: flows(&[("A", "B", values[0][0].abs()), ("B", "C", values[0][1].abs()), ("A", "C", values[0][1].abs())]),
                    show_values: true,
                    ..Default::default()
                };
                for s in &mut c.series {
                    s.low = vec![Some(f64::MAX), Some(1e-320)];
                    s.high = vec![Some(1e-320), Some(-f64::MAX)];
                }
                // Debug builds check every number written (see `num`), so
                // geometry gone wrong on the way fails here too.
                let svg = std::panic::catch_unwind(|| chart_svg(&c, 800.0, 450.0, None)).unwrap_or_else(|_| panic!("{kind}, case {k}"));
                for bad in ["NaN", "inf", "9223372036854775807"] {
                    assert!(!svg.contains(bad), "{kind}, case {k}: {svg}");
                }
            }
        }
        // Opposite extremes still make bars of equal length on both sides.
        let c = ChartContent {
            series: vec![series(&[-1e308, 1e308])],
            labels: vec!["A".into(), "B".into()],
            show_legend: false,
            ..Default::default()
        };
        let svg = chart_svg(&c, 800.0, 450.0, None);
        let heights: Vec<f64> = svg
            .match_indices(r#" height=""#)
            .skip(1)
            .map(|(i, _)| svg[i + 9..].split('"').next().unwrap().parse().unwrap())
            .collect();
        assert_eq!(heights.len(), 2, "{svg}");
        assert!(heights[0] > 100.0 && (heights[0] - heights[1]).abs() < 0.02, "{heights:?}");
    }

    #[test]
    fn values_keep_their_digits_in_labels() {
        for (v, text) in [
            (0.0, "0"),
            (-0.0, "0"),
            (1.23456, "1.23"),
            (0.5, "0.5"),
            (0.123, "0.12"),
            (0.0123, "0.012"),
            (0.001, "0.001"),
            (0.0025, "0.0025"),
            (-0.0012, "-0.0012"),
            (0.00005, "5e-5"),
            (-2.5e-7, "-2.5e-7"),
            (1234.5, "1234.5"),
            (12_500.0, "12.5k"),
            (2_250_000.0, "2.25M"),
            (-1_500_000.0, "-1.5M"),
            (1e30, "1e30"),
            (-1.234e20, "-1.23e20"),
            (f64::MAX, "1.8e308"),
        ] {
            assert_eq!(fmt_value(v), text, "{v}");
        }
        // Axis labels of huge or tiny steps still tell their values apart.
        assert_eq!(fmt_tick(1.25e15, 2.5e14), "1.25e15");
        assert_eq!(fmt_tick(2e15, 5e14), "2e15");
        assert_eq!(fmt_tick(4e-9, 2e-9), "4e-9");
        assert_eq!(fmt_tick(0.0, 2e-9), "0");
        assert_eq!(fmt_tick(1e300, 2e299), "1e300");

        // Data labels and legends show small values as they are.
        let heat = ChartContent {
            chart: "heatmap".into(),
            labels: vec!["a".into(), "b".into()],
            series: vec![series(&[0.001, 0.002]), series(&[0.0015, 0.0025])],
            show_values: true,
            ..Default::default()
        };
        let svg = chart_svg(&heat, 800.0, 450.0, None);
        // The lowest and highest are in a cell and at an end of the legend.
        for (text, count) in [(">0.001<", 2), (">0.002<", 1), (">0.0015<", 1), (">0.0025<", 2)] {
            assert_eq!(svg.matches(text).count(), count, "{text}: {svg}");
        }
        assert!(!svg.contains(">0<"), "{svg}");
        let column = ChartContent {
            series: vec![series(&[0.001, 0.0042])],
            labels: vec!["a".into(), "b".into()],
            show_values: true,
            ..Default::default()
        };
        let svg = chart_svg(&column, 800.0, 450.0, None);
        assert!(svg.contains(">0.001</text>") && svg.contains(">0.0042</text>"), "{svg}");
        let pie = ChartContent {
            chart: "pie".into(),
            series: vec![series(&[996.0, 4.0])],
            labels: vec!["a".into(), "b".into()],
            show_values: true,
            ..Default::default()
        };
        let svg = chart_svg(&pie, 800.0, 450.0, None);
        assert!(svg.contains(">100%<") && svg.contains(">0.4%<"), "a thin slice is not 0%: {svg}");
    }

    #[test]
    fn parliament_labels_keep_the_seats_given() {
        let chart = |seats: &[f64]| ChartContent {
            chart: "parliament".into(),
            labels: (0..seats.len()).map(|i| format!("P{i}")).collect(),
            series: vec![series(seats)],
            show_values: true,
            ..Default::default()
        };
        // Up to 2,000 seats, a dot per seat.
        assert_eq!(parliament_dots(&[120.0, 0.0, 1.0]), [120, 0, 1]);
        let svg = chart_svg(&chart(&[1500.0, 1500.0]), 800.0, 450.0, None);
        assert!(svg.contains(">P0 (1500)<") && svg.contains(">P1 (1500)<"), "{svg}");
        assert!(svg.contains(">3000<"), "the total as given");
        assert_eq!(svg.matches("<circle").count(), 2000, "dots stop at 2,000");
        assert_eq!(parliament_dots(&[1500.0, 1500.0]), [1000, 1000]);
        // Shared out by largest remainders, every party with seats keeps a dot.
        assert_eq!(parliament_dots(&[2950.0, 26.0, 1.0]), [1982, 17, 1]);
        let dots = parliament_dots(&[1e6, 1.0, 0.0, 1.0, 1.0]);
        assert_eq!(dots, [1997, 1, 0, 1, 1]);
        let svg = chart_svg(&chart(&[2950.0, 26.0, 1.0]), 800.0, 450.0, None);
        assert!(svg.contains(">P0 (2950)<") && svg.contains(">P1 (26)<") && svg.contains(">P2 (1)<"), "{svg}");
        assert_eq!(svg.matches("<circle").count(), 2000);
        let steps = Some(SeriesSteps { effect: "fade-in", first: 1 });
        let svg = chart_svg(&chart(&[1e6, 1.0, 1.0]), 800.0, 450.0, steps);
        assert_eq!(svg.matches("class=\"fragment").count(), 3, "one step per party, as series_steps counts");
        assert_eq!(series_steps(&chart(&[1e6, 1.0, 1.0])), 3);
    }
}
