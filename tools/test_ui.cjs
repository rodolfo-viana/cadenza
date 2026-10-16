// Behavioral tests for document closing, editing and portable clipboard operations.
// Uses real UI modules with DOM/Tauri boundaries stubbed; no npm dependencies.
// Run: node --experimental-vm-modules tools/test_ui.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');

const root = path.join(__dirname, '..', 'ui', 'js');
const clone = (value) => JSON.parse(JSON.stringify(value));
const tick = () => new Promise((resolve) => setImmediate(resolve));
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

// Copied media stays in the backend; the clipboard names its snapshot.
const SNAPSHOT = '0123456789abcdef0123456789abcdef';

function view(id = 'document-a', revision = 1) {
  return {
    docId: id, revision, path: '/saved.cadenza', fileName: 'Saved', dirty: false, full: true,
    themeHref: '', deckCss: '', slides: {}, outcome: {},
    deck: {
      title: 'Saved', width: 1280, height: 720, settings: {},
      slides: [{ id: 'slide-1', elements: [{ id: 'table-1', content: { kind: 'table', rows: [['Original']] } }] }],
    },
  };
}

async function ui(invoke, clipboard = {}, { importBytes = () => {}, hooks = {}, quiet = false } = {}) {
  const context = vm.createContext({
    console: quiet ? { ...console, error() {} } : console, queueMicrotask, clearTimeout,
    setTimeout: (fn, ms) => { const timer = setTimeout(fn, ms); if (ms >= 1000) timer.unref(); return timer; },
    navigator: { platform: 'Linux' },
    CustomEvent: class { constructor(type) { this.type = type; } },
    document: { title: 'Cadenza', getElementById: () => null, addEventListener() {}, removeEventListener() {}, createRange: () => ({ selectNodeContents() {}, collapse() {} }) },
    window: { getSelection: () => ({ rangeCount: 0, removeAllRanges() {}, addRange() {} }), dispatchEvent() {} },
  });
  const stubs = {
    'lib.js': { html() {}, useEffect() {}, useReducer() {}, useRef() {}, useState() {}, ...hooks },
    'api.js': {
      invoke, clipboard, importBytes, dialog: {}, opener: {}, events: { listen() {} },
      FILE_FILTER: [], IMAGE_EXTS: [], VIDEO_EXTS: [], currentWindow: () => ({ setTitle: async () => {} }),
    },
    'i18n.js': { t: (s) => s, N_: (s) => s, setLanguage: async () => {}, language: () => 'en' },
    'controls.js': { parseColor: () => null, toCss: (v) => v },
    'icons.js': { Icon() {} },
  };
  const cache = new Map();
  function module(name) {
    if (cache.has(name)) return cache.get(name);
    const exports = stubs[name];
    const mod = exports
      ? new vm.SyntheticModule(Object.keys(exports), function () {
          for (const [key, value] of Object.entries(exports)) this.setExport(key, value);
        }, { context, identifier: name })
      : new vm.SourceTextModule(fs.readFileSync(path.join(root, name), 'utf8'), { context, identifier: name });
    cache.set(name, mod);
    return mod;
  }
  for (const name of ['actions.js', 'textedit.js', 'notes.js']) {
    const mod = module(name);
    await mod.link((specifier) => module(path.basename(specifier)));
    await mod.evaluate();
  }
  return {
    S: module('store.js').namespace, A: module('actions.js').namespace,
    T: module('textedit.js').namespace, Notes: module('notes.js').namespace,
    Charts: module('charts.js').namespace,
  };
}

function editTable(S, T) {
  const cell = { innerHTML: 'Original', querySelector: () => null, setAttribute() {}, removeAttribute() {}, focus() {} };
  const table = {
    querySelectorAll: (selector) => selector === 'tr' ? [{ children: [cell] }] : [cell],
    addEventListener() {}, removeEventListener() {},
  };
  const slideView = { node: () => ({ querySelector: () => table }) };
  T.beginTableEdit(slideView, S.currentSlide().elements[0]);
  cell.innerHTML = 'Unsaved cell';
}

function editText(S, T) {
  const node = {
    innerHTML: '<p>Original</p>', classList: { contains: () => false }, querySelector: () => null,
    setAttribute() {}, removeAttribute() {}, focus() {}, contains: () => false,
    addEventListener() {}, removeEventListener() {},
  };
  const slideView = { node: () => ({ querySelector: () => node }) };
  T.beginTextEdit(slideView, S.currentSlide().elements[0]);
  return node;
}

