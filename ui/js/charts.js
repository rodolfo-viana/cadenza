// Chart types in the editor: icons, the data each type starts with and how
// its data grid works. Charts are drawn in Rust (chart.rs).

import { t } from './i18n.js';

export const CHART_ICONS = {
  column: 'chart-column',
  'stacked-column': 'chart-column-stacked',
  bar: 'chart-bar',
  'stacked-bar': 'chart-bar-stacked',
  line: 'chart-line',
  area: 'chart-area',
  'stacked-area': 'chart-area',
  streamgraph: 'waves',
  pie: 'chart-pie',
  donut: 'chart-pie',
  treemap: 'layout-dashboard',
  parliament: 'landmark',
  scatter: 'chart-scatter',
  heatmap: 'grid-3x3',
  candlestick: 'chart-candlestick',
};

/** The chart's type; older files stack columns, bars and areas with a flag. */
export const chartKind = (c) => (c.stacked && ['column', 'bar', 'area'].includes(c.chart) ? `stacked-${c.chart}` : c.chart);

/** Types that show only the first series: one value per row. */
export const oneSeries = (kind) => ['pie', 'donut', 'parliament'].includes(kind);

/** Candlesticks take exactly four series: opening, high, low and closing. */
export const candleSeries = () => [t('Opening'), t('High'), t('Low'), t('Closing')];

const series = (name, values) => ({ name, values, color: '' });
const named = (n, f) => Array.from({ length: n }, (_, i) => f(i));

/** The data a new chart of `kind` starts with. */
export function chartSample(kind) {
  switch (kind) {
    case 'scatter':
      return {
        labels: named(8, (i) => `P${i + 1}`),
        series: [
          series('X', [1, 2, 3, 4, 5, 6, 7, 8]),
          series(t('Series {n}', { n: 1 }), [52, 58, 61, 67, 70, 76, 79, 85]),
          series(t('Series {n}', { n: 2 }), [45, 49, 55, 54, 60, 63, 70, 72]),
        ],
      };
    case 'heatmap':
      return {
        labels: ['Q1', 'Q2', 'Q3', 'Q4'],
        series: [series('2023', [8, 12, 15, 9]), series('2024', [11, 14, 19, 12]), series('2025', [13, 18, 24, 16]), series('2026', [17, 21, 29, 20])],
      };
    case 'streamgraph':
      return {
        labels: named(9, (i) => String(2018 + i)),
        series: [
          series(t('Series {n}', { n: 1 }), [12, 15, 18, 16, 14, 12, 10, 9, 8]),
          series(t('Series {n}', { n: 2 }), [4, 6, 9, 14, 20, 24, 26, 25, 22]),
          series(t('Series {n}', { n: 3 }), [8, 8, 7, 9, 12, 16, 22, 29, 35]),
          series(t('Series {n}', { n: 4 }), [2, 3, 5, 6, 7, 7, 8, 10, 13]),
        ],
      };
    case 'treemap':
      return { labels: named(6, (i) => t('Item {n}', { n: i + 1 })), series: [series(t('Series {n}', { n: 1 }), [42, 26, 18, 12, 8, 5])] };
    case 'parliament':
      return {
        labels: ['A', 'B', 'C', 'D', 'E'].map((n) => t('Party {n}', { n })),
        series: [series(t('Seats'), [120, 95, 60, 40, 18])],
      };
    case 'candlestick': {
      const close = [102, 104, 101, 106, 109, 107, 111, 115, 112, 117];
      const open = [100, 102, 104, 101, 106, 109, 107, 111, 115, 112];
      const names = candleSeries();
      return {
        labels: named(10, (i) => String(i + 1)),
        series: [
          series(names[0], open),
          series(names[1], close.map((c, i) => Math.max(c, open[i]) + 2)),
          series(names[2], close.map((c, i) => Math.min(c, open[i]) - 2)),
          series(names[3], close),
        ],
      };
    }
    default:
      // Also what Rust gives a new chart (model.rs, ChartContent::default).
      return { labels: ['Q1', 'Q2', 'Q3', 'Q4'], series: [series('2025', [12, 19, 15, 24]), series('2026', [16, 22, 21, 31])] };
  }
}

const sameData = (a, b) =>
  a.labels.length === b.labels.length &&
  a.labels.every((l, i) => l === b.labels[i]) &&
  a.series.length === b.series.length &&
  a.series.every((s, k) => s.name === b.series[k].name && s.values.length === b.series[k].values.length && s.values.every((v, i) => v === b.series[k].values[i]));

/**
 * What changes when a chart becomes `kind`: data nobody edited yet (a
 * type's sample) becomes the sample of the new type; edited data stays.
 */
export function switchChart(c, kind) {
  const patch = { chart: kind, stacked: false };
  if (sameData(c, chartSample(chartKind(c))) || sameData(c, chartSample('column'))) {
    Object.assign(patch, chartSample(kind));
  } else if (kind === 'candlestick' && c.series.length < 4) {
    const names = candleSeries();
    patch.series = [...c.series, ...names.slice(c.series.length).map((n) => series(n, c.labels.map(() => 0)))];
  }
  return patch;
}

/** What a build by series reveals one step at a time ('' when it can't). */
export function buildUnit(c) {
  const kind = chartKind(c);
  if (kind === 'pie' || kind === 'donut') return 'slice';
  if (kind === 'parliament') return 'party';
  if (kind === 'treemap' && c.series.length <= 1) return 'item';
  if (kind === 'heatmap') return 'column';
  if (kind === 'candlestick') return '';
  return 'series';
}

/** Build steps when building by series (mirrors chart.rs, series_steps). */
export function chartSteps(c) {
  const positive = () => (c.series[0]?.values || []).filter((v) => Number.isFinite(v) && v > 0).length;
  const n = c.series.length;
  switch (buildUnit(c)) {
    case 'slice':
    case 'party':
    case 'item':
      return positive();
    case '':
      return 0;
    default:
      return chartKind(c) === 'scatter' ? Math.max(n - 1, Math.min(n, 1)) : n;
  }
}
