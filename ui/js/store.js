// Application state and document actions.
//
// The deck itself lives in Rust. The UI sends operations (`apply`) and
// ingests the returned view: the full deck plus the HTML of slides that
// changed. Everything here is plain JS; components subscribe with
// `useStore()` and re-render on change.

import { useEffect, useReducer } from './lib.js';
import { invoke, dialog, events, currentWindow, getAllWindows, FILE_FILTER } from './api.js';
import { t, setLanguage } from './i18n.js';
import { clone, fileName, scopeCss } from './util.js';

export const state = {
  ready: false,
  info: null, // app info: themes, layouts, shapes, settings, recent...
  doc: null, // current document view
  slideId: null, // slide shown in the canvas
  selectedSlides: [], // navigator selection
  selection: [], // selected element ids on the current slide
  editing: null, // element id whose text is being edited
  editingTable: null, // table element id whose cells are being edited
  zoom: 'fit',
  tab: 'format', // inspector tab
  focus: 'canvas', // 'canvas' | 'slides' — where Delete / Copy act
  modal: null,
  menu: null,
  toasts: [],
  fonts: null,
  presenting: false,
};

const listeners = new Set();
let scheduled = false;

export function subscribe(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function set(patch) {
  Object.assign(state, typeof patch === 'function' ? patch(state) : patch);
  if (scheduled) return;
  scheduled = true;
  queueMicrotask(() => {
    scheduled = false;
    for (const fn of [...listeners]) fn(state);
  });
}

/** Re-renders the calling component whenever the state changes. */
export function useStore() {
  const [, force] = useReducer((x) => x + 1, 0);
  useEffect(() => subscribe(force), []);
  return state;
}

// ---------------------------------------------------------------------------
// Derived data
// ---------------------------------------------------------------------------

export const deck = () => state.doc?.deck;
export const currentSlide = () => state.doc?.deck.slides.find((s) => s.id === state.slideId) || null;
export const currentIndex = () => state.doc?.deck.slides.findIndex((s) => s.id === state.slideId) ?? -1;
export const selectedElements = () => {
  const s = currentSlide();
  return s ? s.elements.filter((e) => state.selection.includes(e.id)) : [];
};
export const settings = () => state.info?.settings || {};
export const themeInfo = (id) => {
  const custom = state.doc?.deck?.customTheme;
  if (custom && custom.id === id) return { id, name: custom.name, dark: custom.dark, custom: true };
  return state.info?.themes.find((theme) => theme.id === id) || state.info?.userThemes?.find((theme) => theme.id === id) || state.info?.themes[0];
};

/** Rich text accessors: text boxes keep it in `content`, shapes in `content.text`. */
export const richOf = (el) => (el.content.kind === 'shape' ? el.content.text : el.content);
export const richPatch = (el, patch) => (el.content.kind === 'shape' ? { content: { text: patch } } : { content: patch });

// ---------------------------------------------------------------------------
// Toasts, menus, modals
// ---------------------------------------------------------------------------

let toastId = 0;
export function toast(message, kind = 'info', opts = {}) {
  const id = ++toastId;
  // Errors mostly come from Rust in English: translate the known ones.
  const item = { id, message: kind === 'error' ? t(String(message)) : message, kind, action: opts.action };
  set({ toasts: [...state.toasts, item] });
  setTimeout(() => set({ toasts: state.toasts.filter((x) => x.id !== id) }), opts.duration || (kind === 'error' ? 6000 : 3200));
}

export const openMenu = (x, y, items) => set({ menu: { x, y, items } });
export const closeMenu = () => state.menu && set({ menu: null });
export const openModal = (type, props = {}) => set({ modal: { type, props }, menu: null });
export const closeModal = () => set({ modal: null });

/** Shows a modal and resolves with the value passed to `resolve`. */
export function ask(type, props = {}) {
  return new Promise((resolve) => {
    openModal(type, {
      ...props,
      resolve: (v) => {
        closeModal();
        resolve(v);
      },
    });
  });
}

// ---------------------------------------------------------------------------
// Pending edits (text being typed, notes...) flushed before saving etc.
// ---------------------------------------------------------------------------

const flushers = new Set();
export function registerFlush(fn) {
  flushers.add(fn);
  return () => flushers.delete(fn);
}

/**
 * Set by textedit.js: after an undo or redo, the text being edited shows what
 * the document now holds. The canvas leaves an edited element as typed, so
 * it would go on showing what the undo took away.
 */
let historySync = null;
export const registerHistorySync = (fn) => {
  historySync = fn;
};

/**
 * Edits on their way to the operation queue (a diagram is drawn before its
 * source goes in): flushes wait for them, even once their field is gone.
 */
const inflight = new Set();
export function trackEdit(promise) {
  inflight.add(promise);
  promise.finally(() => inflight.delete(promise)).catch(() => {});
  return promise;
}

/**
 * Commits pending edits; false if one could not be committed. Save, Close,
 * Play and updates stop then: what is typed stays in its field, for another
 * try (or Escape) once the error is read.
 */
export async function flushEdits() {
  const docId = state.doc?.docId;
  const failedBefore = failures;
  try {
    for (const fn of [...flushers]) {
      if ((await fn()) === false || state.doc?.docId !== docId) return false;
    }
    while (inflight.size) await Promise.allSettled([...inflight]);
    await chain;
    return state.doc?.docId === docId && failures === failedBefore;
  } catch (e) {
    toast(String(e), 'error');
    return false;
  }
}

/**
 * Commits pending edits in every editor window: an update restarts them all,
 * reopening what Rust holds. False if a window could not, or did not answer.
 */
export async function flushAllWindows(timeout = 10000) {
  if (!(await flushEdits())) return false;
  const me = currentWindow().label;
  const others = (await getAllWindows()).map((w) => w.label).filter((l) => l.startsWith('editor-') && l !== me);
  if (!others.length) return true;
  const token = `${me}:${Date.now()}`;
  const waiting = new Set(others);
  let ok = true;
  let off = null;
  const done = new Promise((resolve) => {
    const timer = setTimeout(() => resolve(false), timeout);
    events
      .listen('cz-flush-done', (e) => {
        if (e.payload?.token !== token) return;
        ok &&= !!e.payload.ok;
        waiting.delete(e.payload.label);
        if (!waiting.size) {
          clearTimeout(timer);
          resolve(ok);
        }
      })
      .then((f) => {
        off = f;
        events.emit('cz-flush-all', { token, from: me });
      });
  });
  const result = await done;
  off?.();
  return result;
}

// ---------------------------------------------------------------------------
// Theme styles for the editor canvas
// ---------------------------------------------------------------------------

/**
 * The presentation's own CSS, in the editor: kept to the slides, so that no
 * rule (from a presentation made by someone else, say) can restyle or hide
 * the editor itself. Without @scope (older WebKitGTK) it applies as is.
 */
function editorCss(css) {
  return 'CSSScopeRule' in window ? scopeCss(css, ':has(> .cz-editor-reveal)') : css;
}

function applyDeckStyles(view) {
  const link = document.getElementById('cz-theme');
  // Custom themes have no stylesheet: their CSS is part of deckCss.
  if (link && !view.themeHref) link.removeAttribute('href');
  else if (link && link.getAttribute('href') !== view.themeHref) link.setAttribute('href', view.themeHref);
  const style = document.getElementById('cz-deck-css');
  const css = view.deckCss + (view.customCss ? '\n' + editorCss(view.customCss) : '');
  if (style && style.textContent !== css) style.textContent = css;
  const code = document.getElementById('cz-code-theme');
  const codeHref = `vendor/reveal/plugin/highlight/${view.deck.settings.codeTheme === 'zenburn' ? 'zenburn' : 'monokai'}.css`;
  if (code && code.getAttribute('href') !== codeHref) code.setAttribute('href', codeHref);
}

/** The document's name for display: the default names given by Rust are shown in the interface language. */
export function displayName(d = state.doc) {
  if (!d) return '';
  if (!d.path && d.fileName === 'Untitled Presentation') return t('Untitled Presentation');
  if (!d.path && d.fileName === 'Untitled') return t('Untitled');
  return d.fileName;
}

/** A presentation's title as shown: Rust names new ones in English. */
export function shownTitle(title) {
  return title === 'Untitled Presentation' ? t('Untitled Presentation') : title;
}

/** The deck title, with the default title of new presentations in the interface language. */
export function deckTitle() {
  return shownTitle(state.doc?.deck.title || '');
}

export function updateWindowTitle() {
  const d = state.doc;
  const title = d ? `${displayName(d)}${d.dirty ? ' •' : ''} — Cadenza` : 'Cadenza';
  if (document.title !== title) {
    document.title = title;
    currentWindow().setTitle(title).catch(() => {});
  }
}

// ---------------------------------------------------------------------------
// Ingesting views from Rust
// ---------------------------------------------------------------------------

function ingest(view, { navigate = true } = {}) {
  const prev = state.doc;
  const sameDoc = prev?.docId === view.docId;
  if (sameDoc && view.revision != null && prev.revision != null && view.revision <= prev.revision) return false;
  const missedView = sameDoc && !view.full && view.revision > prev.revision + 1;
  const slides = view.full || !prev ? {} : { ...prev.slides };
  Object.assign(slides, view.slides);
  const ids = view.deck.slides.map((s) => s.id);
  for (const k of Object.keys(slides)) if (!ids.includes(k)) delete slides[k];
  const doc = { ...view, slides, version: (prev?.version || 0) + 1 };

  let slideId = state.slideId;
  const outcome = view.outcome;
  if (navigate && outcome?.slide && ids.includes(outcome.slide)) slideId = outcome.slide;
  if (!ids.includes(slideId)) slideId = ids[0] || null;
  const slide = view.deck.slides.find((s) => s.id === slideId);
  const elIds = slide ? slide.elements.map((e) => e.id) : [];

  let selection = slideId === state.slideId ? state.selection.filter((id) => elIds.includes(id)) : [];
  const createdEls = (outcome?.created || []).filter((id) => elIds.includes(id));
  if (createdEls.length) selection = createdEls;

  let selectedSlides = state.selectedSlides.filter((id) => ids.includes(id));
  const createdSlides = (outcome?.created || []).filter((id) => ids.includes(id));
  if (createdSlides.length) selectedSlides = createdSlides;
  if (!selectedSlides.includes(slideId)) selectedSlides = slideId ? [slideId] : [];

  const editing = state.editing && elIds.includes(state.editing) ? state.editing : null;
  set({ doc, slideId, selection, selectedSlides, editing });
  applyDeckStyles(view);
  updateWindowTitle();
  // Incremental renderings depend on the previous view having arrived.
  if (missedView) refreshDocument();
  return true;
}

/** Ingests a view returned by a command other than `doc_apply`. */
export const ingestView = (view, opts) => ingest(view, opts);

/** Shows a document view as a freshly opened document. */
export const openView = (view) => loadView(view);

function loadView(view) {
  set({
    doc: null,
    slideId: view.deck.slides[0]?.id || null,
    selection: [],
    selectedSlides: [],
    editing: null,
    editingTable: null,
    zoom: 'fit',
    tab: 'format',
  });
  ingest(view);
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

let chain = Promise.resolve();
/** Operations that failed (and showed their error), so a flush can tell. */
let failures = 0;

/**
 * Applies an operation in Rust. Operations are serialized; `coalesce` merges
 * consecutive operations with the same key into one undo step, and
 * `track: false` makes none (derived data, such as measured heights).
 * `op` may be a function, called when the operation's turn comes, so it
 * reads the document as the operations before it left it; it returns null
 * to skip. Resolves with the outcome, or null when the operation failed or
 * was skipped.
 */
export function apply(op, opts = {}) {
  const args = (o) => o && { op: o, coalesce: opts.coalesce || null, track: opts.track ?? null };
  return enqueue('doc_apply', typeof op === 'function' ? () => args(op()) : args(op), opts);
}

/** Whether a text box is exactly as tall as its text ("Fit box to text"). */
export const fitsText = (el) => el.content.kind === 'text' && !!el.content.style.autoHeight;
export const fitsMath = (el) => el.content.kind === 'math' && !!el.content.fit;

function enqueue(command, args, opts = {}) {
  const docId = opts.docId ?? state.doc?.docId;
  const run = async () => {
    if (!docId || state.doc?.docId !== docId) return null;
    try {
      const a = typeof args === 'function' ? args() : args;
      if (!a) return null;
      const view = await invoke(command, { ...a, docId });
      if (state.doc?.docId !== docId) return null;
      ingest(view, opts);
      return view.outcome;
    } catch (e) {
      failures++;
      console.error('operation failed', command, e);
      toast(String(e), 'error');
      return null;
    }
  };
  chain = chain.then(run, run);
  return chain;
}

export const pasteClip = (clip, target, offset = 0) => enqueue('clipboard_paste', { clip, target, offset });

const refreshes = new Map();
export function refreshDocument() {
  const docId = state.doc?.docId;
  if (!docId) return Promise.resolve(null);
  if (refreshes.has(docId)) return refreshes.get(docId);
  const pending = enqueue('doc_refresh', {}, { navigate: false });
  refreshes.set(docId, pending);
  pending.finally(() => refreshes.delete(docId));
  return pending;
}

export const updateElements = (ids, patch, coalesce) =>
  ids.length
    ? apply(
        {
          op: 'updateElements',
          slide: state.slideId,
          updates: ids.map((id) => ({ id, patch: typeof patch === 'function' ? patch(id) : patch })),
        },
        { coalesce },
      )
    : Promise.resolve(null);

export const updateSelected = (patch, coalesce) => updateElements(state.selection, patch, coalesce);
export const updateSlides = (ids, patch, coalesce) => apply({ op: 'updateSlides', ids, patch }, { coalesce, navigate: false });
export const updateCurrentSlide = (patch, coalesce) => updateSlides([state.slideId], patch, coalesce);
export const updateDeck = (patch, coalesce) => apply({ op: 'updateDeck', patch }, { coalesce, navigate: false });

export async function undo() {
  if (!(await flushEdits())) return;
  if (!state.doc?.canUndo) return;
  try {
    const before = state.doc.deck;
    if (ingest(await invoke('doc_undo'))) selectChanged(before);
    historySync?.();
  } catch (e) {
    toast(String(e), 'error');
  }
}

export async function redo() {
  if (!(await flushEdits())) return;
  if (!state.doc?.canRedo) return;
  try {
    const before = state.doc.deck;
    if (ingest(await invoke('doc_redo'))) selectChanged(before);
    historySync?.();
  } catch (e) {
    toast(String(e), 'error');
  }
}

/**
 * After an undo or redo, selects the elements of the slide in view that it
 * brought back or changed (a deleted element comes back selected, ready to
 * be copied or moved). Not while a text is being edited.
 */
function selectChanged(before) {
  const old = before.slides.find((s) => s.id === state.slideId);
  if (!old || state.editing) return;
  const was = new Map(old.elements.map((e) => [e.id, JSON.stringify(e)]));
  const ids = (currentSlide()?.elements || []).filter((e) => was.get(e.id) !== JSON.stringify(e)).map((e) => e.id);
  if (ids.length) set({ selection: ids });
}

export const seal = () => state.doc && invoke('doc_seal').catch(() => {});

// ---------------------------------------------------------------------------
// Navigation & selection
// ---------------------------------------------------------------------------

export function goToSlide(id, { keepSelection = false } = {}) {
  if (!id || id === state.slideId) return;
  set({
    slideId: id,
    selection: [],
    editing: null,
    editingTable: null,
    selectedSlides: keepSelection ? state.selectedSlides : [id],
  });
}

export function stepSlide(delta) {
  const slides = state.doc?.deck.slides || [];
  const i = slides.findIndex((s) => s.id === state.slideId);
  const next = slides[Math.min(slides.length - 1, Math.max(0, i + delta))];
  if (next) goToSlide(next.id);
}

export const select = (ids) => set({ selection: ids, focus: 'canvas' });

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

export async function newDocument(theme, aspect) {
  try {
    loadView(await invoke('doc_new', { theme, aspect }));
  } catch (e) {
    toast(String(e), 'error');
  }
}

export async function openPath(path) {
  if (!path) return;
  try {
    if (await invoke('window_focus_document', { path })) return;
    // Keep the current presentation: open the file in a new window. Pending
    // edits count, so commit them before deciding.
    if (!(await flushEdits())) return;
    if (state.doc && (state.doc.dirty || state.doc.path)) {
      await invoke('window_new', { path });
      return;
    }
    set({ busy: t('Opening…') });
    loadView(await invoke('doc_open', { path }));
  } catch (e) {
    await dialog.message(t(String(e)), { title: t('Could not open {name}', { name: fileName(path) }), kind: 'error' });
    await invoke('recent_remove', { path }).catch(() => {});
    refreshRecent();
  } finally {
    set({ busy: null });
  }
}

/** The presentation file filter, named in the interface language. */
const presentationFilter = () => FILE_FILTER.map((f) => ({ ...f, name: t('Cadenza Presentation') }));

export async function openDialog() {
  const path = await dialog.open({ title: t('Open Presentation'), multiple: false, directory: false, filters: presentationFilter() });
  if (path) await openPath(path);
}

export async function refreshRecent() {
  try {
    const recent = await invoke('recent_list');
    set({ info: { ...state.info, recent } });
  } catch {
    /* ignore */
  }
}

function suggestedName() {
  const name = (deckTitle() || t('Presentation')).replace(/[\\/:*?"<>|]+/g, '-').trim() || t('Presentation');
  return `${name}.cadenza`;
}

/** Saves; returns true on success. */
export async function save({ as = false } = {}) {
  if (!state.doc) return false;
  if (!(await flushEdits())) return false;
  let path = as ? null : state.doc.path;
  if (!path) {
    path = await dialog.save({ title: as ? t('Save Presentation As') : t('Save Presentation'), defaultPath: suggestedName(), filters: presentationFilter() });
    if (!path) return false;
  }
  try {
    set({ busy: t('Saving…') });
    ingest(await invoke('doc_save', { path }), { navigate: false });
    toast(t('Saved'));
    refreshRecent();
    return true;
  } catch (e) {
    await dialog.message(t(String(e)), { title: t('Could not save'), kind: 'error' });
    return false;
  } finally {
    set({ busy: null });
  }
}

/** Asks what to do with unsaved changes. Returns false to cancel. */
export async function confirmDiscard() {
  if (!(await flushEdits())) return false;
  if (!state.doc?.dirty) return true;
  const choice = await ask('unsaved', { name: displayName() });
  if (choice === 'save') return save();
  return choice === 'discard';
}

export async function closeDocument() {
  if (!(await confirmDiscard())) return;
  try {
    await invoke('doc_close');
  } catch (e) {
    toast(String(e), 'error');
    return;
  }
  set({ doc: null, slideId: null, selection: [], selectedSlides: [], editing: null, editingTable: null });
  updateWindowTitle();
  refreshRecent();
  refreshRecoverable();
}

export async function recover(sessionId) {
  try {
    loadView(await invoke('doc_recover', { sessionId }));
    toast(t('Recovered unsaved presentation — remember to save it.'));
  } catch (e) {
    toast(String(e), 'error');
  }
  refreshRecoverable();
}

export async function discardRecovery(sessionId) {
  await invoke('doc_discard_recovery', { sessionId }).catch((e) => toast(String(e), 'error'));
  refreshRecoverable();
}

/** Other windows may have recovered or discarded sessions listed here. */
export async function refreshRecoverable() {
  try {
    const recoverable = await invoke('recovery_list');
    set({ info: { ...state.info, recoverable } });
  } catch {
    /* ignore */
  }
}

export async function updateSettings(patch) {
  try {
    await adoptSettings(await invoke('settings_update', { patch }));
  } catch (e) {
    toast(String(e), 'error');
  }
}

/** Uses settings saved by this or another window (e.g. the presenter display). */
async function adoptSettings(s) {
  const languageChanged = s.language !== settings().language;
  if (languageChanged) await setLanguage(s.language);
  set({ info: { ...state.info, settings: s } });
  applyUiTheme();
  if (languageChanged) updateWindowTitle();
}

export function applyUiTheme() {
  const pref = settings().uiTheme || 'system';
  const dark = pref === 'dark' || (pref === 'system' && matchMedia('(prefers-color-scheme: dark)').matches);
  document.documentElement.dataset.ui = dark ? 'dark' : 'light';
}

// ---------------------------------------------------------------------------
// Presenting
// ---------------------------------------------------------------------------

export async function play(fromCurrent = false) {
  if (!state.doc) return;
  if (!(await flushEdits())) return;
  set({ editing: null, menu: null });
  try {
    // A diagram drawn a moment ago has its picture by then.
    await import('./diagrams.js').then((m) => m.ensureDiagrams());
    await invoke('present_start', { fromSlide: fromCurrent ? state.slideId : null, presenter: null });
  } catch (e) {
    toast(String(e), 'error');
  }
}

// ---------------------------------------------------------------------------
// Clipboard (elements and slides)
// ---------------------------------------------------------------------------

export const CLIP_MARKER = 'application/x-cadenza+json:';
let clip = null;

export function copyElements(elements) {
  return captureClip({ type: 'elements', elements: clone(elements) });
}

export function copySlides(slides) {
  return captureClip({ type: 'slides', slides: clone(slides) });
}

async function captureClip(selection) {
  clip = await invoke('clipboard_copy', { docId: state.doc?.docId, selection });
  return CLIP_MARKER + JSON.stringify(clip);
}

/** Parses clipboard text produced by Cadenza (possibly from another window). */
export function readClip(text) {
  if (text && text.startsWith(CLIP_MARKER)) {
    try {
      return JSON.parse(text.slice(CLIP_MARKER.length));
    } catch {
      return null;
    }
  }
  return text == null ? clip : null;
}

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

export async function boot() {
  const info = await invoke('app_info');
  await setLanguage(info.settings?.language || 'en');
  info.userThemes = await invoke('themes_user').catch(() => []);
  set({ info });
  applyUiTheme();
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyUiTheme);

  events.listen('cz-settings', (e) => adoptSettings(e.payload));
  // Another window is about to install an update (see flushAllWindows).
  events.listen('cz-flush-all', async (e) => {
    const label = currentWindow().label;
    if (e.payload?.from === label) return;
    const ok = await flushEdits();
    events.emit('cz-flush-done', { token: e.payload?.token, label, ok });
  });
  events.listen('cz-present-started', () => set({ presenting: true }));
  events.listen('cz-present-ended', (e) => {
    set({ presenting: false });
    const slide = e.payload?.slide;
    if (slide && state.doc?.deck.slides.some((s) => s.id === slide)) goToSlide(slide);
  });

  const st = await invoke('doc_state');
  if (st.view) loadView(st.view);
  set({ ready: true });
  if (st.pendingOpen) {
    set({ busy: t('Opening…') });
    try {
      loadView(await invoke('doc_open', { path: st.pendingOpen }));
    } catch (e) {
      await dialog.message(t(String(e)), { title: t('Could not open the presentation'), kind: 'error' });
    } finally {
      set({ busy: null });
    }
  }
  // Reopened with its unsaved changes after an update.
  if (st.pendingRecover) await recover(st.pendingRecover);

  invoke('fonts_list')
    .then((fonts) => set({ fonts }))
    .catch(() => set({ fonts: [] }));

  // Crash-recovery checkpoints. A failure is shown once, until one works again.
  let checkpointFailed = false;
  setInterval(() => {
    if (!state.doc?.dirty) return;
    invoke('doc_checkpoint').then(
      () => (checkpointFailed = false),
      (e) => {
        if (!checkpointFailed) toast(`${t('Unsaved changes could not be backed up. Save your work.')} (${t(String(e))})`, 'error');
        checkpointFailed = true;
      },
    );
  }, 15000);
}