for (const choice of ['cancel', 'save', 'discard']) {
  test(`closing with an uncommitted table edit honors ${choice}`, async () => {
    const backend = view();
    const calls = [];
    const { S, T } = await ui(async (name, args) => {
      calls.push(name);
      if (name === 'doc_apply') {
        assert.equal(args.docId, backend.docId);
        backend.deck.slides[0].elements[0].content.rows = clone(args.op.updates[0].patch.content.rows);
        backend.dirty = true;
        backend.revision++;
        return clone(backend);
      }
      if (name === 'doc_save') { backend.dirty = false; backend.revision++; return clone(backend); }
      if (name === 'recent_list') return [];
    });
    S.openView(clone(backend));
    editTable(S, T);
    assert.equal(S.state.doc.dirty, false);
    const closing = S.closeDocument();
    await tick();
    assert.equal(S.state.modal?.type, 'unsaved');
    assert.equal(backend.deck.slides[0].elements[0].content.rows[0][0], 'Unsaved cell');
    assert(!calls.includes('doc_close'));
    S.state.modal.props.resolve(choice);
    await closing;
    assert.equal(calls.includes('doc_close'), choice !== 'cancel');
    assert.equal(calls.includes('doc_save'), choice === 'save');
    assert.equal(S.state.doc === null, choice !== 'cancel');
    if (choice === 'save') assert.equal(backend.dirty, false);
    await T.endAllEditing();
  });
}

test('opening a file commits a pending table edit before replacing an untitled presentation', async () => {
  const backend = { ...view(), path: null };
  const calls = [];
  const { S, T } = await ui(async (name, args) => {
    calls.push(name);
    if (name === 'doc_apply') {
      backend.deck.slides[0].elements[0].content.rows = clone(args.op.updates[0].patch.content.rows);
      backend.dirty = true;
      backend.revision++;
      return clone(backend);
    }
    if (name === 'window_focus_document') return false;
  });
  S.openView(clone(backend));
  editTable(S, T);
  await S.openPath('/other.cadenza');
  assert.equal(backend.deck.slides[0].elements[0].content.rows[0][0], 'Unsaved cell');
  assert(calls.includes('window_new'));
  assert(!calls.includes('doc_open'));
  assert.equal(S.state.doc.docId, 'document-a');
  await T.endAllEditing();
});

test('queued edits and late responses cannot replace a newly opened document', async () => {
  const response = deferred();
  const calls = [];
  const { S } = await ui(async (name, args) => { calls.push({ name, args }); return response.promise; });
  S.openView(view());
  const first = S.apply({ op: 'updateDeck', patch: { title: 'First' } });
  const second = S.apply({ op: 'updateDeck', patch: { title: 'Second' } });
  await tick();
  assert.equal(calls.length, 1);
  S.openView(view('document-b'));
  response.resolve(view());
  await Promise.all([first, second]);
  assert.equal(S.state.doc.docId, 'document-b');
  assert.equal(calls.length, 1);
});

for (const failWrite of [false, true]) {
  test(`cut waits for a complete native clipboard write${failWrite ? ' and preserves content on failure' : ''}`, async () => {
    const nativeWrite = deferred();
    const calls = [];
    const { S, A } = await ui(async (name, args) => {
      calls.push({ name, args });
      if (name === 'clipboard_copy') {
        await nativeWrite.promise;
        return { ...clone(args.selection), media: SNAPSHOT, webFonts: [{ family: 'TestFont', css: '' }] };
      }
      if (name === 'doc_apply') return view('document-a', 2);
    });
    S.openView(view());
    S.select(['table-1']);
    let prevented = false;
    const copying = A.onCopy({ preventDefault: () => { prevented = true; } }, true);
    assert(prevented);
    await tick();
    assert(!calls.some((c) => c.name === 'doc_apply'));
    if (failWrite) nativeWrite.reject(new Error('Clipboard unavailable'));
    else nativeWrite.resolve();
    await copying;
    assert.equal(calls.some((c) => c.name === 'doc_apply'), !failWrite);
    if (!failWrite) {
      const payload = S.readClip(null);
      assert.equal(payload.media, SNAPSHOT);
      assert.equal(payload.webFonts[0].family, 'TestFont');
    }
  });
}

test('cross-window paste transfers the full snapshot through the document queue', async () => {
  const calls = [];
  const payload = { type: 'elements', elements: [], media: SNAPSHOT, webFonts: [] };
  const { S, A } = await ui(async (name, args) => { calls.push({ name, args }); return view('document-b'); }, {
    readText: async () => 'application/x-cadenza+json:' + JSON.stringify(payload),
  });
  S.openView(view('document-b'));
  await A.pasteFromMenu();
  assert.equal(calls[0].name, 'clipboard_paste');
  assert.equal(calls[0].args.docId, 'document-b');
  assert.equal(calls[0].args.target, 'slide-1');
  assert.deepEqual(clone(calls[0].args.clip), payload);
});

test('a delayed clipboard read cannot paste into a replacement document', async () => {
  const read = deferred();
  const calls = [];
  const { S, A } = await ui(async (name) => { calls.push(name); }, { readText: () => read.promise });
  S.openView(view());
  const pasting = A.pasteFromMenu();
  await tick();
  S.openView(view('document-b'));
  read.resolve('application/x-cadenza+json:' + JSON.stringify({ type: 'elements', elements: [] }));
  await pasting;
  assert.equal(calls.length, 0);
});

