// Target, failure, composition and history regressions using the real store.
// Run: node --experimental-vm-modules tools/test_math_edits.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');
const copy = (value) => JSON.parse(JSON.stringify(value));
const tick = () => new Promise((resolve) => setImmediate(resolve));
function deferred() { let resolve; const promise = new Promise((yes) => { resolve = yes; }); return { promise, resolve }; }

async function harness(invoke = async () => {}) {
  const target = { docId: 'math-doc', slide: 'slide', id: 'math' };
  const calls = [], notices = [], history = [], future = [];
  const backend = { docId: target.docId, revision: 1, path: '/test.cadenza', fileName: 'Test', full: true,
    themeHref: '', deckCss: '', slides: {}, outcome: {}, canUndo: false, canRedo: false,
    deck: { title: 'Test', settings: {}, width: 1280, height: 720,
      slides: [{ id: 'slide', elements: [{ id: 'math', content: { kind: 'math', latex: 'x' } }] }] },
  };
  const transport = async (name, args) => {
    calls.push({ name, args: copy(args || {}) }); await invoke(name, args);
    if (name === 'doc_apply') {
      history.push(copy(backend.deck)); future.length = 0;
      for (const update of args.op.updates || []) Object.assign(backend.deck.slides[0].elements.find((el) => el.id === update.id).content, update.patch.content);
    } else if (name === 'doc_undo') { future.push(copy(backend.deck)); backend.deck = history.pop(); }
    else if (name === 'doc_redo') { history.push(copy(backend.deck)); backend.deck = future.pop(); }
    backend.revision++; backend.canUndo = !!history.length; backend.canRedo = !!future.length;
    return copy(backend);
  };
  const context = vm.createContext({
    console: { ...console, error() {} }, queueMicrotask, clearTimeout, URL,
    setTimeout: (fn, ms) => { const timer = setTimeout(fn, ms); if (ms >= 1000) timer.unref(); return timer; },
    navigator: { platform: 'Linux' }, document: { title: '', getElementById: () => null },
  });
  const stubs = {
    'lib.js': { useLayoutEffect() {}, useReducer() {} },
    'api.js': { invoke: transport, dialog: {}, events: {}, currentWindow: () => ({ setTitle: async () => {} }), getAllWindows: async () => [], FILE_FILTER: [] },
    'i18n.js': { t: (s) => s, language: () => 'en', setLanguage: async () => {} },
  };
  const modules = new Map();
  function load(name) {
    if (modules.has(name)) return modules.get(name);
    const exports = stubs[name];
    const mod = exports
      ? new vm.SyntheticModule(Object.keys(exports), function () { for (const [key, value] of Object.entries(exports)) this.setExport(key, value); }, { context })
      : new vm.SourceTextModule(fs.readFileSync(path.join(__dirname, '..', 'ui', 'js', name), 'utf8'), { context, identifier: name });
    modules.set(name, mod); return mod;
  }
  const main = load('math-editor.js');
  await main.link((specifier) => load(path.basename(specifier))); await main.evaluate();
  const S = load('store.js').namespace;
  S.openView(copy(backend));
  return { S, calls, notices, target,
    editor: () => main.namespace.createMathEditor(target, (status) => notices.push(copy(status))),
    source: () => S.state.doc.deck.slides[0].elements[0]?.content.latex,
    remove() { backend.deck.slides[0].elements = []; backend.revision++; S.ingestView(copy(backend)); },
    switchDocument() { const next = copy(backend); next.docId = 'other'; S.openView(next); },
  };
}

test('inspection and draft synchronization do not normalize LaTeX or add operations', async () => {
  const h = await harness(), e = h.editor();
  e.sync('x'); await tick(); assert.equal(await e.flush(), true); assert.equal(h.calls.length, 0);
  await e.setDraft('  \\frac { a }{ b }  ', 'typing');
  assert.equal(h.source(), '  \\frac { a }{ b }  ');
});

test('rapid changes queue in order and Save waits for the most recent text', async () => {
  const gate = deferred(); const h = await harness(() => gate.promise), e = h.editor();
  const first = e.setDraft('x+1', 'typing'), second = e.setDraft('x+12', 'typing');
  let saved = false; const save = h.S.flushEdits().then((result) => { saved = result; });
  await tick(); assert.equal(saved, false); gate.resolve(); await Promise.all([first, second, save]);
  assert.equal(saved, true); assert.equal(h.source(), 'x+12'); assert.equal(h.notices.at(-1).draft, 'x+12');
});

test('Undo waits for a pending equation write and Redo restores it', async () => {
  const gate = deferred(); const h = await harness(() => gate.promise), e = h.editor();
  const pending = e.setDraft('x+1'); const undo = h.S.undo(); await tick();
  assert.equal(h.calls.some((call) => call.name === 'doc_undo'), false);
  gate.resolve(); await Promise.all([pending, undo]); assert.equal(h.source(), 'x');
  e.sync(h.source()); await h.S.redo(); e.sync(h.source()); assert.equal(h.source(), 'x+1');
  assert.equal(h.notices.at(-1).draft, 'x+1');
});

test('a failed write retains its draft after unmount and reopening', async () => {
  let failing = true;
  const h = await harness(async () => { if (failing) throw new Error('Rejected'); });
  const first = h.editor(); assert.equal(await first.setDraft('not lost'), false); first.dispose();
  const second = h.editor(); await tick(); assert.equal(h.notices.at(-1).draft, 'not lost');
  assert.equal(await first.setDraft('obsolete debounce'), false);
  assert.equal(await h.S.flushEdits(), false); failing = false;
  assert.equal(await h.S.flushEdits(), true); assert.equal(h.source(), 'not lost'); second.dispose();
});

test('a failed draft can be retried by Save while another element is selected', async () => {
  let failing = true;
  const h = await harness(async () => { if (failing) throw new Error('Rejected'); });
  const e = h.editor(); await e.setDraft('saved later'); e.dispose(); failing = false;
  assert.equal(await h.S.flushEdits(), true); assert.equal(h.source(), 'saved later');
});

test('reopening during an in-flight write keeps newer typing and ignores the old panel callback', async () => {
  const gate = deferred(); const h = await harness(() => gate.promise), old = h.editor();
  const first = old.setDraft('first'); await tick(); old.dispose(); const next = h.editor();
  const second = next.setDraft('latest'); assert.equal(await old.setDraft('stale'), false);
  gate.resolve(); await Promise.all([first, second]); assert.equal(h.source(), 'latest');
  assert.equal(h.notices.at(-1).draft, 'latest');
});

for (const action of ['delete', 'document switch']) {
  test(`queued equation edits cannot target a ${action}`, async () => {
    const gate = deferred(); const h = await harness((name, args) => args?.op?.op === 'blocked' ? gate.promise : undefined);
    const blocking = h.S.apply({ op: 'blocked', updates: [] }); await tick();
    const e = h.editor(), pending = e.setDraft('obsolete');
    if (action === 'delete') h.remove(); else h.switchDocument(); e.dispose(); gate.resolve();
    await blocking; assert.equal(await pending, false); assert.equal(h.calls.length, 1);
    assert.equal(await h.S.flushEdits(), true);
  });
}

test('IME composition stays local until composition ends, including an intervening Save', async () => {
  const h = await harness(), e = h.editor(); e.beginComposition();
  await e.setDraft('partial'); assert.equal(await h.S.flushEdits(), false); assert.equal(h.calls.length, 0);
  await e.endComposition('\\text{ação}', 'composition'); assert.equal(await h.S.flushEdits(), true);
  assert.equal(h.source(), '\\text{ação}'); assert.equal(h.calls.length, 1);
});
