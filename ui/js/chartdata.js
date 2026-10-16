// Charts from data in text form (CSV, or cells copied from a spreadsheet):
// reading the text, what each column holds, and every chart the data makes.

import { t, N_ } from './i18n.js';

/** What a column can be: names for the column roles. */
export const ROLES = [
  ['label', N_('Labels')],
  ['value', N_('Values')],
  ['pm', N_('± error')],
  ['minus', N_('− error')],
  ['plus', N_('+ error')],
  ['low', N_('Lower end')],
  ['high', N_('Upper end')],
  ['group', N_('Groups')],
  ['ignore', N_('Ignore')],
];

const ERRORS = ['pm', 'minus', 'plus', 'low', 'high'];

// ---------------------------------------------------------------------------
// Text to rows
// ---------------------------------------------------------------------------

/** How often `ch` separates cells in `line`, outside quotes. */
function separators(line, ch) {
  let n = 0;
  let quoted = false;
  for (const c of line) {
    if (c === '"') quoted = !quoted;
    else if (c === ch && !quoted) n++;
  }
  return n;
}

/** The cell separator of a text: tabs (spreadsheets), else semicolons or commas. */
export function delimiterOf(text) {
  const first = text.split(/\r?\n/).find((l) => l.trim()) || '';
  if (separators(first, '\t')) return '\t';
  const [semi, comma] = [separators(first, ';'), separators(first, ',')];
  return semi > comma ? ';' : ',';
}