for (const action of ['close', 'save', 'endEditing']) {
  test(`a failed table edit prevents ${action} and is retried without losing the cell`, async () => {
    const backend = view();
    const calls = [];
    let rejectEdit = true;
    const { S, T } = await ui(async (name, args) => {
      calls.push(name);
      if (name === 'doc_apply') {
        if (rejectEdit) throw new Error('Temporary edit failure');
        backend.deck.slides[0].elements[0].content.rows = clone(args.op.updates[0].patch.content.rows);
        backend.dirty = true;
        backend.revision++;
        return clone(backend);
      }
      if (name === 'doc_save') { backend.dirty = false; backend.revision++; return clone(backend); }
      if (name === 'recent_list') return [];
    }, {}, { quiet: true });
    S.openView(clone(backend));
    editTable(S, T);
    const perform = () => action === 'close' ? S.closeDocument() : action === 'save' ? S.save() : T.endAllEditing();
    await perform();
    assert(!calls.includes('doc_close') && !calls.includes('doc_save'));
    assert.equal(S.state.doc.docId, 'document-a');
    assert(T.isEditingTable());
    assert.equal(S.state.modal, null);
    // A second attempt must still send the unaccepted rows.
    await perform();
    assert.equal(calls.filter((n) => n === 'doc_apply').length, 2);
    rejectEdit = false;
    assert.equal(await S.save(), true);
    assert.equal(backend.deck.slides[0].elements[0].content.rows[0][0], 'Unsaved cell');
    assert.equal(backend.dirty, false);
    await T.endAllEditing();
  });
}

test('failed text edits remain editable until a successful retry saves them', async () => {
  const backend = view();
  backend.deck.slides[0].elements[0].content = { kind: 'text', html: '<p>Original</p>' };
  const calls = [];
  let rejectEdit = true;
  const { S, T } = await ui(async (name, args) => {
    calls.push(name);
    if (name === 'doc_apply') {
      if (rejectEdit) throw new Error('Temporary text failure');
      backend.deck.slides[0].elements[0].content.html = args.op.updates[0].patch.content.html;
      backend.revision++;
      backend.dirty = true;
      return clone(backend);
    }
    if (name === 'doc_save') { backend.dirty = false; backend.revision++; return clone(backend); }
    if (name === 'recent_list') return [];
  }, {}, { quiet: true });
  S.openView(clone(backend));
  const node = editText(S, T);
  node.innerHTML = '<p>Unsaved text</p>';
  assert.equal(await T.endAllEditing(), false);
  assert.equal(await S.save(), false);
  assert.equal(T.editingNode(), node);
  assert(!calls.includes('doc_save'));
  rejectEdit = false;
  assert.equal(await S.save(), true);
  assert.equal(backend.deck.slides[0].elements[0].content.html, '<p>Unsaved text</p>');
  assert.equal(await T.endAllEditing(), true);
  assert.equal(T.isEditing(), false);
});

test('saving waits for text typed while a previous edit is in flight', async () => {
  const backend = view();
  backend.deck.slides[0].elements[0].content = { kind: 'text', html: '<p>Original</p>' };
  const response = deferred(), latest = deferred();
  const calls = [];
  const { S, T } = await ui(async (name, args) => {
    calls.push(name);
    if (name === 'doc_apply') {
      await (calls.filter((n) => n === name).length === 1 ? response.promise : latest.promise);
      backend.deck.slides[0].elements[0].content.html = args.op.updates[0].patch.content.html;
      backend.revision++;
      return clone(backend);
    }
    if (name === 'doc_save') { backend.revision++; return clone(backend); }
    if (name === 'recent_list') return [];
  });
  S.openView(clone(backend));
  const node = editText(S, T);
  node.innerHTML = '<p>First edit</p>';
  const editing = S.flushEdits();
  await tick();
  node.innerHTML = '<p>Original</p>';
  const saving = S.save();
  await tick();
  assert(!calls.includes('doc_save'));
  response.resolve();
  await tick();
  assert.equal(calls.filter((n) => n === 'doc_apply').length, 2);
  assert(!calls.includes('doc_save'));
  latest.resolve();
  await Promise.all([editing, saving]);
  assert.equal(backend.deck.slides[0].elements[0].content.html, '<p>Original</p>');
  assert.equal(calls.at(-1), 'recent_list');
  assert.equal(calls.at(-2), 'doc_save');
  await T.endAllEditing();
});

test('notes remain pending after a rejected edit and are included in a retry', async () => {
  const backend = view();
  let rejectEdit = true;
  const { S, Notes } = await ui(async (name, args) => {
    if (name === 'doc_apply') {
      if (rejectEdit) throw new Error('Temporary notes failure');
      backend.deck.slides[0].notes = args.op.patch.notes;
      backend.dirty = true;
      backend.revision++;
      return clone(backend);
    }
  }, {}, {
    quiet: true,
    hooks: {
      html: (strings, ...values) => ({ strings, values }),
      useState: (initial) => [typeof initial === 'function' ? initial() : initial, () => {}],
      useReducer: () => [0, () => {}], useRef: (current) => ({ current }), useEffect: (fn) => { fn(); },
    },
  });
  S.openView(clone(backend));
  const rendered = Notes.NotesPanel();
  const onInput = rendered.values[rendered.strings.findIndex((s) => s.endsWith('onInput='))];
  onInput({ target: { value: 'Unsaved notes' } });
  assert.equal(await S.confirmDiscard(), false);
  assert.equal(await S.confirmDiscard(), false);
  rejectEdit = false;
  assert.equal(await S.flushEdits(), true);
  assert.equal(backend.deck.slides[0].notes, 'Unsaved notes');
});

