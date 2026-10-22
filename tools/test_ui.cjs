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

async function ui(invoke, clipboard = {}, { importBytes = () => {}, hooks = {}, quiet = false, api = {} } = {}) {
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
      invoke, clipboard, importBytes, dialog: {}, opener: {}, events: { listen() {}, emit() {} },
      FILE_FILTER: [], IMAGE_EXTS: [], VIDEO_EXTS: [], currentWindow: () => ({ setTitle: async () => {} }), getAllWindows: async () => [],
      ...api,
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

test('an edit Rust rejects stops saving and closing until it is fixed or dropped', async () => {
  const calls = [];
  const { S } = await ui(async (name) => {
    calls.push(name);
    if (name === 'doc_apply') throw 'Element not found';
    if (name === 'recent_list') return [];
  }, {}, { quiet: true });
  S.openView(view());
  // A focused field's flush forwards its edit, which Rust refuses.
  const off = S.registerFlush(() => S.updateDeck({ title: 'Typed' }));
  assert.equal(await S.save(), false);
  await S.closeDocument();
  assert(!calls.includes('doc_save') && !calls.includes('doc_close'));
  off(); // Escape, or the field went away
  await S.closeDocument();
  assert(calls.includes('doc_close'));
});

test('saving waits for an edit that is still being prepared', async () => {
  const backend = view();
  const calls = [];
  const { S } = await ui(async (name, args) => {
    calls.push(name);
    if (name === 'doc_apply') {
      backend.deck.title = args.op.patch.title;
      backend.revision++;
      return clone(backend);
    }
    if (name === 'doc_save') return { ...clone(backend), revision: ++backend.revision };
    if (name === 'recent_list') return [];
  });
  S.openView(clone(backend));
  // A diagram's source goes in once Mermaid has drawn it, even if its
  // field is gone by then.
  const drawing = deferred();
  S.trackEdit(drawing.promise.then(() => S.updateDeck({ title: 'Drawn' })));
  const saving = S.save();
  await tick();
  assert(!calls.includes('doc_save'));
  drawing.resolve();
  assert.equal(await saving, true);
  assert.deepEqual(calls.filter((c) => c !== 'recent_list'), ['doc_apply', 'doc_save']);
});

test('nudges sent before earlier ones are back add up', async () => {
  const backend = view();
  backend.deck.slides[0].elements = [{ id: 'box', x: 10, y: 20, content: { kind: 'shape' } }];
  const replies = [];
  const { S, A } = await ui(async (name, args) => {
    if (name !== 'doc_apply') return {};
    const reply = deferred();
    replies.push(reply);
    await reply.promise;
    Object.assign(backend.deck.slides[0].elements[0], args.op.updates[0].patch);
    backend.revision++;
    return clone(backend);
  });
  S.openView(clone(backend));
  S.select(['box']);
  for (let i = 0; i < 5; i++) A.nudge(1, 0);
  A.nudge(0, -2);
  for (let i = 0; i < 6; i++) {
    while (replies.length <= i) await tick();
    replies[i].resolve();
  }
  await S.flushEdits();
  const { x, y } = backend.deck.slides[0].elements[0];
  assert.deepEqual([x, y], [15, 18]);
});

test('an update waits for edits in every editor window', async () => {
  for (const answer of [true, false, null]) {
    const listeners = new Map();
    const events = {
      listen: async (name, fn) => (listeners.set(name, fn), () => listeners.delete(name)),
      emit(name, payload) {
        // The other editor window commits what is typed there, and answers.
        if (name === 'cz-flush-all' && answer !== null) {
          setTimeout(() => listeners.get('cz-flush-done')?.({ payload: { token: payload.token, label: 'editor-2', ok: answer } }));
        }
      },
    };
    const windows = ['editor-1', 'editor-2', 'present-editor-1'].map((label) => ({ label }));
    const { S } = await ui(async () => ({}), {}, {
      api: { events, getAllWindows: async () => windows, currentWindow: () => ({ label: 'editor-1', setTitle: async () => {} }) },
    });
    S.openView(view());
    assert.equal(await S.flushAllWindows(50), answer === true, `answer: ${answer}`);
    assert(!listeners.has('cz-flush-done'));
  }
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

test('a presentation’s own CSS is kept to the slides in the editor', async () => {
  const U = await standalone('util.js', { 'i18n.js': { t: (s) => s, language: () => 'en' } }, { navigator: { platform: 'Linux' } });
  const css = [
    '@import url("x.css");',
    '/* a { } */ :root { --r-heading-color: red; }',
    '@font-face { font-family: "A}"; src: url(a.woff2); }',
    '.toolbar { display: none !important; }',
    '@media (min-width: 1px) { .reveal h1 { color: blue; } }',
    '@keyframes spin { from { rotate: 0deg; } to { rotate: 1turn; } }',
    'a::after { content: "}"; }',
  ].join('\n');
  const out = U.scopeCss(css, '.root');
  const [outside, inside] = out.split('@scope (.root) {');
  for (const rule of ['@import', '@font-face', '@keyframes']) assert(outside.includes(rule) && !inside.includes(rule), rule);
  for (const rule of [':scope { --r-heading-color: red; }', '.toolbar { display: none !important; }', '@media (min-width: 1px) { .reveal h1 { color: blue; } }', 'a::after { content: "}"; }']) {
    assert(inside.includes(rule), rule);
  }
  assert(inside.trimEnd().endsWith('}') && !inside.includes(':root'));
});

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

const chartdata = () => standalone('chartdata.js', { 'i18n.js': { t: (s, v) => s.replace(/\{(\w+)\}/g, (_, k) => v[k]), N_: (s) => s } });
const plainJson = (v) => JSON.parse(JSON.stringify(v));
// Arrays made inside the module's context: compared by their contents.
const eq = (actual, expected, message) => assert.deepEqual(plainJson(actual), plainJson(expected), message);
const KINDS = ['column', 'stacked-column', 'bar', 'stacked-bar', 'line', 'dot', 'area', 'stacked-area', 'streamgraph', 'pie', 'donut', 'treemap', 'parliament', 'scatter', 'heatmap', 'candlestick', 'sankey', 'network', 'map'];

test('data in CSV form is read with its quotes, braces, separators and decimal commas', async () => {
  const D = await chartdata();
  eq((D.parseRows('a,"b, c","say ""hi"""\n\n1,2,3\r\n')), [['a', 'b, c', 'say "hi"'], ['1', '2', '3']]);
  // pgfplots tables: braces keep commas, LaTeX commands become text.
  eq((D.parseRows('label,gain\n{regressão, banco (r5x)},3.04\n{\\emph{plug-in} (q9x)},0.85')), [
    ['label', 'gain'], ['regressão, banco (r5x)', '3.04'], ['plug-in (q9x)', '0.85'],
  ]);
  assert.equal(D.delimiterOf('a;b;c\n1,5;2,5;3'), ';');
  assert.equal(D.delimiterOf('a\tb, c\n1\t2'), '\t', 'cells copied from a spreadsheet');
  const n = (s, d = ',') => D.parseNumber(s, d);
  assert.equal(n('0,8549', ';'), 0.8549, 'decimal comma');
  assert.equal(n('1.234,5', ';'), 1234.5);
  assert.equal(n('1,234.5'), 1234.5);
  assert.equal(n('1,234'), 1234, 'thousands, when commas separate cells');
  assert.equal(n('−3'), -3);
  assert.equal(n('R$ 12'), 12);
  assert.equal(n('45%'), 45);
  assert.equal(n(''), null);
  assert.equal(n('n/a'), null);
  assert.ok(Number.isNaN(n('Q1')) && Number.isNaN(n('1,2,3')));
  const table = D.readTable('Ano;Vendas\n2024;1,5\n2025;2,25');
  assert.ok(table.header);
  eq((table.columns.map((c) => c.numbers)), [[2024, 2025], [1.5, 2.25]]);
  assert.equal(D.readTable('1,2\n3,4').header, false, 'numbers only: no names');
  assert.equal(D.readTable('\t2025\t2026\nQ1\t1\t2').header, true, 'an empty corner names the columns');
  eq((D.readTable('a,b\n1,2', { transpose: true }).columns.map((c) => c.cells)), [['a', 'b'], ['1', '2']], 'swapped: a column of names, one of numbers');
});

test('the columns of real data get their roles and make the charts they can', async () => {
  const D = await chartdata();
  // Error bars as amounts below and above, three series, an index column.
  const groups = D.readTable(`idx,group,r9decx,r9decxminus,r9decxplus,meta,metaminus,metaplus,uniform,uniformminus,uniformplus
0.0000,{completo},0.0628,0.2085,0.1691,0.5286,0.5365,0.6413,-0.7036,0.2896,0.2679
1.0000,{ausências},0.0852,0.1844,0.1736,0.5821,0.3277,0.4347,-0.6199,0.2623,0.2557
2.0000,{local uniforme},0.0483,0.2108,0.1706,0.5264,0.5335,0.6406,-0.6996,0.2964,0.2724`);
  const roles = D.guessRoles(groups);
  eq(roles, ['ignore', 'label', 'value', 'minus', 'plus', 'value', 'minus', 'plus', 'value', 'minus', 'plus']);
  const data = D.chartData(groups, roles);
  assert.equal(data.errorBars, 'amounts');
  eq(data.labels, ['completo', 'ausências', 'local uniforme']);
  eq(data.series.map((s) => s.name), ['r9decx', 'meta', 'uniform']);
  eq(data.series[2].low, [0.2896, 0.2623, 0.2964]);
  const kinds = D.chartChoices(groups, roles, KINDS).map((c) => c.kind);
  assert.ok(['column', 'bar', 'stacked-column', 'dot', 'line', 'heatmap', 'scatter'].every((k) => kinds.includes(k)), kinds.join());
  assert.ok(!kinds.includes('pie') && !kinds.includes('treemap'), 'negative values');

  // One column of values, errors named alone, rows sorted into groups.
  const gain = D.readTable(`idx,label,kind,gain,minus,plus
0.0000,{1ª rodada},{fusao},1.4397,0.3262,0.3587
1.0000,{plug-in (q9x)},{fusao},0.8504,0.3562,0.3849
2.0000,{r9decx},{escolhida},3.8060,0.4536,0.4764
3.0000,{regra},{regra},5.7045,0.6967,0.6736`);
  const gainRoles = D.guessRoles(gain);
  eq(gainRoles, ['ignore', 'label', 'group', 'value', 'minus', 'plus']);
  const split = D.chartData(gain, gainRoles);
  eq(split.series.map((s) => s.name), ['fusao', 'escolhida', 'regra']);
  eq(split.series[1].values, [null, null, 3.806, null]);
  eq(split.series[1].high, [null, null, 0.4764, null]);
  const choices = D.chartChoices(gain, gainRoles, KINDS);
  assert.ok(choices.some((c) => c.kind === 'stacked-bar'), 'one bar per row, colored by group');
  assert.ok(choices.some((c) => c.kind === 'sankey'), 'labels flow into their groups');

  // A column with one value throughout says nothing; the other names the rows.
  const family = D.readTable('modality,family,joint,r9decx,rule\n{T1ce},{ruído},0.8549,0.8536,0.7656\n{T1ce},{ganho},0.7075,0.7336,0.7857');
  eq(D.guessRoles(family), ['ignore', 'label', 'value', 'value', 'value']);

  // Lower and upper ends, and ± given beside them, become ends.
  const ends = D.readTable('x,mean,mean lower,mean upper,other,other sd\na,10,8,13,5,1\nb,20,15,22,6,2');
  const endRoles = D.guessRoles(ends);
  eq(endRoles, ['label', 'value', 'low', 'high', 'value', 'pm']);
  const both = D.chartData(ends, endRoles);
  assert.equal(both.errorBars, 'range');
  eq([both.series[1].low, both.series[1].high], [[4, 4], [6, 8]]);

  // Errors named in other languages.
  const pt = D.readTable('Ano;Preço médio;Desvio padrão\n2022;10,5;1,2\n2023;12,25;0,8');
  eq(D.guessRoles(pt), ['label', 'value', 'pm']);

  // Candlesticks need high and low around opening and closing.
  const ohlc = D.readTable('day,open,high,low,close\n1,10,12,9,11\n2,11,13,10,12\n3,12,12.5,10,10.5');
  assert.ok(D.chartChoices(ohlc, D.guessRoles(ohlc), KINDS).some((c) => c.kind === 'candlestick'));
  // A scatterplot of numbers only: the first column is X.
  const xy = D.readTable('x,y\n1,2\n2,4\n3,5');
  const scatter = D.chartChoices(xy, D.guessRoles(xy), KINDS).find((c) => c.kind === 'scatter');
  eq((scatter.content.series.map((s) => s.values)), [[1, 2, 3], [2, 4, 5]]);
  // Pies and parliaments take one series of positive values.
  const seats = D.readTable('party,seats\nA,120\nB,95\nC,60');
  const seatKinds = D.chartChoices(seats, D.guessRoles(seats), KINDS).map((c) => c.kind);
  assert.ok(['pie', 'donut', 'parliament', 'treemap'].every((k) => seatKinds.includes(k)), seatKinds.join());
});

test('values the data does not have stay empty, and groups share their labels', async () => {
  const D = await chartdata();
  // Each label in one group: one dot per row, no zeros elsewhere.
  const qa = D.readTable('Group,Estimate,Low,High,Series\nAlpha,10,8,12,A\nBeta,18,15,21,A\nGamma,13,11,15,B\nDelta,22,19,25,B');
  const roles = D.guessRoles(qa);
  eq(roles, ['label', 'value', 'low', 'high', 'group']);
  const data = D.chartData(qa, roles);
  eq(data.labels, ['Alpha', 'Beta', 'Gamma', 'Delta']);
  eq(data.series.map((s) => [s.name, s.values, s.low]), [
    ['A', [10, 18, null, null], [8, 15, null, null]],
    ['B', [null, null, 13, 22], [null, null, 11, 19]],
  ]);
  const qaKinds = D.chartChoices(qa, roles, KINDS).map((c) => c.kind);
  assert.ok(qaKinds.includes('dot') && !qaKinds.includes('scatter'), 'no row has both an X and a Y: ' + qaKinds.join());
  // Long data: a label for each group in turn becomes one row.
  const long = D.readTable('year,country,sales\n2024,BR,5\n2024,PT,3\n2025,BR,7\n2026,PT,4');
  const longRoles = ['label', 'group', 'value'];
  const pivot = D.chartData(long, longRoles);
  eq(pivot.labels, ['2024', '2025', '2026']);
  eq(pivot.series.map((s) => [s.name, s.values]), [['BR', [5, 7, null]], ['PT', [3, null, 4]]]);
  const scatter = D.chartChoices(long, longRoles, KINDS).find((c) => c.kind === 'scatter');
  eq(scatter.content.series.map((s) => s.values), [[2024, 2025, 2026], [5, 7, null], [3, null, 4]], 'X for each row of the groups');
  // Without labels, the rows of each group are numbered.
  const plain = D.chartData(D.readTable('kind,value\nA,1\nA,2\nB,3\nB,4\nB,5'), ['group', 'value']);
  eq(plain.labels, ['1', '2', '3']);
  eq(plain.series.map((s) => s.values), [[1, 2, null], [3, 4, 5]]);
  // An empty cell is no value, not zero; charts that need every value don't fit.
  const gaps = D.readTable('item,a\nx,4\ny,\nz,2');
  eq(D.chartData(gaps, D.guessRoles(gaps)).series[0].values, [4, null, 2]);
  const kinds = D.chartChoices(gaps, D.guessRoles(gaps), KINDS).map((c) => c.kind);
  assert.ok(kinds.includes('pie') && kinds.includes('dot'), kinds.join());
  const ohlc = D.readTable('day,open,high,low,close\n1,10,12,9,11\n2,11,,10,12');
  assert.ok(!D.chartChoices(ohlc, D.guessRoles(ohlc), KINDS).some((c) => c.kind === 'candlestick'));
});

test('a chart written as CSV reads back the same', async () => {
  const D = await chartdata();
  for (const errorBars of ['', 'pm', 'amounts', 'range']) {
    const chart = {
      chart: 'column', errorBars, labels: ['Q1', 'Q2, late'],
      series: [{ name: '2025', values: [12, 19.5], low: [1, null], high: [2, 3] }, { name: 'B "x"', values: [-1, 0], low: [0.5, 0.5], high: [0.5, 1] }],
    };
    const table = D.readTable(D.toCsv(chart));
    const data = D.chartData(table, D.guessRoles(table));
    eq(data.labels, chart.labels, errorBars);
    eq(data.series.map((s) => [s.name, s.values]), chart.series.map((s) => [s.name, s.values]), errorBars);
    assert.equal(data.errorBars, errorBars);
    if (errorBars === 'pm') eq(data.series[0].high, [2, 3]);
    else if (errorBars) eq([data.series[0].low, data.series[1].high], [[1, null], [0.5, 1]], errorBars);
  }
});

test('large data is read without spreading its rows into one call', async () => {
  const D = await chartdata();
  const rows = D.parseRows('x,y\n' + '1,2\n'.repeat(300000));
  assert.equal(rows.length, 300001);
  assert.equal(rows[300000].length, 2);
});

test('rows repeating a group and label are counted, and error bars come from the row charted', async () => {
  const D = await chartdata();
  const table = D.readTable('item,kind,value,low,high\na,X,1,,1.5\na,X,9,8,10\nb,X,,1.9,\nb,X,2,,2.5\nb,Y,3,2,4\nc,Y,4,3,5');
  const roles = ['label', 'group', 'value', 'low', 'high'];
  assert.equal(D.repeatedRows(table, roles), 2, 'a in X and b in X come twice');
  const data = D.chartData(table, roles);
  eq(data.labels, ['a', 'b', 'c']);
  // The first row with a value, and its own ends: none borrowed from another row.
  eq(data.series.map((s) => [s.name, s.values, s.low, s.high]), [
    ['X', [1, 2, null], [null, null, null], [1.5, 2.5, null]],
    ['Y', [null, 3, 4], [null, 2, 3], [null, 4, 5]],
  ]);
  assert.equal(D.repeatedRows(table, ['ignore', 'group', 'value', 'low', 'high']), 0, 'rows of a group without labels are numbered');
  assert.equal(D.repeatedRows(D.readTable('item,kind,v\na,X,1\na,Y,2\nb,X,3'), ['label', 'group', 'value']), 0);
});

test('the data grid of a scatterplot made from numbers keeps its points', async () => {
  const D = await chartdata();
  const C = await standalone('charts.js', { 'i18n.js': { t: (s, v = {}) => s.replace(/\{(\w+)\}/g, (_, k) => v[k]), N_: (s) => s } });
  const xy = D.readTable('x,y\n1,2\n2,4\n3,5');
  const chart = plainJson(D.chartChoices(xy, D.guessRoles(xy), KINDS).find((c) => c.kind === 'scatter').content);
  eq(chart.labels, []);
  assert.equal(C.chartRows(chart), 3, 'a grid row per point, as chart.rs counts rows');
  const points = (c) => plainJson(c.series.map((s) => s.values));
  // Editing a point leaves the others as they were.
  const edited = { ...chart, ...C.setChartCell(chart, 1, 1, 40) };
  eq(points(edited), [[1, 2, 3], [2, 40, 5]]);
  eq(edited.labels, []);
  // Labels stay optional: writing one gives the rows before it blank ones.
  eq(C.setChartLabel(edited, 2, 'outlier').labels, ['', '', 'outlier']);
  // A new row is added after the points, still without labels.
  const added = { ...edited, ...C.addChartRow(edited) };
  eq(points(added), [[1, 2, 3, 0], [2, 40, 5, 0]]);
  eq(added.labels, []);
  eq(points({ ...added, ...C.deleteChartRow(added, 0) }), [[2, 3, 0], [40, 5, 0]]);
  // Edit as CSV gives the same points back.
  const table = D.readTable(D.toCsv(edited));
  const again = D.chartChoices(table, D.guessRoles(table), KINDS).find((c) => c.kind === 'scatter').content;
  eq(points(again), points(edited));
  eq(again.labels, []);

  // Error bars stay beside their values when rows come and go.
  const withErrors = {
    labels: ['Q1'],
    series: [{ name: 'a', values: [1, 2, 3], low: [0.5], high: [] }, { name: 'b', values: [4, 5, 6], low: [0.1, 0.2, 0.3], high: [1, null, 3] }],
  };
  assert.equal(C.chartRows(withErrors), 3);
  const more = C.addChartRow(withErrors);
  eq(more.labels, ['Q1', '', '', 'Item 4']);
  eq(more.series.map((s) => [s.values, s.low, s.high]), [
    [[1, 2, 3, 0], [0.5, null, null, null], []],
    [[4, 5, 6, 0], [0.1, 0.2, 0.3, null], [1, null, 3, null]],
  ]);
  const fewer = C.deleteChartRow(withErrors, 1);
  eq(fewer.series.map((s) => [s.values, s.low, s.high]), [
    [[1, 3], [0.5], []],
    [[4, 6], [0.1, 0.3], [1, 3]],
  ]);
  const err = C.setChartCell(withErrors, 1, 2, 9, 'high');
  eq(err.series[1], { ...withErrors.series[1], high: [1, null, 9] });
  eq(err.series[0], withErrors.series[0]);
  eq(C.addChartRow({ labels: [], series: [{ name: 'a', values: [] }] }).labels, ['Item 1'], 'an empty chart starts its labels');
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
    'util.js': { clamp: (v, lo, hi) => Math.min(hi, Math.max(lo, v)), cls: (...a) => a.filter(Boolean).join(' '), trackPointer() {} },
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

/**
 * Just enough of Preact's hooks to render components by hand: each instance
 * keeps its hook slots, effects run after the render (`ready` sees the
 * output first, to attach refs) and `unmount` runs their cleanups.
 */
function hookHarness() {
  let cur = null;
  let pending = [];
  const slot = (init) => (cur.at in cur.slots ? cur.slots[cur.at++] : (cur.slots[cur.at++] = init()));
  const effect = (fn, deps) => {
    const s = slot(() => ({ deps: null, cleanup: null }));
    if (!s.deps || deps.some((d, k) => d !== s.deps[k])) {
      s.deps = deps;
      pending.push([s, fn]);
    }
  };
  const lib = {
    html: (strings, ...values) => ({ strings, values }),
    useState(initial) {
      const s = slot(() => ({ v: typeof initial === 'function' ? initial() : initial }));
      return [s.v, (v) => { s.v = typeof v === 'function' ? v(s.v) : v; }];
    },
    useRef: (current) => slot(() => ({ current })),
    useMemo: (f) => f(),
    useEffect: effect,
    useLayoutEffect: effect,
  };
  const render = (inst, C, props, ready) => {
    cur = inst;
    inst.at = 0;
    const out = C(props);
    ready?.(out);
    const run = pending;
    pending = [];
    for (const [s, fn] of run) {
      s.cleanup?.();
      s.cleanup = fn() || null;
    }
    return out;
  };
  const unmount = (inst) => inst.slots.forEach((s) => s?.cleanup?.());
  return { lib, render, unmount, instance: () => ({ slots: [], at: 0 }) };
}

/** The value of attribute `name` in a template from the `html` stub, searched depth-first. */
function attrOf(tpl, name) {
  if (Array.isArray(tpl)) {
    for (const x of tpl) {
      const v = attrOf(x, name);
      if (v !== undefined) return v;
    }
    return undefined;
  }
  if (!tpl || !tpl.strings) return undefined;
  const i = tpl.strings.findIndex((s) => new RegExp(`\\s${name}=$`).test(s));
  if (i >= 0) return tpl.values[i];
  return attrOf(tpl.values, name);
}

/** A fake window that keeps its listeners. */
function fakeWindow() {
  const listeners = {};
  return {
    listeners,
    addEventListener: (type, fn) => (listeners[type] ||= []).push(fn),
    removeEventListener: (type, fn) => { listeners[type] = (listeners[type] || []).filter((f) => f !== fn); },
    dispatch(type, e = {}) {
      const ev = { type, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, stopPropagation() {}, ...e };
      for (const fn of [...(listeners[type] || [])]) fn(ev);
      return ev;
    },
  };
}

async function uiModule({ rtl = false, store = {} } = {}) {
  const H = hookHarness();
  const window = fakeWindow();
  const body = { classList: { add() {}, remove() {} } };
  const document = { body, activeElement: body };
  const i18n = { t: (s) => s, isRtl: () => rtl.value ?? rtl };
  const U = await standalone('ui.js', {
    'lib.js': H.lib, 'icons.js': { Icon() {} },
    'store.js': { state: { menu: null }, set() {}, openMenu() {}, closeMenu() {}, useStore: () => ({}), ...store },
    'util.js': { cls: (...a) => a.filter(Boolean).join(' '), keyLabel: (k) => k },
    'i18n.js': i18n,
  }, { window, document, innerWidth: 1000, innerHeight: 800, setTimeout, clearTimeout });
  return { U, H, window, document };
}

/** The root menu and the submenu open in it, rendered by hand like Preact would. */
async function menu(items, opts) {
  const done = [];
  const { U, H, window, document } = await uiModule({ ...opts, store: { useStore: () => ({ menu: { x: 0, y: 0, items } }) } });
  const MenuList = H.render(H.instance(), U.MenuHost, {}).values[0];
  const root = H.instance();
  let child = null;
  let out = null;
  let subOut = null;
  const focusable = (tpl) => {
    // Items get elements that take the focus.
    for (const [i, item] of (tpl.values.find(Array.isArray) || []).entries()) {
      const ref = item && attrOf(item, 'ref');
      if (typeof ref === 'function') ref({ i, focus() { document.activeElement = this; } });
    }
  };
  const flush = () => {
    for (let pass = 0; pass < 3; pass++) {
      out = H.render(root, MenuList, { items, x: 0, y: 0, onDone: () => done.push('done') }, focusable);
      const sub = out.values.find(Array.isArray).map((it) => it && attrOf(it, 'items') && it.values.find((v) => v?.values?.[0]?.name === 'SubMenu')).find(Boolean);
      if (!sub) {
        if (child) H.unmount(child);
        child = null;
        subOut = null;
        continue;
      }
      const props = Object.fromEntries(['items', 'onDone', 'depth', 'keyboard', 'onKeyboard', 'onBack', 'labelledBy', 'nav'].map((k) => [k, attrOf(sub, k)]));
      child ||= H.instance();
      subOut = H.render(child, MenuList, { ...props, x: 0, y: 0 }, focusable);
    }
  };
  flush();
  const itemsOf = (tpl) => tpl.values.find(Array.isArray);
  const active = (tpl) => itemsOf(tpl).findIndex((it) => it && /\bactive\b/.test(attrOf(it, 'class') || ''));
  const key = (k) => { window.dispatch('keydown', { key: k }); flush(); };
  const hover = (i) => { attrOf(itemsOf(out)[i], 'onMouseEnter')(); flush(); };
  return { done, key, hover, document, window, root: () => out, sub: () => subOut, active, itemsOf };
}

const fileMenu = (runs) => [
  { label: 'New', run: () => runs.push('new') },
  { label: 'Recent', disabled: true, children: [{ label: 'Old', run: () => runs.push('old') }] },
  '-',
  { label: 'Export', children: [{ label: 'Folder', run: () => runs.push('folder') }, { label: 'PDF', disabled: true, run: () => runs.push('pdf') }, { label: 'Single', run: () => runs.push('single') }] },
  { label: 'Close', disabled: true, run: () => runs.push('close') },
];

test('menus and their submenus work from the keyboard alone', async () => {
  const runs = [];
  const m = await menu(fileMenu(runs));
  assert.match(m.root().strings.join(''), /role="menu"/);
  m.key('ArrowUp');
  assert.equal(m.active(m.root()), 3, 'Up from nothing goes to the last enabled item');
  m.key('ArrowDown');
  m.key('ArrowDown');
  assert.equal(m.active(m.root()), 3, 'disabled items and separators are skipped');
  const exportItem = () => m.itemsOf(m.root())[3];
  assert.equal(attrOf(exportItem(), 'aria-haspopup'), 'menu');
  assert.equal(attrOf(exportItem(), 'aria-expanded'), false);

  m.key('ArrowRight');
  assert.ok(m.sub(), 'Right opens the submenu');
  assert.equal(attrOf(exportItem(), 'aria-expanded'), true);
  assert.equal(attrOf(m.sub(), 'aria-labelledby'), attrOf(exportItem(), 'aria-labelledby'), 'the submenu is named by its item');
  assert.equal(m.active(m.sub()), 0, 'and goes to its first item');
  assert.equal(m.document.activeElement.i, 0, 'which has the focus');
  m.key('ArrowDown');
  assert.equal(m.active(m.sub()), 2, 'Down moves inside the submenu, past its disabled item');
  assert.equal(m.active(m.root()), 3, 'the parent item stays active');
  assert.equal(m.window.listeners.keydown.length, 1, 'one menu level has the keyboard');

  m.key('ArrowLeft');
  assert.equal(m.sub(), null, 'Left closes the submenu');
  assert.equal(m.active(m.root()), 3);
  m.key('Enter');
  assert.equal(m.active(m.sub()), 0, 'Enter opens it too');
  m.key('Escape');
  assert.equal(m.sub(), null, 'Escape goes back to the parent item');
  assert.deepEqual(m.done, [], 'without closing the menu');
  m.key(' ');
  m.key('ArrowDown');
  m.key('ArrowDown');
  assert.equal(m.active(m.sub()), 0, 'Down wraps around');
  m.key('ArrowDown');
  m.key(' ');
  assert.deepEqual(runs, ['single']);
  assert.deepEqual(m.done, ['done']);
});

test('disabled menu items never become active or run', async () => {
  const runs = [];
  const m = await menu(fileMenu(runs));
  assert.equal(attrOf(m.itemsOf(m.root())[4], 'aria-disabled'), 'true');
  m.hover(4);
  assert.equal(m.active(m.root()), -1, 'hovering a disabled item leaves it inactive');
  m.key('Enter');
  m.key(' ');
  m.hover(1);
  assert.equal(m.sub(), null, 'a disabled item opens no submenu');
  m.key('ArrowRight');
  assert.equal(m.sub(), null);
  assert.deepEqual(runs, []);
  assert.deepEqual(m.done, []);
  // Hovering a submenu opens it; moving down closes it again.
  m.hover(3);
  assert.ok(m.sub());
  m.key('ArrowDown');
  assert.equal(m.sub(), null, 'arrow keys close a submenu opened by hovering');
});

test('in right-to-left languages Left opens a submenu and Right goes back', async () => {
  const m = await menu(fileMenu([]), { rtl: true });
  m.key('ArrowUp');
  m.key('ArrowRight');
  assert.equal(m.sub(), null);
  m.key('ArrowLeft');
  assert.equal(m.active(m.sub()), 0);
  m.key('ArrowRight');
  assert.equal(m.sub(), null);
});

/** A Modal rendered by hand over fake elements. */
async function modal({ storeState = { menu: null }, focusCaller = false } = {}) {
  const { U, H, window, document } = await uiModule({ store: { state: storeState } });
  const el = (name, more = {}) => ({
    name, tag: 'button', tabIndex: 0, disabled: false, getClientRects: () => [{}], focus() { document.activeElement = this; },
    matches(sel) { return sel.split(',').some((s) => s.trim() === this.tag); },
    ...more,
  });
  const editor = { previousElementSibling: null, inert: false };
  const before = el('canvas', { isConnected: true });
  document.activeElement = before;
  // A section's toggle comes before the first field (as in the Theme Builder).
  const section = el('section');
  const field = el('field', { tag: 'input' });
  const other = el('other', { tag: 'input' });
  const cancel = el('cancel');
  const close = el('close');
  const inside = [close, section, field, other, cancel];
  let popover = null;
  const dialog = {
    name: 'dialog', parentElement: { previousElementSibling: editor },
    contains: (x) => x === dialog || inside.includes(x),
    querySelectorAll: () => inside,
    querySelector: (sel) => sel === '.popover' ? popover
      : sel.includes('modal-body') ? { querySelectorAll: () => [section, field, other] }
      : sel.includes('modal-foot') ? { querySelectorAll: () => [cancel] } : null,
    focus() { document.activeElement = this; },
  };
  const closes = [];
  const inst = H.instance();
  const out = H.render(inst, U.Modal, { title: 'Settings', onClose: () => closes.push(1), footer: 'x' }, (o) => {
    attrOf(o, 'ref').current = dialog;
  });
  if (focusCaller) other.focus();
  await new Promise((r) => setTimeout(r, 5));
  const key = (k, shiftKey = false) => window.dispatch('keydown', { key: k, shiftKey });
  return { out, document, editor, before, field, other, cancel, close, closes, key, setPopover: (p) => (popover = p), unmount: () => H.unmount(inst) };
}

test('a dialog takes the focus, keeps Tab inside and gives the focus back', async () => {
  const d = await modal();
  assert.equal(attrOf(d.out, 'aria-labelledby'), attrOf(d.out, 'id'), 'the title names the dialog');
  assert.equal(d.document.activeElement, d.field, 'the first field of the body gets the focus, not the button before it');
  assert.equal(d.editor.inert, true, 'the page behind is out of reach');
  d.document.activeElement = d.cancel;
  assert.ok(d.key('Tab').defaultPrevented);
  assert.equal(d.document.activeElement, d.close, 'Tab wraps from the last to the first');
  assert.ok(d.key('Tab', true).defaultPrevented);
  assert.equal(d.document.activeElement, d.cancel, 'Shift+Tab wraps from the first to the last');
  d.document.activeElement = d.field;
  assert.ok(!d.key('Tab').defaultPrevented, 'inside, Tab moves as usual');
  d.unmount();
  assert.equal(d.editor.inert, false);
  assert.equal(d.document.activeElement, d.before, 'the focus goes back where it was');
});

test('a dialog leaves the focus a caller gives and Escape to an open menu or popover', async () => {
  const storeState = { menu: null };
  const d = await modal({ storeState, focusCaller: true });
  assert.equal(d.document.activeElement, d.other, 'the field the caller focused keeps the focus');
  storeState.menu = { items: [] };
  d.key('Escape');
  d.key('Tab');
  storeState.menu = null;
  d.setPopover({});
  d.key('Escape');
  assert.deepEqual(d.closes, [], 'the menu or popover closes first');
  d.setPopover(null);
  d.key('Escape');
  assert.deepEqual(d.closes, [1]);
});

test('a row label names its controls and clicking it focuses the field', async () => {
  const H = hookHarness();
  const C = await standalone('controls.js', {
    'lib.js': H.lib, 'icons.js': { Icon() {} }, 'store.js': { state: {}, registerFlush: () => () => {} },
    'util.js': { clamp: (v) => v, cls: (...a) => a.filter(Boolean).join(' '), trackPointer() {} },
    'i18n.js': { t: (s) => s, N_: (s) => s },
  }, { window: {}, document: {} });
  let focused = null;
  const out = H.render(H.instance(), C.Row, { label: 'Size', children: 'field' }, (o) => {
    attrOf(o, 'ref').current = { querySelector: (sel) => (/input/.test(sel) && !/button/.test(sel) ? { focus: () => (focused = sel) } : null) };
  });
  assert.equal(attrOf(out, 'role'), 'group');
  assert.equal(attrOf(out, 'aria-labelledby'), attrOf(out, 'id'));
  attrOf(out, 'onClick')();
  assert.ok(focused, 'the label focuses a field');
  assert.ok(/:not\(\[type=checkbox\]\)/.test(focused), 'not a checkbox, which a click would toggle');
  const plain = H.render(H.instance(), C.Row, { children: 'toggle' });
  assert.equal(attrOf(plain, 'role'), undefined, 'a row without a label is no group');
});

test('drags end on pointer cancel and when their component goes away', async () => {
  const window = fakeWindow();
  const U = await standalone('util.js', { 'i18n.js': { t: (s) => s, language: () => 'en' } }, { window, navigator: { platform: 'Linux' } });
  const ends = [];
  const moves = [];
  U.trackPointer((ev) => moves.push(ev.clientY), (ev) => ends.push(ev && ev.clientY));
  window.dispatch('pointermove', { clientY: 5 });
  window.dispatch('pointerup', { clientY: 9 });
  window.dispatch('pointermove', { clientY: 7 });
  assert.deepEqual([moves, ends], [[5], [9]], 'pointerup ends with its event');
  U.trackPointer(() => {}, (ev) => ends.push(ev));
  window.dispatch('pointercancel', { clientY: 0 });
  assert.deepEqual(ends, [9, null], 'a cancelled drag ends with nothing to drop');
  const stop = U.trackPointer(() => {}, (ev) => ends.push(ev));
  stop();
  stop();
  window.dispatch('pointerup', { clientY: 3 });
  assert.deepEqual(ends, [9, null, null], 'stopping ends it once');
  assert.ok(['pointermove', 'pointerup', 'pointercancel'].every((t) => !window.listeners[t].length), 'no listener is left');
});

test('presenter pauses stay paused and notes keep their scroll during builds', async () => {
  const els = {};
  const $ = (id) => (els[id] ||= {
    id, textContent: '', scrollTop: 0, style: { setProperty() {} }, title: '',
    classList: { set: new Set(), toggle(c, on) { if (on) this.set.add(c); else this.set.delete(c); }, contains(c) { return this.set.has(c); } },
    setAttribute() {}, addEventListener() {}, click() {},
    contentWindow: null, contentDocument: null,
  });
  let onState = null;
  const data = { width: 1280, height: 720, title: 'Talk', language: 'en', slides: [{ id: 's1', notes: 'Long notes' }, { id: 's2', notes: 'More notes' }] };
  const tauri = {
    core: { invoke: async (name) => (name === 'present_data' ? data : undefined) },
    event: { listen: (name, fn) => { if (name === 'cz-present-state') onState = fn; }, emit() {} },
  };
  const context = vm.createContext({
    window: { addEventListener() {}, __TAURI__: tauri }, location: { search: '?doc=d1' }, URLSearchParams,
    document: { getElementById: $, addEventListener() {}, querySelectorAll: () => [], documentElement: { style: { setProperty() {} } }, title: '' },
    setInterval() {}, setTimeout: () => 0, localStorage: { getItem: () => null, setItem() {} }, console,
  });
  new vm.Script(fs.readFileSync(path.join(root, 'presenter.js'), 'utf8'), { filename: 'presenter.js' }).runInContext(context);
  await tick();
  const state = (slideId, extra = {}) => onState({ payload: { doc: 'd1', slideId, index: 0, total: 2, state: {}, ...extra } });
  // Reset redraws the timer, whose button says what it would do.
  state('s1', { started: true });
  $('timer-reset').onclick();
  assert.equal($('timer-toggle').textContent, 'Pause', 'the show starts the timer');
  $('timer-toggle').onclick();
  assert.equal($('timer-toggle').textContent, 'Resume');
  $('notes').scrollTop = 120;
  state('s1');
  state('s1', { paused: true });
  $('timer-reset').onclick();
  assert.equal($('timer-toggle').textContent, 'Resume', 'builds, blackouts and syncs leave the timer paused');
  assert.equal($('notes').scrollTop, 120, 'and the notes where they were scrolled');
  assert.equal($('notes').textContent, 'Long notes');
  state('s2');
  assert.equal($('notes').textContent, 'More notes');
  assert.equal($('notes').scrollTop, 0, 'another slide starts at the top of its notes');
});
