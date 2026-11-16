use super::archive::{child, desc, named, num};
use super::text::{Theme, fill};
use super::{MaterializationBudget, n_};
use crate::model::{ChartContent, Series};
use anyhow::Context;
use roxmltree::Node;

/// Borrow cached point text; never copy a numeric value through a String.
fn points<'a, 'i>(node: Node<'a, 'i>, budget: &mut MaterializationBudget) -> anyhow::Result<Vec<Option<&'a str>>> {
    let count = desc(node, "ptCount")
        .map(|n| {
            n.attribute("val")
                .context(n_("Missing chart point count"))?
                .parse::<usize>()
                .context(n_("Invalid chart point count"))
        })
        .transpose()?
        .unwrap_or(0);
    let index = |n: Node<'_, '_>| -> anyhow::Result<usize> {
        let i = n
            .attribute("idx")
            .context(n_("Missing chart point index"))?
            .parse::<usize>()
            .context(n_("Invalid chart point index"))?;
        anyhow::ensure!(i < 10_000, n_("Invalid or duplicate chart point index"));
        Ok(i)
    };
    let mut len = count;
    for pt in node.descendants().filter(|n| named(*n, "pt")) {
        len = len.max(index(pt)? + 1);
    }
    anyhow::ensure!(len <= 10_000, n_("PowerPoint chart has too many points"));
    budget.array::<Option<&str>>(len)?;
    let mut values = vec![None; len];
    for pt in node.descendants().filter(|n| named(*n, "pt")) {
        let i = index(pt)?;
        anyhow::ensure!(values[i].is_none(), n_("Invalid or duplicate chart point index"));
        values[i] = Some(child(pt, "v").and_then(|n| n.text()).unwrap_or_default());
    }
    Ok(values)
}
fn labels(n: Node<'_, '_>, budget: &mut MaterializationBudget) -> anyhow::Result<Vec<String>> {
    let values = points(n, budget)?;
    budget.array::<String>(values.len())?;
    for value in &values {
        budget.charge(value.unwrap_or_default().len())?;
    }
    Ok(values.into_iter().map(|value| value.unwrap_or_default().to_owned()).collect())
}
fn numbers(n: Node<'_, '_>, budget: &mut MaterializationBudget) -> anyhow::Result<Vec<Option<f64>>> {
    let values = points(n, budget)?;
    budget.array::<Option<f64>>(values.len())?;
    Ok(values
        .into_iter()
        .map(|value| value.and_then(|s| s.parse::<f64>().ok()).filter(|v| v.is_finite()))
        .collect())
}

/// Only literal/cached values are imported. Workbook formulas and external
/// data links are never evaluated; a chart without usable caches is unsupported.
pub fn parse(root: Node<'_, '_>, theme: &Theme, budget: &mut MaterializationBudget) -> anyhow::Result<Option<ChartContent>> {
    let Some(plot) = desc(root, "plotArea") else { return Ok(None) };
    let types: Vec<_> = plot.children().filter(|n| n.is_element() && n.tag_name().name().ends_with("Chart")).collect();
    if types.len() != 1 {
        return Ok(None);
    }
    let chart = types[0];
    let grouping = child(chart, "grouping").and_then(|n| n.attribute("val")).unwrap_or("standard");
    if grouping == "percentStacked" {
        return Ok(None);
    }
    let kind = match chart.tag_name().name() {
        "barChart" => match (child(chart, "barDir").and_then(|n| n.attribute("val")).unwrap_or("col"), grouping == "stacked") {
            ("bar", true) => "stacked-bar",
            ("bar", false) => "bar",
            (_, true) => "stacked-column",
            _ => "column",
        },
        "lineChart" if grouping != "stacked" => "line",
        "areaChart" => {
            if grouping == "stacked" {
                "stacked-area"
            } else {
                "area"
            }
        }
        "pieChart" => "pie",
        "doughnutChart" => "donut",
        "scatterChart" => "scatter",
        _ => return Ok(None),
    };
    budget.array::<ChartContent>(1)?;
    let mut content = ChartContent {
        chart: kind.into(),
        labels: vec![],
        series: vec![],
        font_size: 20.,
        color: "#222222".into(),
        show_legend: desc(root, "legend").is_some(),
        show_grid: desc(plot, "majorGridlines").is_some(),
        ..ChartContent::default()
    };
    let series_count = chart.children().filter(|n| named(*n, "ser")).count();
    budget.array::<Node<'_, '_>>(series_count)?;
    budget.array::<Series>(series_count.saturating_mul(2))?;
    let series: Vec<_> = chart.children().filter(|n| named(*n, "ser")).collect();
    if series.is_empty() || series.len() > 100 || (kind == "scatter" && series.len() != 1) {
        return Ok(None);
    }
    for (index, ser) in series.into_iter().enumerate() {
        if desc(ser, "errBars").is_some() || desc(ser, "trendline").is_some() {
            return Ok(None);
        }
        let source_name = child(ser, "tx").and_then(|n| desc(n, "v")).and_then(|n| n.text());
        budget.charge(source_name.map_or(32, str::len))?;
        let name = source_name.map(str::to_owned).unwrap_or_else(|| format!("Series {}", index + 1));
        if let Some(cat) = child(ser, "cat") {
            // Some OOXML writers use multiLvlStrRef even for one flat level.
            // Only genuine hierarchies remain unsupported by native charts.
            if desc(cat, "multiLvlStrRef").is_some_and(|r| r.descendants().filter(|n| named(*n, "lvl")).count() != 1) {
                return Ok(None);
            }
            let labels = labels(cat, budget)?;
            if content.labels.is_empty() {
                content.labels = labels;
            } else if content.labels != labels {
                return Ok(None);
            }
        }
        if kind == "scatter" {
            let Some(xs) = child(ser, "xVal") else { return Ok(None) };
            content.series.push(Series {
                name: "X".into(),
                values: numbers(xs, budget)?,
                ..Series::default()
            });
        }
        let Some(val) = child(ser, if kind == "scatter" { "yVal" } else { "val" }) else {
            return Ok(None);
        };
        let values = numbers(val, budget)?;
        if values.is_empty() {
            return Ok(None);
        }
        content.series.push(Series {
            name,
            values,
            color: child(ser, "spPr").and_then(|n| fill(n, theme)).unwrap_or_default(),
            ..Series::default()
        });
    }
    if kind != "scatter" && content.labels.is_empty() {
        let len = content.series.iter().map(|s| s.values.len()).max().unwrap_or(0);
        budget.array::<String>(len)?;
        budget.charge(len.saturating_mul(5))?;
        content.labels = (1..=len).map(|i| i.to_string()).collect();
    }
    if let Some(axis) = child(plot, "valAx").and_then(|n| child(n, "scaling")) {
        content.axis_min = child(axis, "min").map(|n| num(n, "val", 0.));
        content.axis_max = child(axis, "max").map(|n| num(n, "val", 0.));
    }
    if let Some(run) = child(plot, "valAx")
        .or_else(|| child(plot, "catAx"))
        .and_then(|n| desc(n, "txPr"))
        .and_then(|n| desc(n, "defRPr"))
    {
        if let Some(ink) = fill(run, theme) {
            content.color = ink;
        }
        content.font_size = (num(run, "sz", 1500.) / 100. * 4. / 3.).clamp(4., 200.);
    }
    Ok(Some(content))
}