for (const kind of ['table', 'text']) {
  test(`a ${kind} edit whose element was removed elsewhere does not block closing`, async () => {
    const backend = view();
    if (kind === 'text') backend.deck.slides[0].elements[0].content = { kind: 'text', html: '<p>Original</p>' };
    const calls = [];
    const { S, T } = await ui(async (name) => {
      calls.push(name);
      // The slide was replaced: the backend no longer has the element.
      if (name === 'doc_apply') throw new Error('element not found');
      if (name === 'recent_list') return [];
    }, {}, { quiet: true });
    S.openView(clone(backend));
    if (kind === 'table') editTable(S, T);
    else editText(S, T).innerHTML = '<p>Typed into the replaced box</p>';
    // Until the window learns about the change, the edit is kept for a retry.
    assert.equal(await T.endAllEditing(), false);
    const replaced = view('document-a', 2);
    replaced.deck.slides[0].elements = [];
    S.ingestView(replaced);
    assert.equal(await T.endAllEditing(), true);
    await S.closeDocument();
    assert.equal(S.state.doc, null);
    assert.equal(calls.filter((n) => n === 'doc_apply').length, 1);
    assert(calls.includes('doc_close'));
  });
}

test('notes of a slide removed elsewhere do not block closing', async () => {
  const calls = [];
  const { S, Notes } = await ui(async (name) => {
    calls.push(name);
    if (name === 'doc_apply') throw new Error('slide not found');
    if (name === 'recent_list') return [];
  }, {}, {
    quiet: true,
    hooks: {
      html: (strings, ...values) => ({ strings, values }),
      useState: (initial) => [typeof initial === 'function' ? initial() : initial, () => {}],
      useReducer: () => [0, () => {}], useRef: (current) => ({ current }), useEffect: (fn) => { fn(); },
    },
  });
  S.openView(clone(view()));
  const rendered = Notes.NotesPanel();
  const onInput = rendered.values[rendered.strings.findIndex((s) => s.endsWith('onInput='))];
  onInput({ target: { value: 'Notes for a slide about to vanish' } });
  const replaced = view('document-a', 2);
  replaced.deck.slides = [{ id: 'slide-2', elements: [] }];
  S.ingestView(replaced);
  assert.equal(await S.confirmDiscard(), true);
  await S.closeDocument();
  assert.equal(S.state.doc, null);
  assert(!calls.includes('doc_apply'));
});

test('out-of-order incremental views refresh missing renderings and ignore stale data', async () => {
  const response = deferred();
  const calls = [];
  const { S } = await ui(async (name) => { calls.push(name); return response.promise; });
  S.openView(view());
  const third = view('document-a', 3);
  third.full = false;
  third.deck.title = 'Latest';
  S.ingestView(third);
  await tick();
  assert.deepEqual(calls, ['doc_refresh']);
  const second = view('document-a', 2);
  second.deck.title = 'Old';
  assert.equal(S.ingestView(second), false);
  // A more recent view arrives while the refresh response is in transit.
  const fifth = view('document-a', 5);
  fifth.deck.title = 'Even newer';
  fifth.slides = { 'slide-1': { html: 'Current rendering' } };
  S.ingestView(fifth);
  response.resolve({ ...third, revision: 4, full: true, slides: { 'slide-1': { html: 'Older rendering' } } });
  await S.flushEdits();
  assert.equal(S.state.doc.deck.title, 'Even newer');
  assert.equal(S.state.doc.slides['slide-1'].html, 'Current rendering');
});

for (const stage of ['fileRead', 'import', 'flush']) {
  test(`an image paste is abandoned if its document changes during ${stage}`, async () => {
    const delayed = deferred();
    const calls = [];
    const asset = { kind: 'image', name: 'photo.png', width: 20, height: 20 };
    const { S, A } = await ui(async (name, args) => { calls.push({ name, args }); return view('document-b', 2); }, {}, {
      importBytes: async (_bytes, _ext, docId) => {
        calls.push({ name: 'import', docId });
        return stage === 'import' ? delayed.promise : asset;
      },
    });
    S.openView(view());
    const unregister = stage === 'flush' ? S.registerFlush(() => delayed.promise) : () => {};
    const pasting = A.onPaste({
      preventDefault() {},
      clipboardData: { getData: () => '', files: [{
        name: 'photo.png', type: 'image/png',
        arrayBuffer: () => stage === 'fileRead' ? delayed.promise : Promise.resolve(new ArrayBuffer(8)),
      }] },
    });
    await tick();
    S.openView(view('document-b'));
    delayed.resolve(stage === 'fileRead' ? new ArrayBuffer(8) : stage === 'import' ? asset : true);
    await pasting;
    unregister();
    assert(!calls.some((c) => c.name === 'doc_apply'));
    assert(calls.filter((c) => c.name === 'import').every((c) => c.docId === 'document-a'));
    if (stage === 'fileRead') assert.equal(calls.length, 0);
  });
}

