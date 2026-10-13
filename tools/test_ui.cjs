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
    document: { title: 'Cadenza', getElementById: () => null, createRange: () => ({ selectNodeContents() {}, collapse() {} }) },
    window: { getSelection: () => ({ removeAllRanges() {}, addRange() {} }), dispatchEvent() {} },
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
