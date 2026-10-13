// Application state and document actions.
//
// The deck itself lives in Rust. The UI sends operations (`apply`) and
// ingests the returned view: the full deck plus the HTML of slides that
// changed. Everything here is plain JS; components subscribe with
// `useStore()` and re-render on change.

import { useEffect, useReducer } from './lib.js';
import { invoke, dialog, events, currentWindow, FILE_FILTER } from './api.js';
import { t, setLanguage } from './i18n.js';
import { clone, fileName } from './util.js';

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
  ai: null, // running assistant request: { phase, slides, title, count }
  aiChat: 0, // bumps when an assistant conversation changes
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
export async function flushEdits() {
  for (const fn of [...flushers]) await fn();
  await chain;
}

// ---------------------------------------------------------------------------
// Theme styles for the editor canvas
// ---------------------------------------------------------------------------

function applyDeckStyles(view) {
  const link = document.getElementById('cz-theme');
  // Custom themes have no stylesheet: their CSS is part of deckCss.
  if (link && !view.themeHref) link.removeAttribute('href');
  else if (link && link.getAttribute('href') !== view.themeHref) link.setAttribute('href', view.themeHref);
  const style = document.getElementById('cz-deck-css');
  if (style && style.textContent !== view.deckCss) style.textContent = view.deckCss;
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

/** The deck title, with the default title of new presentations in the interface language. */
export function deckTitle() {
  const title = state.doc?.deck.title || '';
  return title === 'Untitled Presentation' ? t('Untitled Presentation') : title;
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
}

/** Ingests a view returned by a command other than `doc_apply`. */
export const ingestView = (view, opts) => ingest(view, opts);

/** Shows a freshly created document (e.g. one made by the assistant). */
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

/**
 * Applies an operation in Rust. Operations are serialized; `coalesce` merges
 * consecutive operations with the same key into one undo step.
 * Resolves with the outcome, or null when the operation failed.
 */
export function apply(op, opts = {}) {
  const run = async () => {
    if (!state.doc) return null;
    try {
      const view = await invoke('doc_apply', { op, coalesce: opts.coalesce || null });
      ingest(view, opts);
      return view.outcome;
    } catch (e) {
      console.error('operation failed', op, e);
      toast(String(e), 'error');
      return null;
    }
  };
  chain = chain.then(run, run);
  return chain;
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
  await flushEdits();
  if (!state.doc?.canUndo) return;
  try {
    ingest(await invoke('doc_undo'));
  } catch (e) {
    toast(String(e), 'error');
  }
}

export async function redo() {
  await flushEdits();
  if (!state.doc?.canRedo) return;
  try {
    ingest(await invoke('doc_redo'));
  } catch (e) {
    toast(String(e), 'error');
  }
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
    // Keep the current presentation: open the file in a new window.
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
  await flushEdits();
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
  if (!state.doc?.dirty) return true;
  await flushEdits();
  if (!state.doc?.dirty) return true;
  const choice = await ask('unsaved', { name: displayName() });
  if (choice === 'save') return save();
  return choice === 'discard';
}

export async function closeDocument() {
  if (!(await confirmDiscard())) return;
  await invoke('doc_close').catch(() => {});
  set({ doc: null, slideId: null, selection: [], selectedSlides: [], editing: null });
  updateWindowTitle();
  refreshRecent();
}

export async function recover(sessionId) {
  try {
    loadView(await invoke('doc_recover', { sessionId }));
    set({ info: { ...state.info, recoverable: state.info.recoverable.filter((r) => r.sessionId !== sessionId) } });
    toast(t('Recovered unsaved presentation — remember to save it.'));
  } catch (e) {
    toast(String(e), 'error');
  }
}

export async function discardRecovery(sessionId) {
  await invoke('doc_discard_recovery', { sessionId }).catch(() => {});
  set({ info: { ...state.info, recoverable: state.info.recoverable.filter((r) => r.sessionId !== sessionId) } });
}

export async function updateSettings(patch) {
  try {
    const s = await invoke('settings_update', { patch });
    if (patch.language) await setLanguage(s.language);
    set({ info: { ...state.info, settings: s } });
    applyUiTheme();
    if (patch.language) updateWindowTitle();
  } catch (e) {
    toast(String(e), 'error');
  }
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
  await flushEdits();
  set({ editing: null, menu: null });
  try {
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
  clip = { type: 'elements', elements: clone(elements) };
  return CLIP_MARKER + JSON.stringify(clip);
}

export function copySlides(slides) {
  clip = { type: 'slides', slides: clone(slides) };
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

  invoke('fonts_list')
    .then((fonts) => set({ fonts }))
    .catch(() => set({ fonts: [] }));

  // Crash-recovery checkpoints.
  setInterval(() => {
    if (state.doc?.dirty) invoke('doc_checkpoint').catch(() => {});
  }, 15000);
}
