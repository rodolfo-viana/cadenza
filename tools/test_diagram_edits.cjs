// Real diagram/store queues with controlled renderer and native transport.
// Run: node --experimental-vm-modules tools/test_diagram_edits.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');
const clone = (value) => JSON.parse(JSON.stringify(value));
const tick = () => new Promise((resolve) => setImmediate(resolve));
const deferred = () => {
  let resolve;
  const promise = new Promise((yes) => { resolve = yes; });
  return { promise, resolve };
};
const source = (label) => `flowchart LR\n  A[${label}] --> B[Finish]`;

async function harness({ render = async () => {}, invoke = async () => {}, stamp = true } = {}) {
  const original = { source: source('Original'), style: 'classic', look: 'classic' };
  let backend = {
    docId: 'doc-a', revision: 1, path: '/test.cadenza', fileName: 'Test', full: true,
    themeHref: '', deckCss: '', slides: {}, outcome: {}, canUndo: false, canRedo: false,
    deck: { title: 'Test', width: 1280, height: 720, settings: {}, slides: [{ id: 'slide-a', elements: [{
      id: 'diagram-a', x: 0, y: 0, w: 500, h: 300,
      content: { kind: 'diagram', ...original, mode: 'auto', svgLight: '<svg>old light</svg>', svgDark: '<svg>old dark</svg>', ...(stamp ? { renderedFor: clone(original) } : {}) },
    }] }] },
  };
  const history = [], future = [], calls = [], renders = [];
  const content = () => backend.deck.slides[0].elements[0]?.content;
  const current = () => S.state.doc.deck.slides[0].elements[0]?.content;
  function merge(target, patch) {
    for (const [key, value] of Object.entries(patch)) {
      if (value && typeof value === 'object' && !Array.isArray(value)) merge(target[key] ||= {}, value);
      else target[key] = clone(value);
    }
  }
  const transport = async (name, args) => {
    calls.push({ name, args: clone(args || {}) });
    await invoke(name, args);
    if (name === 'doc_apply') {
      if (args.track !== false) { history.push(clone(backend.deck)); future.length = 0; }
      for (const update of args.op.updates || []) {
        const el = backend.deck.slides.find((s) => s.id === args.op.slide)?.elements.find((e) => e.id === update.id);
        if (el) merge(el, update.patch);
      }
    } else if (name === 'doc_undo') {
      future.push(clone(backend.deck)); backend.deck = history.pop();
    } else if (name === 'doc_redo') {
      history.push(clone(backend.deck)); backend.deck = future.pop();
    } else throw new Error('Unexpected command: ' + name);
    backend.revision++; backend.canUndo = !!history.length; backend.canRedo = !!future.length;
    return clone(backend);
  };
  let theme;
  const context = vm.createContext({
    console: { ...console, warn() {}, error() {} }, queueMicrotask, clearTimeout, URL,
    setTimeout: (fn, ms) => { const timer = setTimeout(fn, ms); if (ms >= 1000) timer.unref(); return timer; },
    navigator: { platform: 'Linux' }, document: { title: '', getElementById: () => null },
    DOMParser: class { parseFromString() { return { documentElement: null }; } },
    window: { mermaid: {
      initialize: (options) => { theme = options.theme; },
      parse: async (text) => { if (text.includes('INVALID')) throw new Error('Mermaid syntax error'); },
      render: async (id, text) => {
        renders.push({ source: text, theme });
        await render(text, theme);
        return { svg: `<svg>${theme}: ${text}</svg>` };
      },
    } },
  });
  const stubs = {
    'lib.js': { useLayoutEffect() {}, useReducer() {} },
    'api.js': { invoke: transport, dialog: {}, events: {}, currentWindow: () => ({ setTitle: async () => {} }), getAllWindows: async () => [], FILE_FILTER: [] },
    'i18n.js': { t: (s) => s, N_: (s) => s, language: () => 'en', setLanguage: async () => {} },
  };
  const modules = new Map();
  const load = (name) => {
    if (modules.has(name)) return modules.get(name);
    const exports = stubs[name];
    const module = exports
      ? new vm.SyntheticModule(Object.keys(exports), function () { for (const [key, value] of Object.entries(exports)) this.setExport(key, value); }, { context })
      : new vm.SourceTextModule(fs.readFileSync(path.join(__dirname, '..', 'ui', 'js', name), 'utf8'), { context, identifier: name });
    modules.set(name, module);
    return module;
  };
  const main = load('diagram-edit.js');
  await main.link((specifier) => load(path.basename(specifier)));
  await main.evaluate();
  const S = load('store.js').namespace, D = load('diagrams.js').namespace;
  S.openView(clone(backend));
  const notifications = [];
  const editor = () => main.namespace.createDiagramEditor({ docId: 'doc-a', slide: 'slide-a', id: 'diagram-a' }, (value) => notifications.push(clone(value)));
  return { S, D, editor, calls, renders, notifications, current, original,
    external(patch) { Object.assign(content(), patch); backend.revision++; S.ingestView(clone(backend)); },
    remove() { backend.deck.slides[0].elements = []; backend.revision++; S.ingestView(clone(backend)); },
    switchDocument() { const next = clone(backend); next.docId = 'doc-b'; S.openView(next); },
  };
}

