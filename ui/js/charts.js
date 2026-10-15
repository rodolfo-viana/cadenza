// Chart types in the editor: icons, the data each type starts with and how
// its data grid works. Charts are drawn in Rust (chart.rs).

import { t, N_ } from './i18n.js';

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
  sankey: 'split',
  network: 'chart-network',
  map: 'earth',
};

/** Maps besides countries: the world and the continents (maps.rs). */
export const MAP_AREAS = [
  ['world', N_('World')],
  ['africa', N_('Africa')],
  ['asia', N_('Asia')],
  ['europe', N_('Europe')],
  ['north-america', N_('North America')],
  ['oceania', N_('Oceania')],
  ['south-america', N_('South America')],
];

/** Types drawn from links between named nodes instead of the table. */
export const linkChart = (kind) => kind === 'sankey' || kind === 'network';

/** The chart's type; older files stack columns, bars and areas with a flag. */
export const chartKind = (c) => (c.stacked && ['column', 'bar', 'area'].includes(c.chart) ? `stacked-${c.chart}` : c.chart);

/** Types that show only the first series: one value per row. */
export const oneSeries = (kind) => ['pie', 'donut', 'parliament', 'map'].includes(kind);

/** Candlesticks take exactly four series: opening, high, low and closing. */
export const candleSeries = () => [t('Opening'), t('High'), t('Low'), t('Closing')];

const series = (name, values) => ({ name, values, color: '' });
const link = (from, to, value) => ({ from, to, value });
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
    case 'sankey': {
      const [s1, s2, s3] = [1, 2, 3].map((n) => t('Source {n}', { n }));
      const [m1, m2] = [1, 2].map((n) => t('Stage {n}', { n }));
      const [t1, t2, t3] = [1, 2, 3].map((n) => t('Target {n}', { n }));
      return {
        links: [
          link(s1, m1, 30), link(s2, m1, 20), link(s2, m2, 15), link(s3, m2, 25),
          link(m1, t1, 35), link(m1, t2, 15), link(m2, t2, 30), link(m2, t3, 10),
        ],
      };
    }
    case 'map': {
      // Population in millions, about 2024.
      const people = [
        ['China', 1410], ['India', 1440], ['United States', 340], ['Indonesia', 280], ['Pakistan', 245], ['Nigeria', 230],
        ['Brazil', 212], ['Bangladesh', 174], ['Russia', 144], ['Ethiopia', 129], ['Mexico', 130], ['Japan', 124],
        ['Philippines', 116], ['Egypt', 116], ['Turkey', 86], ['Germany', 84], ['United Kingdom', 69], ['France', 68],
        ['South Africa', 63], ['Kenya', 56], ['Colombia', 53], ['Spain', 48], ['Argentina', 46], ['Canada', 41], ['Australia', 27],
      ];
      return { map: 'world', labels: people.map((p) => p[0]), series: [series(t('Series {n}', { n: 1 }), people.map((p) => p[1]))] };
    }
    case 'network':
      return {
        links: [
          ['A', 'B', 3], ['A', 'C', 2], ['A', 'D', 1], ['B', 'C', 2], ['B', 'E', 1], ['C', 'F', 2],
          ['D', 'E', 1], ['E', 'F', 3], ['F', 'G', 2], ['G', 'H', 1], ['E', 'H', 2], ['D', 'G', 1],
        ].map(([a, b, v]) => link(a, b, v)),
      };
    default:
      // Also what Rust gives a new chart (model.rs, ChartContent::default).
      return { labels: ['Q1', 'Q2', 'Q3', 'Q4'], series: [series('2025', [12, 19, 15, 24]), series('2026', [16, 22, 21, 31])] };
  }
}

/** Whether a chart still has the sample data of `kind`. */
export const isSample = (c, kind) => sameData(c, chartSample(kind));

const sameLinks = (a = [], b = []) => a.length === b.length && a.every((l, i) => l.from === b[i].from && l.to === b[i].to && l.value === b[i].value);

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
  if (linkChart(kind)) {
    // The table stays as it is; links nobody wrote yet become the sample.
    const samples = [chartSample('sankey').links, chartSample('network').links];
    if (!c.links?.length || samples.some((s) => sameLinks(c.links, s))) patch.links = chartSample(kind).links;
    return patch;
  }
  if (sameData(c, chartSample(chartKind(c))) || sameData(c, chartSample('column')) || sameData(c, chartSample('map'))) {
    Object.assign(patch, chartSample(kind));
  } else if (kind === 'map' && !c.map) {
    patch.map = 'world';
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
  if (kind === 'candlestick' || kind === 'map') return '';
  if (kind === 'sankey') return 'column';
  if (kind === 'network') return 'item';
  return 'series';
}

/**
 * Nodes and links of a Sankey diagram or network, as chart.rs (graph)
 * reads them: names in order of appearance, links between two nodes.
 */
export function graph(c, flows) {
  const names = [];
  const links = [];
  const node = (name) => (names.includes(name) ? names.indexOf(name) : names.push(name) - 1);
  for (const l of c.links || []) {
    const [a, b] = [String(l.from ?? '').trim(), String(l.to ?? '').trim()];
    const ok = Number.isFinite(l.value) && l.value > 0;
    if ((!ok && flows) || !a || !b || a === b) continue;
    links.push([node(a), node(b), ok ? l.value : 1]);
  }
  return { names, links };
}

/** Columns of a Sankey diagram's nodes (mirrors chart.rs, sankey_columns). */
export function sankeyColumns(g) {
  const n = g.names.length;
  const col = new Array(n).fill(0);
  for (let round = 0; round < n; round++) {
    let changed = false;
    for (const [s, t] of g.links) {
      if (col[t] < col[s] + 1 && col[s] + 1 < n) {
        col[t] = col[s] + 1;
        changed = true;
      }
    }
    if (!changed) break;
  }
  const last = Math.max(0, ...col);
  return col.map((c, i) => (g.links.some((l) => l[0] === i) ? c : last));
}

/** Build steps when building by series (mirrors chart.rs, series_steps). */
export function chartSteps(c) {
  const positive = () => (c.series[0]?.values || []).filter((v) => Number.isFinite(v) && v > 0).length;
  const n = c.series.length;
  switch (buildUnit(c)) {
    case 'slice':
    case 'party':
      return positive();
    case 'item':
      return chartKind(c) === 'network' ? Math.min(300, graph(c, false).names.length) : positive();
    case '':
      return 0;
    case 'column':
      return chartKind(c) === 'sankey' ? Math.max(0, ...sankeyColumns(graph(c, true))) : n;
    default:
      return chartKind(c) === 'scatter' ? Math.max(n - 1, Math.min(n, 1)) : n;
  }
}
