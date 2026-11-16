// CSV round trips through the actual data modal and persistent source helpers.
// Run: node --experimental-vm-modules tools/test_data_source.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');

const root = path.join(__dirname, '..', 'ui', 'js');
const clone = (x) => JSON.parse(JSON.stringify(x));
const series = (name, values) => ({ name, values, low: [], high: [], color: '', marker: '' });
const translate = (s, vars = {}) => s.replace(/\{(\w+)\}/g, (_, key) => vars[key]);

async function modal(element = null, overrides = {}) {
  const applied = [], inserted = [], closed = [], notices = [];
  const slide = { id: 'slide-a', elements: element ? [element] : [] };
  const document = { width: 1280, height: 720, slides: [slide] };
  const state = { doc: { docId: 'doc-a', deck: document }, slideId: slide.id, info: { charts: ['column', 'line', 'scatter', 'sankey', 'network'].map((id) => ({ id, name: id })) } };
  let slots = [], cursor = 0;
  const html = (strings, ...values) => ({ strings: [...strings], values });
  const stubs = {
    'lib.js': {
      html, useEffect() {}, useMemo: (fn) => fn(), useRef: () => ({ current: null }),
      useState: (initial) => {
        const index = cursor++;
        if (!(index in slots)) slots[index] = typeof initial === 'function' ? initial() : initial;
        return [slots[index], (value) => { slots[index] = typeof value === 'function' ? value(slots[index]) : value; }];
      },
    },
    'store.js': {
      state, closeModal() { closed.push(true); }, apply: async (op) => {
        if (overrides.beforeApply) await overrides.beforeApply(state);
        const value = typeof op === 'function' ? op() : op;
        if (value) applied.push(clone(value));
        return overrides.failedApply ? null : value;
      },
      deck: () => document, currentSlide: () => slide, toast: (...args) => notices.push(args), openModal() {},
    },
    'ui.js': { Modal() {}, Button() {} },
    'controls.js': { Select() {}, Toggle() {}, Section() {} },
    'icons.js': { Icon() {} },
    'api.js': { invoke: async () => [], dialog: {}, ...overrides.api },
    'i18n.js': { t: translate, tn: translate, N_: (s) => s, language: () => 'en' },
    'actions.js': { insert: async (...args) => inserted.push(clone(args)), flushEdits: async () => true },
    'util.js': {
      cls: (...a) => a.filter(Boolean).join(' '), fileName: (p) => String(p).split(/[\\/]/).pop(),
      escapeHtml: (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;'),
    },
  };
  // DOM-dependent table editing runs in test_editing.html with the browser's
  // HTML parser. Do not approximate parsing or entity decoding with regexes.
  const context = vm.createContext({ console, setTimeout, clearTimeout });
  const cache = new Map();
  function load(name) {
    if (cache.has(name)) return cache.get(name);
    const stub = stubs[name];
    const module = stub
      ? new vm.SyntheticModule(Object.keys(stub), function () { for (const [key, value] of Object.entries(stub)) this.setExport(key, value); }, { context })
      : new vm.SourceTextModule(fs.readFileSync(path.join(root, name), 'utf8'), { context, identifier: name });
    cache.set(name, module);
    return module;
  }
  const main = load('datachart.js');
  await main.link((specifier) => load(path.basename(specifier)));
  await main.evaluate();
  return {
    state, slide, applied, inserted, closed, notices,
    render(props = {}) { cursor = 0; return main.namespace.ChartData({ target: element?.id, ...props }); },
    data: cache.get('chartdata.js').namespace,
  };
}

function nodes(tree, match) {
  if (!tree || typeof tree !== 'object') return [];
  if (Array.isArray(tree)) return tree.flatMap((node) => nodes(node, match));
  return [...(tree.strings && match(tree.strings.join('')) ? [tree] : []), ...nodes(tree.values, match)];
}

for (const [name, labels, values] of [
  ['ordinary data', ['A', 'B'], [series('Revenue', [10, 20]), series('Cost', [4, 8])]],
  ['single category', ['A'], [series('Revenue', [10])]],
  ['repeated category', ['A', 'A'], [series('Revenue', [10, 20])]],
  ['literal braces', ['{A}', '{B}'], [series('Revenue', [10, 20])]],
  ['independent SE series', ['A', 'B'], [series('Revenue', [10, 20]), series('SE', [4, 8])]],
]) {
  test(`Edit as CSV preserves ${name} when Column is chosen without changes`, async () => {
    const content = { kind: 'chart', chart: 'column', labels, series: values, errorBars: '' };
    const ui = await modal({ id: 'chart-a', w: 700, h: 400, content });
    const gallery = nodes(ui.render(), (source) => source.includes('class="dc-card"'));
    assert(gallery.length > 0, 'the modal offers a chart');
    await gallery[0].values.find((v) => typeof v === 'function')();
    assert.equal(ui.applied.length, 0, 'unchanged Apply creates no operation or Undo entry');
    assert.deepEqual(ui.slide.elements[0].content, content);
  });
}

// Locate an actual event property in htm's template, instead of bypassing the
// component's handler with a reconstructed serializer/importer pipeline.
function handler(tree, property, label) {
  for (const node of nodes(tree, () => true)) {
    const start = label ? node.values.findIndex((v, i) => v === label && /label\s*=\s*$/.test(node.strings[i])) : 0;
    if (start < 0) continue;
    for (let i = start; i < node.values.length; i++) {
      if (new RegExp(`\\b${property}\\s*=\\s*$`).test(node.strings[i])) return node.values[i];
    }
  }
  throw new Error(`No ${property} handler for ${label || 'input'}`);
}

function edit(ui, text, props = {}) { handler(ui.render(props), 'onInput')({ target: { value: text } }); }
async function chart(ui, kind = 'column', props = {}) {
  const cards = nodes(ui.render(props), (s) => s.includes('class="dc-card"'));
  const card = cards.find((c) => c.values.includes(kind));
  assert(card, `chart ${kind} is offered`);
  await handler(card, 'onClick')();
}

test('unchanged Apply preserves missing rows, inactive errors, colors and axis settings exactly', async () => {
  for (const errorBars of ['', 'pm', 'amounts', 'range']) {
    const content = {
      kind: 'chart', chart: 'column', labels: ['A', '', 'C'], errorBars,
      series: [{ ...series('SE', [10, null, 30]), color: '#ff0000', marker: 'square', low: [1, null, 3], high: [2, null, 4] }],
      axisMin: -99, axisMax: 9999, xTitle: 'Custom axis', refLines: [{ value: 42, label: 'Goal' }],
    };
    const ui = await modal({ id: 'chart-a', w: 700, h: 400, content });
    await chart(ui);
    assert.equal(ui.applied.length, 0);
    assert.deepEqual(ui.slide.elements[0].content, content);
  }
});

test('changing a value retains explicit repeated labels and an independent SE series', async () => {
  const content = { kind: 'chart', chart: 'column', labels: ['{A}', '{A}'], errorBars: '', series: [series('Revenue', [10, 20]), series('SE', [4, 8])] };
  const ui = await modal({ id: 'chart-a', w: 700, h: 400, content });
  edit(ui, ui.data.toCsv(content).replace('10,4', '12,4'));
  await chart(ui);
  const patch = ui.applied[0].updates[0].patch;
  assert.deepEqual(patch.content.labels, ['{A}', '{A}']);
  assert.deepEqual(patch.content.series.map((s) => [s.name, s.values]), [['Revenue', [12, 20]], ['SE', [4, 8]]]);
  assert.equal(patch.content.errorBars, '');
  assert.equal(patch.dataSource.text, ui.data.toCsv(content).replace('10,4', '12,4'));
});

test('CSV keeps literal braces, quoted spaces, CRLF and missing rows; pgfplots is explicit', async () => {
  const ui = await modal();
  const D = ui.data;
  assert.deepEqual(clone(D.parseRows('label,value\r\n{A},1\r\n" B ",2\r\n,\r\n')), [['label', 'value'], ['{A}', '1'], [' B ', '2'], ['', '']]);
  assert.deepEqual(clone(D.parseRows('label,value\n{\\emph{A, B}},1', ',', 'pgfplots')), [['label', 'value'], ['A, B', '1']]);
  const text = '\uFEFFLabel;Sales\r\n{A};1,5\r\n{B};2,5\r\n';
  edit(ui, text);
  await chart(ui);
  const props = ui.inserted[0][2];
  assert.equal(props.dataSource.text, text, 'the source is not reconstructed from parsed cells');
  assert.equal(props.dataSource.delimiter, ';');
  assert.equal(props.dataSource.header, true);
  assert.deepEqual(props.content.labels, ['{A}', '{B}']);
});

test('CSV table insertion escapes cells and stores the exact same source', async () => {
  const ui = await modal();
  const text = 'Name,Note\r\n<img src=x>,"line one\nline two"\r\n';
  edit(ui, text, { kind: 'table' });
  await handler(ui.render({ kind: 'table' }), 'onClick', 'Insert as table')();
  const [kind, , props] = ui.inserted[0];
  assert.equal(kind, 'table');
  assert.equal(props.dataSource.text, text);
  assert.deepEqual(props.content.rows, [['Name', 'Note'], ['&lt;img src=x&gt;', 'line one<br>line two']]);
});

test('current data edits preserve the original source; explicit source edits replace it atomically', async () => {
  const original = { text: 'Label;Revenue;SE\r\nA;10;4\r\nA;20;8\r\n', name: 'original.csv', header: true, transpose: false, delimiter: ';', dialect: 'csv', roles: ['label', 'value', 'value'], edited: false };
  const content = { kind: 'chart', chart: 'column', labels: ['A', 'A'], errorBars: '', series: [series('Revenue', [10, 20]), series('SE', [4, 8])] };
  const element = { id: 'chart-a', w: 700, h: 400, content, dataSource: original };
  const current = await modal(element);
  edit(current, current.data.toCsv(content).replace('10,4', '12,4'));
  await chart(current);
  assert.equal(current.applied[0].updates[0].patch.dataSource, undefined, 'the Rust operation will mark the historical source edited');
  assert.deepEqual(element.dataSource, original);

  const source = await modal(element);
  edit(source, original.text.replace('10;4', '14;4'), { source: true });
  await chart(source, 'column', { source: true });
  const patch = source.applied[0].updates[0].patch;
  assert.equal(source.applied.length, 1);
  assert.equal(patch.dataSource.text, original.text.replace('10;4', '14;4'));
  assert.deepEqual(patch.dataSource.roles, ['label', 'value', 'value']);
  assert.equal(patch.dataSource.edited, false);
  assert.deepEqual(patch.content.series.map((s) => s.values), [[14, 20], [4, 8]]);
});

test('source filename contains no Windows or Linux filesystem path', async () => {
  for (const path of ['/home/private/project/data.csv', 'C:\\Users\\Private\\data.csv']) {
    const ui = await modal(null, { api: { dialog: { open: async () => path }, invoke: async () => 'Label,Value\nA,1\nB,2' } });
    await handler(ui.render(), 'onClick', 'Open File…')();
    await chart(ui);
    assert.equal(ui.inserted[0][2].dataSource.name, 'data.csv');
  }
});

test('data modal targets its original slide and rejects deleted, changed or switched documents', async () => {
  const content = { kind: 'chart', chart: 'column', labels: ['A', 'B'], errorBars: '', series: [series('Value', [1, 2])] };
  for (const change of ['slide', 'deleted', 'edited', 'document']) {
    const ui = await modal({ id: 'chart-a', w: 700, h: 400, content: clone(content) });
    edit(ui, ',Value\nA,3\nB,2');
    if (change === 'slide') ui.state.slideId = 'other-slide';
    if (change === 'deleted') ui.slide.elements = [];
    if (change === 'edited') ui.slide.elements[0] = { ...ui.slide.elements[0], content: { ...content, axisMax: 10 } };
    if (change === 'document') ui.state.doc = { docId: 'different' };
    await chart(ui);
    assert.equal(ui.applied.length, change === 'slide' ? 1 : 0, change);
    if (change === 'slide') assert.equal(ui.applied[0].slide, 'slide-a');
    if (change === 'deleted' || change === 'edited') {
      assert.equal(ui.closed.length, 0, 'a stale target keeps the draft open');
      assert.equal(ui.notices.length, 1, 'explain why applying was skipped');
    }
  }
});

test('failed Apply keeps the draft; a late response never closes a newer modal', async () => {
  const content = { kind: 'chart', chart: 'column', labels: ['A', 'B'], errorBars: '', series: [series('Value', [1, 2])] };
  const element = { id: 'chart-a', w: 700, h: 400, content };
  const failed = await modal(element, { failedApply: true });
  edit(failed, ',Value\nA,3\nB,2');
  await chart(failed);
  assert.equal(failed.closed.length, 0);
  const delayed = await modal(element, { beforeApply: async (state) => { state.modal = { type: 'other-modal' }; } });
  edit(delayed, ',Value\nA,3\nB,2');
  await chart(delayed);
  assert.equal(delayed.closed.length, 0);
});

test('a new CSV with one repeated category keeps its label', async () => {
  const ui = await modal();
  edit(ui, 'Label,Value\nA,1\nA,2');
  await chart(ui);
  assert.deepEqual(ui.inserted[0][2].content.labels, ['A', 'A']);
});

test('editing a scatter value never turns numeric point labels into an extra X series', async () => {
  for (const labels of [[], ['2025', '2026']]) {
    const content = { kind: 'chart', chart: 'scatter', labels, errorBars: '', series: [series('X', [1, 2]), series('Y', [3, 4])] };
    const ui = await modal({ id: 'chart-a', w: 700, h: 400, content });
    edit(ui, ui.data.toCsv(content).replace('1,3', '1,30'));
    await chart(ui, 'scatter');
    const after = ui.applied[0].updates[0].patch.content;
    assert.deepEqual(after.labels, labels);
    assert.deepEqual(after.series.map((s) => [s.name, s.values]), [['X', [1, 2]], ['Y', [30, 4]]]);
  }
});
