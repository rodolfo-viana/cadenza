// Inline rich-text editing on the canvas (text boxes, shapes, table cells).
//
// The rendered element itself becomes `contenteditable`; changes are
// committed to Rust (debounced, one undo step per editing session) and the
// SlideView leaves the edited element's content alone while it is active.

import { state, set, apply, registerFlush, seal, richOf } from './store.js';
import { parseColor, toCss } from './controls.js';

let session = null;
let editStart = 0;

export const isEditing = () => !!session;
export const editingNode = () => session?.node || null;
export const editingKind = () => session?.kind || null;

function placeCaret(node, point, selectAll) {
  const sel = window.getSelection();
  sel.removeAllRanges();
  if (point && document.caretRangeFromPoint && !selectAll) {
    const r = document.caretRangeFromPoint(point.x, point.y);
    if (r && node.contains(r.startContainer)) {
      sel.addRange(r);
      return;
    }
  }
  const r = document.createRange();
  r.selectNodeContents(node);
  if (!selectAll) r.collapse(false);
  sel.addRange(r);
}

/** Starts editing the text of a text box or shape. */
export async function beginTextEdit(view, el, point = null, { selectAll = false } = {}) {
  const start = ++editStart;
  const docId = state.doc?.docId;
  const slide = state.slideId;
  if (session?.id === el.id) return;
  if (session && !(await endTextEdit())) return;
  if (tableSession && !(await endTableEdit())) return;
  if (start !== editStart || state.doc?.docId !== docId || state.slideId !== slide) return;
  const wrapper = view.node(el.id);
  const node = wrapper?.querySelector('.cz-text');
  if (!node) return;
  view.editingId = el.id;
  const wasEmpty = node.classList.contains('cz-empty');
  if (wasEmpty) {
    node.innerHTML = '<p><br></p>';
    node.classList.remove('cz-empty');
  } else if (node.querySelector('.cz-math-inline, .cz-math-display')) {
    // Edit the LaTeX source ($…$), not the typeset formula.
    node.innerHTML = richOf(el).html;
  }
  node.setAttribute('contenteditable', 'true');
  node.setAttribute('spellcheck', 'true');
  try {
    document.execCommand('defaultParagraphSeparator', false, 'p');
    document.execCommand('styleWithCSS', false, true);
  } catch {
    /* not supported */
  }
  node.focus({ preventScroll: true });
  placeCaret(node, point, selectAll && !wasEmpty);
  session = {
    docId,
    id: el.id,
    kind: el.content.kind,
    slide: state.slideId,
    view,
    node,
    lastHtml: wasEmpty ? '' : node.innerHTML,
    timer: null,
  };
  node.addEventListener('input', onInput);
  node.addEventListener('paste', onPaste);
  node.addEventListener('keydown', onKeyDown);
  const s = session;
  s.unregister = registerFlush(() => commit(s));
  set({ editing: el.id, selection: [el.id], focus: 'canvas' });
}

function onInput() {
  if (!session) return;
  clearTimeout(session.timer);
  const s = session;
  s.timer = setTimeout(() => commit(s), 500);
  window.dispatchEvent(new CustomEvent('cz-text-input'));
}

function onPaste(e) {
  e.preventDefault();
  e.stopPropagation();
  const text = e.clipboardData.getData('text/plain');
  if (text) document.execCommand('insertText', false, text);
}

function onKeyDown(e) {
  if (e.key === 'Escape') {
    e.preventDefault();
    e.stopPropagation();
    endTextEdit();
  } else if (e.key === 'Tab') {
    e.preventDefault();
    const inList = window.getSelection()?.anchorNode?.parentElement?.closest('li');
    if (inList) document.execCommand(e.shiftKey ? 'outdent' : 'indent');
  }
}

function commit(s) {
  if (!s || s.docId !== state.doc?.docId) return Promise.resolve(true);
  clearTimeout(s.timer);
  // A flush must also wait for edits already being sent, including any text
  // typed while that request was pending.
  if (!s.committing) s.committing = commitText(s).finally(() => { s.committing = null; });
  return s.committing;
}

/**
 * Whether an edited element still exists. Once it is deleted, undone or
 * replaced, its pending edit has nowhere to go and must not block saving or
 * closing forever.
 */
const elementExists = (s) => !!state.doc?.deck.slides.find((x) => x.id === s.slide)?.elements.some((e) => e.id === s.id);

async function commitText(s) {
  for (;;) {
    const html = cleanHtml(s.node.innerHTML);
    if (html === s.lastHtml || !elementExists(s)) return true;
    const patch = s.kind === 'shape' ? { content: { text: { html } } } : { content: { html } };
    const out = await apply({ op: 'updateElements', slide: s.slide, updates: [{ id: s.id, patch }] }, { docId: s.docId, coalesce: 'text:' + s.id, navigate: false });
    if (!out) return false;
    s.lastHtml = html;
  }
}

/** Drops the empty markup editors leave behind. */
function cleanHtml(html) {
  const trimmed = html.trim();
  return trimmed === '<p><br></p>' || trimmed === '<br>' || trimmed === '<div><br></div>' ? '' : html;
}

/** Finishes editing, committing any pending change. */
export async function endTextEdit() {
  const s = session;
  if (!s) return true;
  if (!(await commit(s))) return false;
  if (session !== s) return true;
  session = null;
  s.node.removeEventListener('input', onInput);
  s.node.removeEventListener('paste', onPaste);
  s.node.removeEventListener('keydown', onKeyDown);
  s.unregister?.();
  s.node.removeAttribute('contenteditable');
  s.node.removeAttribute('spellcheck');
  s.view.editingId = null;
  const sel = window.getSelection();
  if (sel && s.node.contains(sel.anchorNode)) sel.removeAllRanges();
  seal();
  if (state.editing === s.id) set({ editing: null });
  window.dispatchEvent(new CustomEvent('cz-edit-ended'));
  return true;
}