test('a delayed image paste keeps its original slide and supports ordinary insertion', async () => {
  const bytes = deferred();
  const calls = [];
  const backend = view();
  backend.deck.slides.push({ id: 'slide-2', elements: [] });
  const { S, A } = await ui(async (name, args) => { calls.push({ name, args }); return { ...clone(backend), revision: 2 }; }, {}, {
    importBytes: async (_bytes, _ext, docId) => { assert.equal(docId, 'document-a'); return { kind: 'image', name: 'photo.png', width: 20, height: 20 }; },
  });
  S.openView(clone(backend));
  const pasting = A.onPaste({
    preventDefault() {},
    clipboardData: { getData: () => '', files: [{ name: 'photo.png', type: 'image/png', arrayBuffer: () => bytes.promise }] },
  });
  S.goToSlide('slide-2');
  bytes.resolve(new ArrayBuffer(8));
  await pasting;
  const edit = calls.find((c) => c.name === 'doc_apply');
  assert.equal(edit.args.op.slide, 'slide-1');
  assert.equal(edit.args.docId, 'document-a');
  assert.equal(edit.args.op.props.content.src, 'photo.png');
});

/** One real UI module, with each of its imports stubbed. */
async function standalone(name, stubs, globals = {}) {
  const context = vm.createContext(globals);
  const mod = new vm.SourceTextModule(fs.readFileSync(path.join(root, name), 'utf8'), { context, identifier: name });
  await mod.link((specifier) => {
    const exports = stubs[path.basename(specifier)];
    if (!exports) throw new Error(`${name} imports ${specifier}, which has no stub`);
    return new vm.SyntheticModule(Object.keys(exports), function () {
      for (const [key, value] of Object.entries(exports)) this.setExport(key, value);
    }, { context });
  });
  await mod.evaluate();
  return mod.namespace;
}

const teleprompter = () => standalone('teleprompter.js', { 'i18n.js': { t: (s) => s }, 'icon-data.js': { ICONS: {} } });

test('the teleprompter box stays inside the presenter display', async () => {
  const P = await teleprompter();
  const plain = (r) => ({ ...r });
  assert.deepEqual(plain(P.boxRect({ x: 0.25, y: 0.02, width: 0.5, height: 0.3 }, 1000, 800)), { left: 250, top: 16, width: 500, height: 240 });
  // Off-screen or oversized boxes are pulled back in; tiny ones get a usable size.
  assert.deepEqual(plain(P.boxRect({ x: 0.9, y: -1, width: 0.5, height: 2 }, 1000, 800)), { left: 500, top: 0, width: 500, height: 800 });
  assert.deepEqual(plain(P.boxRect({ x: 0, y: 0, width: 0.01, height: 0.01 }, 1000, 800)), { left: 0, top: 0, width: 240, height: 120 });

  const r = { left: 100, top: 100, width: 400, height: 200 };
  assert.deepEqual(plain(P.dragRect(r, null, 5000, -5000, 1000, 800)), { left: 600, top: 0, width: 400, height: 200 });
  assert.deepEqual(plain(P.dragRect(r, 'w', -50, 0, 1000, 800)), { left: 50, top: 100, width: 450, height: 200 });
  // Shrinking stops at the minimum size and keeps the opposite edge in place.
  assert.deepEqual(plain(P.dragRect(r, 'nw', 1000, 1000, 1000, 800)), { left: 260, top: 180, width: 240, height: 120 });
  assert.deepEqual(plain(P.dragRect(r, 'se', 5000, 5000, 1000, 800)), { left: 100, top: 100, width: 900, height: 700 });
});

test('teleprompter scrolling is steady and stops at the end of the notes', async () => {
  const P = await teleprompter();
  assert.equal(P.advance(0, 40, 0.05, 1000), 2);
  assert.equal(P.advance(0, 40, 5, 1000), 4, 'a stalled frame does not jump ahead');
  assert.equal(P.advance(990, 200, 0.1, 1000), 1000);
  assert.equal(P.clampSpeed(43), 45);
  assert.equal(P.clampSpeed(0), P.SPEED_MIN);
  assert.equal(P.clampSpeed(1e6), P.SPEED_MAX);
  assert.equal(P.rgba('#ff8000', 0.5), 'rgba(255, 128, 0, 0.5)');
  assert.equal(P.rgba('not a color', 2), 'rgba(17, 24, 39, 1)');
});