test('successful source, rendering inputs and both SVG variants are one history operation', async () => {
  const h = await harness();
  const e = h.editor();
  assert.equal(await e.update({ source: source('Changed'), style: 'forest' }), true);
  assert.equal(h.calls.length, 1);
  const patch = h.calls[0].args.op.updates[0].patch.content;
  assert.equal(patch.source, source('Changed'));
  assert.equal(patch.renderedFor.source, patch.source);
  assert.equal(patch.renderedFor.style, 'forest');
  assert.ok(patch.svgLight.startsWith('<svg>forest:'));
  assert.ok(patch.svgDark.startsWith('<svg>dark:'));
  assert.equal(h.D.diagramPreviewCurrent(h.current()), true);
  await h.S.undo();
  assert.equal(h.current().source, h.original.source);
  assert.equal(h.current().svgLight, '<svg>old light</svg>');
  await h.S.redo();
  assert.equal(h.current().source, source('Changed'));
  assert.equal(h.D.diagramPreviewCurrent(h.current()), true);
});

test('invalid source survives exactly, retains the last SVG and becomes recoverable', async () => {
  const h = await harness();
  const e = h.editor();
  const invalid = 'INVALID\n  trailing spaces  \n';
  assert.equal(await e.update({ source: invalid }), true);
  assert.equal(h.current().source, invalid);
  assert.equal(h.current().svgLight, '<svg>old light</svg>');
  assert.equal(h.current().renderedFor.source, h.original.source);
  assert.equal(h.D.diagramPreviewCurrent(h.current()), false);
  assert.match(h.notifications.at(-1).error, /syntax error/);
  await e.update({ source: source('Recovered') });
  assert.equal(h.D.diagramPreviewCurrent(h.current()), true);
  assert.equal(h.notifications.at(-1).error, null);
});

test('a later edit wins over a delayed render and includes preceding draft changes', async () => {
  const gate = deferred();
  const h = await harness({ render: () => gate.promise });
  const e = h.editor();
  const first = e.update({ source: source('Latest') });
  await tick();
  const last = e.update({ style: 'forest', look: 'handDrawn' });
  gate.resolve();
  assert.equal(await first, false);
  assert.equal(await last, true);
  assert.equal(h.calls.length, 1);
  assert.equal(h.current().source, source('Latest'));
  assert.equal(h.current().style, 'forest');
  assert.equal(h.current().look, 'handDrawn');
});

test('Undo during rendering waits, then reverts the entire completed edit once', async () => {
  const gate = deferred();
  const h = await harness({ render: () => gate.promise });
  const edit = h.editor().update({ source: source('Pending') });
  await tick();
  const undo = h.S.undo();
  await tick();
  assert.equal(h.calls.length, 0, 'Undo cannot overtake a pending diagram');
  gate.resolve();
  await Promise.all([edit, undo]);
  assert.deepEqual(h.calls.map((c) => c.name), ['doc_apply', 'doc_undo']);
  assert.equal(h.current().source, h.original.source);
  assert.equal(h.current().svgLight, '<svg>old light</svg>');
  assert.equal(h.D.diagramPreviewCurrent(h.current()), true);
});

for (const change of ['external edit', 'delete', 'document switch']) {
  test(`a delayed diagram cannot overwrite a ${change}`, async () => {
    const gate = deferred();
    const h = await harness({ render: () => gate.promise });
    const pending = h.editor().update({ source: source('Obsolete') });
    await tick();
    if (change === 'external edit') h.external({ source: source('External') });
    else if (change === 'delete') h.remove();
    else h.switchDocument();
    gate.resolve();
    assert.equal(await pending, false);
    assert.equal(h.calls.length, 0);
    if (change === 'external edit') assert.equal(h.current().source, source('External'));
    else if (change === 'delete') assert.equal(h.current(), undefined);
    else assert.equal(h.S.state.doc.docId, 'doc-b');
  });
}