/** LaTeX commands with one argument (`\emph{x}`) and escapes, as plain text. */
const plainLatex = (s) => s.replace(/\\[a-zA-Z]+\{([^{}]*)\}/g, '$1').replace(/\\([%&$#_])/g, '$1');

/**
 * Splits delimited text into rows of trimmed cells. "Quotes" and, as in
 * pgfplots tables, {braces} keep separators inside a cell.
 */
export function parseRows(text, delimiter = delimiterOf(text)) {
  const src = String(text).replace(/^﻿/, '');
  const rows = [];
  let row = [];
  let cell = '';
  let mode = ''; // '"' inside quotes, '{' inside braces
  let depth = 0;
  let braced = false;
  const endCell = () => {
    const v = cell.trim();
    row.push(braced ? plainLatex(v) : v);
    cell = '';
    braced = false;
  };
  const endRow = () => {
    endCell();
    if (row.some((c) => c !== '')) rows.push(row);
    row = [];
  };
  for (let i = 0; i < src.length; i++) {
    const c = src[i];
    if (mode === '"') {
      if (c === '"' && src[i + 1] === '"') {
        cell += '"';
        i++;
      } else if (c === '"') mode = '';
      else cell += c;
    } else if (mode === '{') {
      if (c === '{') depth++;
      if (c === '}' && --depth === 0) mode = '';
      else cell += c;
    } else if (c === '"' && !cell.trim()) {
      cell = '';
      mode = '"';
    } else if (c === '{' && !cell.trim()) {
      cell = '';
      mode = '{';
      depth = 1;
      braced = true;
    } else if (c === delimiter) endCell();
    else if (c === '\n') endRow();
    else if (c !== '\r') cell += c;
  }
  endRow();
  const width = Math.max(0, ...rows.map((r) => r.length));
  return rows.map((r) => [...r, ...Array(width - r.length).fill('')]);
}

const MISSING = new Set(['na', 'n/a', 'nan', 'null', 'none', '-', '–', '—', '?']);

/**
 * A cell as a number: `null` when empty or marked missing, NaN when it is
 * text. Takes decimal commas (unless commas separate the cells), thousands
 * separators, a leading currency sign and a trailing %.
 */
export function parseNumber(cell, delimiter = ',') {
  let s = String(cell ?? '').trim();
  if (!s || MISSING.has(s.toLowerCase())) return null;
  s = s
    .replace(/[\s  ']/g, '')
    .replace(/^−/, '-')
    .replace(/^([+-]?)(R\$|US\$|[$€£¥₹])/, '$1')
    .replace(/[%‰]$/, '');
  const comma = s.lastIndexOf(',');
  const dot = s.lastIndexOf('.');
  if (comma >= 0 && dot >= 0) {
    s = comma > dot ? s.replace(/\./g, '').replace(',', '.') : s.replace(/,/g, '');
  } else if (comma >= 0) {
    const parts = s.split(',');
    if (delimiter !== ',' && parts.length === 2) s = parts.join('.');
    else if (parts.slice(1).every((p) => p.length === 3)) s = parts.join('');
    else return NaN;
  } else if ((s.match(/\./g) || []).length > 1) {
    const parts = s.split('.');
    if (!parts.slice(1).every((p) => p.length === 3)) return NaN;
    s = parts.join('');
  }
  return /^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i.test(s) ? Number(s) : NaN;
}

// ---------------------------------------------------------------------------
// Rows to columns
// ---------------------------------------------------------------------------

const transposed = (rows) => (rows[0] || []).map((_, j) => rows.map((r) => r[j] ?? ''));

/**
 * Reads text as a table of columns. `header`: 'auto' (decided from the
 * cells), true or false; `transpose` swaps rows and columns first.
 */
export function readTable(text, { header = 'auto', transpose = false } = {}) {
  const delimiter = delimiterOf(text);
  let grid = parseRows(text, delimiter);
  if (transpose) grid = transposed(grid);
  if (!grid.length) return { columns: [], rows: 0, header: false, delimiter };
  const isText = (c) => c !== '' && Number.isNaN(parseNumber(c, delimiter));
  const body = grid.slice(1);
  const top = grid[0];
  const named =
    header === 'auto'
      ? body.length > 0 &&
        // Text over a column of numbers, a first row of text over numbers,
        // or the empty corner of cells copied from a spreadsheet.
        (top.some((c, j) => isText(c) && body.some((r) => r[j] !== '') && body.every((r) => !isText(r[j]))) ||
          (top.every((c) => c === '' || isText(c)) && body.some((r) => r.some((c) => c !== '' && !isText(c)))) ||
          (top[0] === '' && top.length > 1 && top.slice(1).every((c) => c !== '')))
      : !!header;
  const rows = named ? body : grid;
  const columns = top.map((head, j) => {
    const cells = rows.map((r) => r[j] ?? '');
    const parsed = cells.map((c) => parseNumber(c, delimiter));
    const filled = cells.filter((c, i) => c !== '' && parsed[i] !== null).length;
    const numbers = parsed.map((v) => (Number.isFinite(v) ? v : null));
    return {
      name: (named && head) || t('Column {n}', { n: j + 1 }),
      cells,
      numbers,
      numeric: filled > 0 && numbers.filter((v) => v !== null).length === filled,
      distinct: new Set(cells.filter((c) => c !== '')).size,
    };
  });
  return { columns, rows: rows.length, header: named, delimiter };
}

// ---------------------------------------------------------------------------
// What each column holds
// ---------------------------------------------------------------------------

const fold = (s) => s.toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '').trim();

// Names that make a column of numbers the error bars of the values before it.
const ERROR_NAMES = {
  pm: /^(±|\+\/-|\+-|pm|err|error|errors|erro|sd|std|stdev|stddev|se|sem|stderr|moe|margin|dp|ep|desvio|desviopadrao|erropadrao|desviacion|desviacionestandar|errorestandar|ecart|ecarttype)$/,
  minus: /^(-|−|minus|menos|neg|negative|errminus|err-)$/,
  plus: /^(\+|plus|mais|pos|positive|errplus|err\+)$/,
  low: /^(lo|low|lower|lwr|min|inf|inferior|lb|lcl|lci|cilow|cilower|ciinf|ll)$/,
  high: /^(hi|high|upper|upr|max|sup|superior|ub|ucl|uci|cihigh|ciupper|cisup|ul)$/,
};

/** The error role a column name gives a column after a values column named `base`. */
function errorRole(name, base) {
  const n = fold(name);
  const b = fold(base);
  const rest = b && n.startsWith(b) && n.length > b.length ? n.slice(b.length) : n;
  const token = rest.replace(/^[\s_.:([]+/, '').replace(/[\s_.:)\]]+$/, '').replace(/[\s_.]/g, '');
  return Object.keys(ERROR_NAMES).find((r) => ERROR_NAMES[r].test(token)) || '';
}

/** A column named like an index, or a first column counting the rows (0, 1, 2… or 1, 2, 3…). */
function isIndex(col, j, rows) {
  if (/^(#|n|no|nr|num|idx|index|id|row|i)$/.test(fold(col.name))) return true;
  const v = col.numbers;
  return j === 0 && rows >= 3 && (v.every((x, i) => x === i) || v.every((x, i) => x === i + 1));
}

/** A role for each column of a table, guessed from its cells and names. */
export function guessRoles(table) {
  const { columns, rows } = table;
  const roles = columns.map(() => 'ignore');
  // Labels: the text column with the most different values.
  let label = -1;
  columns.forEach((c, j) => {
    if (!c.numeric && c.distinct >= 2 && (label < 0 || c.distinct > columns[label].distinct)) label = j;
  });
  const numeric = columns.map((c, j) => j).filter((j) => columns[j].numeric);
  const indexes = numeric.filter((j) => isIndex(columns[j], j, rows));
  if (label < 0) {
    // Numbers only: the first column names the rows (years, an index…).
    if (numeric.length > 1) label = numeric[0];
  }
  if (label >= 0) roles[label] = 'label';
  // Another text column whose values repeat sorts the rows into groups.
  const group = columns.findIndex((c, j) => j !== label && !c.numeric && c.distinct >= 2 && c.distinct < rows);
  if (group >= 0) roles[group] = 'group';
  // Prices (open, high, low, close) are four series, not errors.
  const prices = ['open', 'close'].every((name) => columns.some((c) => fold(c.name) === name));
  let base = null;
  for (const j of numeric) {
    if (j === label || (indexes.includes(j) && label >= 0)) continue;
    const role = base !== null && table.header && !prices ? errorRole(columns[j].name, columns[base].name) : '';
    roles[j] = role || 'value';
    if (!role) base = j;
  }
  return roles;
}

// ---------------------------------------------------------------------------
// Columns to charts
// ---------------------------------------------------------------------------

/**
 * The labels and series that columns with `roles` give, with error bars
 * and, when a column sorts the rows into groups, one series per group.
 */
export function chartData(table, roles) {
  const { columns, rows } = table;
  const label = roles.indexOf('label');
  const labels = label >= 0 ? columns[label].cells.slice() : Array.from({ length: rows }, (_, i) => String(i + 1));
  const found = [];
  let current = null;
  roles.forEach((role, j) => {
    if (role === 'value') {
      current = { name: columns[j].name, values: columns[j].numbers, errors: {} };
      found.push(current);
    } else if (ERRORS.includes(role) && current) current.errors[role] = columns[j].numbers;
  });
  const has = (...r) => found.some((s) => r.some((x) => s.errors[x]));
  const errorBars = has('low', 'high') ? 'range' : has('minus', 'plus') ? 'amounts' : has('pm') ? 'pm' : '';
  let series = found.map(({ name, values, errors: e }) => {
    const at = (list, i) => list?.[i] ?? null;
    const ends = values.map((raw, i) => {
      const v = raw ?? 0;
      const [pm, minus, plus] = [at(e.pm, i), at(e.minus, i), at(e.plus, i)];
      const down = minus ?? pm;
      const up = plus ?? pm;
      if (errorBars === 'range') {
        return [at(e.low, i) ?? (down === null ? null : v - Math.abs(down)), at(e.high, i) ?? (up === null ? null : v + Math.abs(up))];
      }
      return [down, up];
    });
    return {
      name,
      values: values.map((v) => v ?? 0),
      color: '',
      marker: '',
      low: errorBars ? ends.map((x) => x[0]) : [],
      high: errorBars ? ends.map((x) => x[1]) : [],
    };
  });
  const g = roles.indexOf('group');
  if (g >= 0 && series.length) {
    const kinds = [...new Set(columns[g].cells.filter((c) => c !== ''))];
    const keep = (list, i, k) => (columns[g].cells[i] === k ? list[i] : null);
    series = series.flatMap((s) =>
      kinds.map((k) => ({
        ...s,
        name: series.length > 1 ? `${s.name} · ${k}` : k,
        values: s.values.map((v, i) => keep(s.values, i, k) ?? 0),
        low: s.low.length ? s.low.map((_, i) => keep(s.low, i, k)) : [],
        high: s.high.length ? s.high.map((_, i) => keep(s.high, i, k)) : [],
      })),
    );
  }
  return { labels, series, errorBars };
}

/** Links between named nodes: from the labels to the groups, as much as the first values. */
export function chartLinks(table, roles) {
  const from = roles.indexOf('label');
  const to = roles.indexOf('group');
  const value = roles.indexOf('value');
  if (from < 0 || to < 0 || table.columns[from].numeric) return [];
  return table.columns[from].cells
    .map((a, i) => ({ from: a, to: table.columns[to].cells[i], value: value >= 0 ? table.columns[value].numbers[i] ?? 1 : 1 }))
    .filter((l) => l.from && l.to && l.from !== l.to);
}

/**
 * Every chart type the data can make, in the order of `kinds` (chart.rs,
 * CHART_TYPES), as the content of a chart of that type. Maps are left to
 * the caller: whether the labels name regions is known in Rust.
 */
export function chartChoices(table, roles, kinds) {
  const { labels, series, errorBars } = chartData(table, roles);
  const links = chartLinks(table, roles);
  const n = labels.length;
  const S = series.length;
  const all = series.flatMap((s) => s.values);
  const sum = all.reduce((a, b) => a + b, 0);
  const positive = all.every((v) => v >= 0) && sum > 0;
  const base = { stacked: false, labels, series, errorBars, links: [], map: '' };
  // A scatterplot's first series holds X: numbers naming the rows, or the first values.
  const label = roles.indexOf('label');
  const xs = label >= 0 && table.columns[label].numeric ? table.columns[label].numbers.map((v) => v ?? 0) : null;
  const scatter = xs && S >= 1 ? [{ ...series[0], name: table.columns[label].name, values: xs, low: [], high: [] }, ...series] : S >= 2 ? series : null;
  const candles = S === 4 && n > 0 && labels.every((_, i) => {
    const [open, high, low, close] = series.map((s) => s.values[i]);
    return high >= Math.max(open, close, low) && low <= Math.min(open, close, high);
  });
  const fits = {
    column: S >= 1,
    'stacked-column': S >= 2,
    bar: S >= 1,
    'stacked-bar': S >= 2,
    line: S >= 1 && n >= 2,
    dot: S >= 1,
    area: S >= 1 && n >= 2,
    'stacked-area': S >= 2 && n >= 2,
    streamgraph: S >= 2 && n >= 3 && positive,
    pie: S === 1 && n >= 2 && positive,
    donut: S === 1 && n >= 2 && positive,
    treemap: positive,
    parliament: S === 1 && positive && all.every(Number.isInteger) && sum <= 1000,
    scatter: !!scatter,
    heatmap: S >= 2 && n >= 2,
    candlestick: candles,
    sankey: links.length >= 2 && links.every((l) => l.value > 0),
    network: links.length >= 2,
  };
  return kinds
    .filter((k) => fits[k])
    .map((kind) => ({
      kind,
      content: {
        ...base,
        chart: kind,
        ...(kind === 'scatter' && { series: scatter, labels: xs ? [] : labels }),
        ...((kind === 'sankey' || kind === 'network') && { links }),
      },
    }));
}

// ---------------------------------------------------------------------------
// Charts to text
// ---------------------------------------------------------------------------

const csvCell = (v) => {
  const s = v === null || v === undefined ? '' : String(v);
  return /[",\n\r]/.test(s) || s !== s.trim() ? `"${s.replace(/"/g, '""')}"` : s;
};

/** A chart's data as CSV: a column of labels, then each series (with its error bars). */
export function toCsv(c) {
  const lines = [];
  if (c.chart === 'sankey' || c.chart === 'network') {
    lines.push([t('From'), t('To'), t('Value')]);
    for (const l of c.links || []) lines.push([l.from, l.to, l.value]);
  } else {
    const mode = c.errorBars || '';
    const errs = { pm: [['high', '±']], amounts: [['low', '−'], ['high', '+']], range: [['low', 'lower'], ['high', 'upper']] }[mode] || [];
    const n = Math.max(c.labels.length, ...c.series.map((s) => s.values.length));
    lines.push(['', ...c.series.flatMap((s) => [s.name, ...errs.map(([, suffix]) => `${s.name} ${suffix}`)])]);
    for (let i = 0; i < n; i++) {
      lines.push([c.labels[i] ?? '', ...c.series.flatMap((s) => [s.values[i] ?? '', ...errs.map(([key]) => s[key]?.[i] ?? '')])]);
    }
  }
  return lines.map((r) => r.map(csvCell).join(',')).join('\n');
}