test('build steps count chart series, pie slices and motion paths', async () => {
  const { A } = await ui(async () => ({}));
  const chart = (kind, values, delivery = 'series', effect = 'cz-wipe-up') => ({
    id: 'chart',
    build: { effect, order: 2, delivery, outOrder: 0, pathOrder: 0, path: [] },
    content: { kind: 'chart', chart: kind, series: [{ values }, { values }, { values }] },
  });
  assert.equal(A.seriesSteps(chart('column', [1, 2])), 3, 'one step per series');
  assert.equal(A.seriesSteps(chart('pie', [3, 0, 2, NaN])), 2, 'one step per non-empty slice');
  assert.equal(A.seriesSteps(chart('column', [1], '')), 0, 'all at once');
  assert.equal(A.seriesSteps(chart('column', [1], 'series', '')), 0, 'no build-in');

  const mover = {
    id: 'mover',
    build: { effect: 'fade-in', order: 1, outOrder: 9, pathOrder: 6, path: [[0, 0], [100, 0]] },
    content: { kind: 'shape' },
  };
  const still = { id: 'still', build: { effect: '', order: 0, outOrder: 0, pathOrder: 0, path: [[0, 0]] }, content: { kind: 'text' } };
  const slide = { id: 's', elements: [chart('column', [1]), mover, still] };
  assert.deepEqual(
    A.buildEvents(slide).map((e) => `${e.el.id}:${e.kind}:${e.step}`),
    ['mover:in:1', 'chart:in:2', 'mover:path:6'],
    'a single point is not a path',
  );
  assert.equal(A.nextBuildStep(slide), 10, 'after the build-out at step 9');
  mover.build.outOrder = 0;
  mover.build.pathOrder = 3;
  assert.equal(A.nextBuildStep(slide), 6, 'after the chart’s last series (2 + 3)');
});

test('each chart type builds by its own unit and keeps edited data', async () => {
  const { A, Charts } = await ui(async () => ({}));
  const el = (chart, series, extra = {}) => ({
    build: { effect: 'fade-in', order: 1, delivery: 'series' },
    content: { kind: 'chart', chart, labels: [], series: series.map((values) => ({ name: '', values })), ...extra },
  });
  const one = [[3, 0, 2, 1]];
  const three = [[1], [2], [3]];
  assert.equal(A.seriesSteps(el('parliament', one)), 3, 'parties with seats');
  assert.equal(A.seriesSteps(el('treemap', one)), 3, 'rectangles');
  assert.equal(A.seriesSteps(el('treemap', three)), 3, 'groups');
  assert.equal(A.seriesSteps(el('scatter', three)), 2, 'the first series is X');
  assert.equal(A.seriesSteps(el('heatmap', three)), 3, 'columns');
  assert.equal(A.seriesSteps(el('candlestick', three)), 0, 'candles build all at once');
  assert.equal(A.seriesSteps(el('column', three, { stacked: true })), 3, 'older stacked columns');
  assert.equal(Charts.chartKind({ chart: 'bar', stacked: true }), 'stacked-bar');

  // Untouched sample data follows the type; edited data stays.
  const sample = { chart: 'parliament', ...Charts.chartSample('parliament') };
  const toTree = Charts.switchChart(sample, 'treemap');
  assert.deepEqual(toTree.labels, Charts.chartSample('treemap').labels);
  assert.equal(toTree.stacked, false);
  const edited = { ...sample, series: [{ ...sample.series[0], values: [1, 2, 3, 4, 5] }] };
  const kept = Charts.switchChart(edited, 'treemap');
  assert.equal(kept.labels, undefined, 'edited data is kept');
  const candles = Charts.switchChart(edited, 'candlestick');
  assert.equal(candles.series.length, 4, 'candlesticks get the series they need');
  assert.deepEqual(candles.series[0].values, [1, 2, 3, 4, 5]);

  // Sankey diagrams and networks count steps as chart.rs does (its tests use the same flows).
  const links = (rows) => rows.map(([from, to, value]) => ({ from, to, value }));
  const flows = links([
    ['S1', 'M1', 30], ['S2', 'M1', 20], ['S2', 'M2', 15], ['S3', 'M2', 25], ['M1', 'T1', 35], ['M1', 'T2', 15],
    ['M2', 'T2', 30], ['S3', 'T3', 10], ['', 'X', 5], ['Y', 'Y', 5], ['Z', 'W', 0],
  ]);
  assert.deepEqual([...Charts.sankeyColumns(Charts.graph({ links: flows }, true))], [0, 1, 0, 1, 0, 2, 2, 2]);
  assert.equal(A.seriesSteps(el('sankey', [], { links: flows })), 2, 'one step per column after the first');
  const net = links([['A', 'B', 3], ['A', 'C', 0], ['B', 'C', 2], ['C', 'D', 1], ['E', 'F', 1]]);
  assert.equal(A.seriesSteps(el('network', [], { links: net })), 6, 'node by node');
  const cycle = Charts.sankeyColumns(Charts.graph({ links: links([['A', 'B', 1], ['B', 'C', 1], ['C', 'A', 1]]) }, true));
  assert.ok(cycle.every((c) => c < 3), 'a cycle stops growing');
  // Switching to a link chart fills in sample links only when there are none.
  assert.equal(Charts.switchChart({ ...sample, links: [] }, 'sankey').links.length, 8);
  assert.equal(Charts.switchChart({ ...sample, links: net }, 'sankey').links, undefined, 'written links are kept');

  // Maps color regions all at once; they start on the world.
  assert.equal(A.seriesSteps(el('map', one)), 0);
  const basic = { chart: 'column', ...Charts.chartSample('column') };
  const toMap = Charts.switchChart(basic, 'map');
  assert.equal(toMap.map, 'world');
  assert.ok(Charts.isSample({ ...basic, ...toMap }, 'map'), 'the sample becomes the map sample');
  const mine = Charts.switchChart(edited, 'map');
  assert.equal(mine.map, 'world', 'edited data is kept, on the world map');
  assert.equal(mine.labels, undefined);
});