test('target inputs are checked again at queued operation execution', async () => {
  const gate = deferred();
  const h = await harness({ invoke: (name, args) => args?.op?.op === 'blocked' ? gate.promise : undefined });
  const blocking = h.S.apply({ op: 'blocked' });
  await tick();
  const pending = h.editor().update({ source: source('Obsolete') });
  await tick();
  assert.equal(h.renders.length, 2, 'both SVGs finished while the operation queue was blocked');
  h.external({ source: source('External') });
  gate.resolve();
  await blocking;
  assert.equal(await pending, false);
  assert.equal(h.calls.length, 1);
  assert.equal(h.current().source, source('External'));
});

test('switching selection keeps a pending intent on its original element', async () => {
  const gate = deferred();
  const h = await harness({ render: () => gate.promise });
  const e = h.editor();
  const pending = e.update({ source: source('Finish after unmount') });
  await tick(); e.dispose(); const count = h.notifications.length;
  gate.resolve();
  assert.equal(await pending, true);
  assert.equal(h.current().source, source('Finish after unmount'));
  assert.equal(h.notifications.length, count, 'a disposed inspector is never notified');
});

test('a newly mounted editor supersedes an older editor for the same element', async () => {
  const gate = deferred();
  const h = await harness({ render: () => gate.promise });
  const first = h.editor();
  const old = first.update({ source: source('Old editor') });
  await tick(); first.dispose();
  const next = h.editor().update({ source: source('New editor') });
  gate.resolve();
  assert.equal(await old, false);
  assert.equal(await next, true);
  assert.equal(h.calls.length, 1);
  assert.equal(h.current().source, source('New editor'));
});

test('failed native writes preserve the draft and Save retries it', async () => {
  let failing = true;
  const h = await harness({ invoke: async () => { if (failing) throw new Error('Write failed'); } });
  const e = h.editor();
  assert.equal(await e.update({ source: source('Unsaved') }), false);
  assert.equal(h.current().source, h.original.source);
  assert.equal(h.notifications.at(-1).draft.source, source('Unsaved'));
  assert.match(h.notifications.at(-1).error, /Could not save/);
  assert.equal(await h.S.flushEdits(), false);
  failing = false;
  assert.equal(await h.S.flushEdits(), true);
  assert.equal(h.current().source, source('Unsaved'));
});

test('reopening a rejected diagram draft preserves it while ignoring an old panel callback', async () => {
  let failing = true;
  const h = await harness({ invoke: async () => { if (failing) throw new Error('Write failed'); } });
  const first = h.editor();
  assert.equal(await first.update({ source: source('Unsaved') }), false); first.dispose();
  const second = h.editor(); failing = false;
  assert.equal(await first.update({ source: source('Obsolete') }), false);
  assert.equal(await second.update({ style: 'forest' }), true);
  assert.equal(h.current().source, source('Unsaved')); assert.equal(h.current().style, 'forest');
});

test('Save retries a rejected diagram after its inspector has been closed', async () => {
  let failing = true;
  const h = await harness({ invoke: async () => { if (failing) throw new Error('Write failed'); } });
  const e = h.editor(); await e.update({ source: source('Unsaved') }); e.dispose();
  assert.equal(await h.S.flushEdits(), false); failing = false;
  assert.equal(await h.S.flushEdits(), true); assert.equal(h.current().source, source('Unsaved'));
});

test('legacy caches are refreshed untracked and stamped with their rendered source', async () => {
  const h = await harness({ stamp: false });
  await h.D.ensureDiagrams();
  assert.equal(h.calls.length, 1);
  assert.equal(h.calls[0].args.track, false);
  assert.equal(h.D.diagramPreviewCurrent(h.current()), true);
  assert.equal(h.S.state.doc.canUndo, false);
  await h.D.ensureDiagrams();
  assert.equal(h.calls.length, 1, 'current SVGs need no new render');
});

test('background caches cannot attach an old SVG to changed inputs', async () => {
  const gate = deferred();
  const h = await harness({ stamp: false, render: () => gate.promise });
  const pending = h.D.ensureDiagrams();
  await tick();
  h.external({ source: source('External') });
  gate.resolve(); await pending;
  assert.equal(h.calls.length, 0);
  assert.equal(h.current().source, source('External'));
  assert.equal(h.D.diagramPreviewCurrent(h.current()), false);
  await h.D.ensureDiagrams();
  assert.equal(h.D.diagramPreviewCurrent(h.current()), true);
});