/** Runs a formatting command on the current text selection. */
export function exec(cmd, value = null) {
  if (!session) return false;
  session.node.focus({ preventScroll: true });
  let v = value;
  if (typeof v === 'string' && v.startsWith('var(')) {
    const c = parseColor(v);
    v = c ? toCss(c) : v;
  }
  const ok = document.execCommand(cmd, false, v);
  onInput();
  return ok;
}

export function queryState(cmd) {
  try {
    return session ? document.queryCommandState(cmd) : false;
  } catch {
    return false;
  }
}

/**
 * Inserts inline LaTeX at the caret: wraps the selected text in `$…$`, or
 * inserts a `$x$` template with `x` selected.
 */
export function insertInlineMath(display = false) {
  if (!session) return false;
  session.node.focus({ preventScroll: true });
  const sel = window.getSelection();
  const text = sel && !sel.isCollapsed ? sel.toString() : '';
  const d = display ? '$$' : '$';
  const body = text || 'x';
  document.execCommand('insertText', false, d + body + d);
  if (!text && sel && sel.focusNode) {
    // Select the placeholder so typing replaces it.
    const node = sel.focusNode;
    const end = sel.focusOffset - d.length;
    if (node.nodeType === Node.TEXT_NODE && end >= 1) {
      const r = document.createRange();
      r.setStart(node, end - body.length);
      r.setEnd(node, end);
      sel.removeAllRanges();
      sel.addRange(r);
    }
  }
  onInput();
  return true;
}

/** Wraps the selection in a link (or removes links when `url` is empty). */
export function setLink(url) {
  if (!session) return;
  if (!url) exec('unlink');
  else exec('createLink', /^(https?:|mailto:|#)/i.test(url) ? url : `https://${url}`);
}

// ---------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------

let tableSession = null;

export const isEditingTable = () => !!tableSession;
export const tableNode = () => tableSession?.node || null;

export async function beginTableEdit(view, el, point = null) {
  const start = ++editStart;
  const docId = state.doc?.docId;
  const slide = state.slideId;
  if (tableSession?.id === el.id) return;
  if (tableSession && !(await endTableEdit())) return;
  if (session && !(await endTextEdit())) return;
  if (start !== editStart || state.doc?.docId !== docId || state.slideId !== slide) return;
  const wrapper = view.node(el.id);
  const table = wrapper?.querySelector('table');
  if (!table) return;
  view.editingId = el.id;
  const cells = [...table.querySelectorAll('td, th')];
  for (const c of cells) {
    // Show the LaTeX source of typeset math while editing.
    if (c.querySelector('.cz-math-inline, .cz-math-display')) {
      c.innerHTML = el.content.rows[Number(c.dataset.r)]?.[Number(c.dataset.c)] ?? c.innerHTML;
    }
    c.setAttribute('contenteditable', 'true');
  }
  tableSession = { docId, id: el.id, slide, view, node: table, rows: el.content.rows };
  table.addEventListener('keydown', onTableKey);
  table.addEventListener('paste', onPaste);
  const s = tableSession;
  s.unregister = registerFlush(() => commitTable(s));
  let target = cells[0];
  if (point) {
    const hit = document.elementFromPoint(point.x, point.y)?.closest('td, th');
    if (hit && table.contains(hit)) target = hit;
  }
  target?.focus();
  if (target) placeCaret(target, point, false);
  set({ editingTable: el.id, selection: [el.id], focus: 'canvas' });
}

function onTableKey(e) {
  if (e.key === 'Escape') {
    e.preventDefault();
    e.stopPropagation();
    endTableEdit();
    return;
  }
  if (e.key === 'Tab') {
    e.preventDefault();
    const cells = [...tableSession.node.querySelectorAll('td, th')];
    const i = cells.indexOf(document.activeElement);
    const next = cells[(i + (e.shiftKey ? -1 : 1) + cells.length) % cells.length];
    next?.focus();
    if (next) placeCaret(next, null, true);
  }
}

function readTable(table) {
  return [...table.querySelectorAll('tr')].map((tr) => [...tr.children].map((c) => cleanHtml(c.innerHTML)));
}

function commitTable(s) {
  if (!s || s.docId !== state.doc?.docId) return Promise.resolve(true);
  if (!s.committing) s.committing = commitRows(s).finally(() => { s.committing = null; });
  return s.committing;
}

async function commitRows(s) {
  for (;;) {
    const rows = readTable(s.node);
    if (JSON.stringify(rows) === JSON.stringify(s.rows) || !elementExists(s)) return true;
    const out = await apply({ op: 'updateElements', slide: s.slide, updates: [{ id: s.id, patch: { content: { rows } } }] }, { docId: s.docId, coalesce: 'table:' + s.id, navigate: false });
    if (!out) return false;
    s.rows = rows;
  }
}

export async function endTableEdit() {
  const s = tableSession;
  if (!s) return true;
  if (!(await commitTable(s))) return false;
  if (tableSession !== s) return true;
  tableSession = null;
  s.node.removeEventListener('keydown', onTableKey);
  s.node.removeEventListener('paste', onPaste);
  s.unregister?.();
  for (const c of s.node.querySelectorAll('[contenteditable]')) c.removeAttribute('contenteditable');
  s.view.editingId = null;
  seal();
  if (state.editingTable === s.id) set({ editingTable: null });
  window.dispatchEvent(new CustomEvent('cz-edit-ended'));
  return true;
}

/** Ends any inline editing. */
export async function endAllEditing() {
  if (!(await endTextEdit())) return false;
  return endTableEdit();
}