test('every chart type can become every other type', async () => {
  const { Charts } = await ui(async () => ({}));
  const kinds = Object.keys(Charts.CHART_ICONS);
  const fresh = () => ({ chart: 'column', stacked: false, links: [], map: '', ...Charts.chartSample('column') });
  for (const from of kinds) {
    for (const to of kinds) {
      if (from === to) continue;
      const chart = fresh();
      Object.assign(chart, Charts.switchChart(chart, from));
      assert.equal({ ...chart, ...Charts.switchChart(chart, to) }.chart, to, `${from} → ${to}`);
    }
  }
  // Untouched data still follows the type after a Sankey diagram.
  const sankey = fresh();
  Object.assign(sankey, Charts.switchChart(sankey, 'sankey'));
  assert.ok(Charts.isSample({ ...sankey, ...Charts.switchChart(sankey, 'treemap') }, 'treemap'));
});

test('changing the chart type keeps the colors picked for series', async () => {
  const { Charts } = await ui(async () => ({}));
  let chart = { chart: 'column', stacked: false, links: [], map: '', ...Charts.chartSample('column') };
  // One series with a color of its own, one with the theme's.
  chart.series = chart.series.map((s, k) => ({ ...s, color: k === 0 ? '#ef4444' : '' }));
  const colors = () => clone(chart.series.map((s) => s.color));
  const values = clone(chart.series.map((s) => s.values));
  for (const kind of ['line', 'area']) {
    chart = { ...chart, ...Charts.switchChart(chart, kind) };
    assert.deepEqual(colors(), ['#ef4444', ''], kind);
    assert.deepEqual(clone(chart.series.map((s) => s.values)), values, kind);
  }
  chart = { ...chart, ...Charts.switchChart(chart, 'treemap') };
  assert.deepEqual(colors(), ['#ef4444'], 'the treemap sample has one series');
  chart = { ...chart, ...Charts.switchChart(chart, 'column') };
  assert.deepEqual(colors(), ['#ef4444', '']);
});

/**
 * controls.js with just enough of Preact's hooks to render a control again
 * and call its handlers. `field` stands for the control's <input>.
 */
async function control(name, props) {
  const flushers = new Set();
  const slots = [];
  let at = 0;
  const slot = (init) => (at in slots ? slots[at++] : (slots[at++] = init()));
  const lib = {
    html: (strings, ...values) => ({ strings, values }),
    useState(initial) {
      const s = slot(() => ({ v: typeof initial === 'function' ? initial() : initial }));
      return [s.v, (v) => { s.v = typeof v === 'function' ? v(s.v) : v; }];
    },
    useRef: (current) => slot(() => ({ current })),
    useMemo: (f) => f(),
    useLayoutEffect() {},
    useEffect(fn, deps) {
      const s = slot(() => ({ deps: null }));
      if (!s.deps || deps.some((d, k) => d !== s.deps[k])) {
        s.deps = deps;
        fn();
      }
    },
  };
  const document = { activeElement: null };
  const C = await standalone('controls.js', {
    'lib.js': lib, 'icons.js': { Icon() {} }, 'store.js': { state: {}, registerFlush: (fn) => (flushers.add(fn), () => flushers.delete(fn)) },
    'util.js': { clamp: (v, lo, hi) => Math.min(hi, Math.max(lo, v)), cls: (...a) => a.filter(Boolean).join(' ') },
    'i18n.js': { t: (s) => s, N_: (s) => s },
  }, { window: {}, document, setTimeout, clearTimeout });
  let out;
  // Attributes are either `name=${…}` or spread from one object (TextField).
  const attr = (key) => {
    const i = out.strings.findIndex((s) => s.endsWith(` ${key}=`) || s.endsWith(`\n${key}=`));
    return i >= 0 ? out.values[i] : out.values.find((v) => v && typeof v === 'object' && key in v)?.[key];
  };
  const field = {
    value: '',
    select() {},
    // The browser runs the blur handler at once, the one of the last render.
    blur: () => { document.activeElement = null; attr('onBlur')({ target: field }); },
  };
  const render = (more = {}) => {
    at = 0;
    out = C[name]({ ...props, ...more });
    attr('ref').current = field;
    field.value = String(attr('value'));
    return out;
  };
  render();
  const on = (event, e = {}) => attr(event)({ target: field, preventDefault() {}, ...e });
  const focus = () => { document.activeElement = field; on('onFocus'); render(); };
  const type = (text) => { field.value = text; on('onInput'); render(); };
  const flush = () => Promise.all([...flushers].map((fn) => fn()));
  return { render, on, focus, type, flush, attr };
}

