// Exercise the real import/export modal handlers across asynchronous boundaries.
// Run: node --experimental-vm-modules tools/test_pptx_actions.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');
const root = path.join(__dirname, '..', 'ui', 'js');
const tick = () => new Promise((resolve) => setImmediate(resolve));
function deferred() { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }

async function harness(options = {}) {
  const calls = [], notices = [], closed = [], slots = [], refs = [], cleanups = [];
  let cursor = 0, refCursor = 0, rendered = false;
  const state = { doc: { docId: 'document-a', deck: { title: 'Relatório' } }, busy: null };
  const invoke = async (name, args) => {
    calls.push({ name, args });
    if (options.invoke) { const value = await options.invoke(name, args); if (value !== undefined) return value; }
    if (name === 'pptx_begin_export') return { jobId: 'job-a', deck: { slides: [{ id: 'slide-a' }] } };
    if (name === 'pptx_capture') return { data: 'data:image/png;base64,AA==' };
    return null;
  };
  const html = (strings, ...values) => ({ strings: [...strings], values });
  const stubs = {
    'lib.js': {
      html,
      useState(initial) { const i = cursor++; if (!(i in slots)) slots[i] = typeof initial === 'function' ? initial() : initial; return [slots[i], (value) => { slots[i] = typeof value === 'function' ? value(slots[i]) : value; }]; },
      useRef(initial) { const i = refCursor++; return refs[i] ||= { current: initial }; },
      useEffect(effect) { if (!rendered) cleanups.push(effect()); },
    },
    'store.js': { state, set: (patch) => Object.assign(state, patch), toast: (...args) => notices.push(args), closeModal: () => closed.push(true), flushEdits: options.flushEdits || (async () => true) },
    'api.js': { invoke, dialog: { open: options.open || (async () => '/input.pptx'), save: options.save || (async () => '/tmp/Relatório.pptx') }, opener: { revealItemInDir() {} } },
    'i18n.js': { t: (s, args = {}) => s.replace(/\{(\w+)\}/g, (_, key) => args[key]) },
    'ui.js': { Modal() {}, Button() {} },
    'diagrams.js': { ensureDiagrams: options.ensureDiagrams || (async () => {}) },
    'pptx-export.js': { buildPptx: options.buildPptx || (async () => ({ bytes: new Uint8Array([80, 75, 3, 4]), issues: [] })) },
  };
  const context = vm.createContext({ console, AbortController, TextEncoder, Uint8Array, btoa, window: { __TAURI__: { core: { invoke: async (...args) => {
    calls.push({ name: args[0], args: args.slice(1) });
    return options.binarySave ? options.binarySave(...args) : '/tmp/Relatório.pptx';
  } } } } });
  const cache = new Map();
  function load(name) {
    if (cache.has(name)) return cache.get(name);
    const stub = stubs[name];
    const module = stub ? new vm.SyntheticModule(Object.keys(stub), function () { for (const [key, value] of Object.entries(stub)) this.setExport(key, value); }, { context })
      : new vm.SourceTextModule(fs.readFileSync(path.join(root, name), 'utf8'), { context, identifier: name, importModuleDynamically: async (specifier) => {
        const child = load(path.basename(specifier));
        if (child.status === 'unlinked') await child.link((specifier) => load(path.basename(specifier)));
        if (child.status === 'linked') await child.evaluate();
        return child;
      } });
    cache.set(name, module); return module;
  }
  for (const name of ['pptx-actions.js', 'pptx-dialog.js']) { const module = load(name); await module.link((specifier) => load(path.basename(specifier))); await module.evaluate(); }
  return {
    state, calls, notices, closed, slots,
    import: () => cache.get('pptx-actions.js').namespace.importPowerPoint(),
    render() { cursor = refCursor = 0; const result = cache.get('pptx-dialog.js').namespace.PptxExport(); rendered = true; return result; },
    unmount() { for (const cleanup of cleanups) cleanup?.(); },
  };
}
function nodes(tree) {
  if (!tree || typeof tree !== 'object') return [];
  if (Array.isArray(tree)) return tree.flatMap(nodes);
  return [...(tree.strings ? [tree] : []), ...nodes(tree.values)];
}
function click(tree, label) {
  for (const node of nodes(tree)) {
    const index = node.values.findIndex((v, i) => v === label && /label\s*=\s*$/.test(node.strings[i]));
    if (index < 0) continue;
    for (let i = index; i < node.values.length; i++) if (/onClick\s*=\s*$/.test(node.strings[i])) return node.values[i]();
  }
  throw new Error(`Missing button ${label}`);
}
function output(tree) { return nodes(tree).flatMap((node) => node.values.filter((v) => typeof v === 'string')).join('\n'); }