test('Escape leaves a number field without committing what was typed', async () => {
  const changes = [];
  const f = await control('NumberField', { value: 94, min: 4, max: 800, onChange: (v) => changes.push(v) });
  f.focus();
  f.type('48');
  f.on('onKeyDown', { key: 'Escape' });
  assert.deepEqual(changes, [], 'Escape cancels');
  f.focus();
  f.type('48');
  let prevented = false;
  f.on('onKeyDown', { key: 'Enter', preventDefault: () => (prevented = true) });
  assert.deepEqual(changes, [48], 'Enter commits once');
  // Committing a size can put the focus back in selected text: the Enter
  // must not go on to replace it.
  assert.ok(prevented, 'the Enter itself goes nowhere');
});

test('Escape cancels a one-line text field and keeps what a multiline one got', async () => {
  for (const multiline of [false, true]) {
    const changes = [];
    const f = await control('TextField', { value: 'Old', multiline, onChange: (v) => changes.push(v) });
    f.focus();
    f.type('New');
    f.on('onKeyDown', { key: 'Escape' });
    assert.deepEqual(changes, multiline ? ['New'] : [], multiline ? 'multiline' : 'one line');
  }
  const changes = [];
  const f = await control('TextField', { value: 'Old', onChange: (v) => changes.push(v) });
  f.focus();
  f.type('New');
  f.on('onKeyDown', { key: 'Enter' });
  assert.deepEqual(changes, ['New'], 'Enter commits once');
});

test('saving commits a chart cell that is still being typed in', async () => {
  const changes = [];
  const cell = await control('Cell', { value: 12, number: true, onChange: (v) => changes.push(v) });
  cell.focus();
  cell.type('99');
  // Another update while typing (the "Saving…" status) leaves the text alone.
  cell.render({ value: 12 });
  assert.equal(cell.attr('value'), '99');
  await cell.flush();
  assert.deepEqual(changes, [99]);
  cell.render({ value: 99 });
  cell.on('onChange');
  assert.deepEqual(changes, [99], 'the same value is not committed twice');
  const label = await control('Cell', { value: 'Q1', onChange: (v) => changes.push(v) });
  label.focus();
  label.type('Spring');
  await label.flush();
  assert.deepEqual(changes, [99, 'Spring']);
});

test('inline math is typeset again after editing without changes', async () => {
  const backend = view();
  backend.deck.slides[0].elements = ['text-1', 'text-2'].map((id) => ({ id, content: { kind: 'text', html: '<p>$x^2$</p>' } }));
  const { S, T } = await ui(async (name) => (name === 'recent_list' ? [] : {}));
  S.openView(clone(backend));
  // Rust's rendering of the two boxes; each update parses it into new nodes.
  const typeset = '<span class="cz-math-inline">x²</span>';
  const src = (id) => `<div data-cz-id="${id}"><div class="cz-text">${typeset}</div></div>`;
  const html = src('text-1') + src('text-2');
  const root = {
    children: [],
    get firstChild() { return this.children[0] ?? null; },
    insertBefore(n, before) {
      const list = this.children.filter((x) => x !== n);
      list.splice(before ? list.indexOf(before) : list.length, 0, n);
      this.children = list;
    },
    querySelector(sel) { return this.children.find((n) => sel.includes(`"${n.id}"`)) ?? null; },
  };
  const node = (id) => {
    const text = {
      innerHTML: typeset, classList: { contains: () => false, remove() {} }, querySelector: (sel) => (sel.includes('cz-math') ? {} : null),
      setAttribute() {}, removeAttribute() {}, focus() {}, contains: () => false, addEventListener() {}, removeEventListener() {},
    };
    const n = {
      id, text, outerHTML: src(id), getAttribute: (k) => (k === 'data-cz-id' ? id : null),
      querySelector: (sel) => (sel === '.cz-text' ? text : null), querySelectorAll: () => [], remove() {},
      get nextSibling() { return root.children[root.children.indexOf(n) + 1] ?? null; },
      replaceWith(other) { root.children[root.children.indexOf(n)] = other; },
    };
    return n;
  };
  const document = { createElement: () => ({ set innerHTML(_) {}, content: { get children() { return [node('text-1'), node('text-2')]; } } }) };
  const SV = await standalone('slideview.js', { 'i18n.js': { t: (s) => s, N_: (s) => s, language: () => 'en' } }, { window: {}, document, CSS: { escape: (s) => s } });
  const slideView = new SV.SlideView(root);
  slideView.update(html);
  assert.equal(slideView.update(html), false, 'nothing to draw');

  await T.beginTextEdit(slideView, S.currentSlide().elements[0]);
  assert.equal(root.children[0].text.innerHTML, '<p>$x^2$</p>', 'the editor shows the LaTeX');
  await T.endTextEdit();
  // Editing moves straight on to the other box: no forced update.
  slideView.editingId = 'text-2';
  assert.equal(slideView.update(html), true);
  assert.equal(root.children[0].text.innerHTML, typeset, 'the formula is typeset again');
  assert.equal(root.children.length, 2);
});