test('cancelled import and a document switch while choosing a file never call the backend', async () => {
  const cancelled = await harness({ open: async () => null });
  await cancelled.import(); assert.equal(cancelled.calls.length, 0); assert.equal(cancelled.state.busy, null);
  const chosen = deferred(); const switched = await harness({ open: () => chosen.promise });
  const pending = switched.import(); switched.state.doc.docId = 'document-b'; chosen.resolve('/input.pptx'); await pending;
  assert.equal(switched.calls.length, 0); assert.equal(switched.state.busy, null);
});
test('failed import leaves the current document intact and clears busy state', async () => {
  const ui = await harness({ invoke: async (name) => { if (name === 'import_pptx') throw new Error('Invalid PowerPoint XML'); } });
  const doc = ui.state.doc; await ui.import();
  assert.equal(ui.state.doc, doc); assert.equal(ui.state.busy, null);
  assert.equal(ui.calls[0].args.docId, 'document-a'); assert.match(ui.notices[0][0], /Invalid PowerPoint XML/);
});
test('file chooser failure is reported and does not clear unrelated busy work', async () => {
  const ui = await harness({ open: async () => { throw new Error('File chooser unavailable'); } });
  ui.state.busy = 'Saving'; await ui.import();
  assert.equal(ui.calls.length, 0); assert.equal(ui.state.busy, 'Saving');
  assert.match(ui.notices[0][0], /File chooser unavailable/);
});
test('cancelled export save dialog creates no job and restores the modal', async () => {
  const ui = await harness({ save: async () => null }); await click(ui.render(), 'Export PowerPoint');
  assert.equal(ui.calls.length, 0); assert.equal(ui.slots[1], false);
});
test('document replacement during the export save dialog cannot export the new document', async () => {
  const chosen = deferred(); const ui = await harness({ save: () => chosen.promise });
  const pending = click(ui.render(), 'Export PowerPoint'); ui.state.doc.docId = 'document-b'; chosen.resolve('/out.pptx'); await pending;
  assert.equal(ui.calls.length, 0); assert.match(output(ui.render()), /presentation changed/);
});
test('failed edit flush stops export before snapshotting', async () => {
  const ui = await harness({ flushEdits: async () => false }); await click(ui.render(), 'Export PowerPoint');
  assert.equal(ui.calls.length, 0); assert.match(output(ui.render()), /presentation changed/);
});
test('unmount while snapshot is pending releases the returned job and does not build or save', async () => {
  const snapshot = deferred(); let built = false;
  const ui = await harness({ invoke: (name) => name === 'pptx_begin_export' ? snapshot.promise : undefined, buildPptx: async () => { built = true; } });
  const pending = click(ui.render(), 'Export PowerPoint'); await tick(); ui.unmount();
  snapshot.resolve({ jobId: 'job-late', deck: { slides: [{ id: 'slide-a' }] } }); await pending;
  assert.equal(built, false); assert(ui.calls.some((call) => call.name === 'pptx_end_export' && call.args.jobId === 'job-late'));
  assert(!ui.calls.some((call) => call.name === 'pptx_save'));
});
test('cancel during conversion releases its snapshot and never saves late output', async () => {
  const built = deferred(); const ui = await harness({ buildPptx: () => built.promise });
  const pending = click(ui.render(), 'Export PowerPoint'); await tick(); await click(ui.render(), 'Cancel');
  built.resolve({ bytes: new Uint8Array([80, 75, 3, 4]), issues: [] }); await pending;
  assert.equal(ui.closed.length, 1); assert(ui.calls.some((call) => call.name === 'pptx_end_export'));
  assert(!ui.calls.some((call) => call.name === 'pptx_save'));
});
test('save failure releases assets, reports the error and permits a retry', async () => {
  let writes = 0; const ui = await harness({ binarySave: async () => { if (++writes === 1) throw new Error('Disk is full'); return '/tmp/Relatório.pptx'; } });
  await click(ui.render(), 'Export PowerPoint'); assert.match(output(ui.render()), /Disk is full/); assert.equal(ui.slots[1], false);
  assert.equal(ui.calls.filter((call) => call.name === 'pptx_end_export').length, 1);
  await click(ui.render(), 'Export PowerPoint'); assert.match(output(ui.render()), /Saved: \/tmp\/Relatório.pptx/);
  assert.equal(ui.calls.filter((call) => call.name === 'pptx_end_export').length, 2);
  const save = ui.calls.find((call) => call.name === 'pptx_save');
  assert.equal(Buffer.from(save.args[1].headers['x-export-path'], 'base64').toString('utf8'), '/tmp/Relatório.pptx');
});
test('preview capture failure releases its snapshot without claiming a preview', async () => {
  const ui = await harness({ invoke: async (name) => { if (name === 'pptx_capture') throw new Error('Capture timed out'); } });
  await click(ui.render(), 'Preview first slide'); assert.match(output(ui.render()), /Capture timed out/);
  assert.equal(ui.slots[4], null); assert(ui.calls.some((call) => call.name === 'pptx_end_export'));
  assert(!ui.calls.some((call) => call.name === 'pptx_save'));
});
