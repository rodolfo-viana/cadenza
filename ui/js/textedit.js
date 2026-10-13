// Inline rich-text editing on the canvas (text boxes, shapes, table cells).
//
// The rendered element itself becomes `contenteditable`; changes are
// committed to Rust (debounced, one undo step per editing session) and the
// SlideView leaves the edited element's content alone while it is active.

import { state, set, apply, registerFlush, seal, richOf } from './store.js';
import { parseColor, toCss } from './controls.js';

let session = null;

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
export function beginTextEdit(view, el, point = null, { selectAll = false } = {}) {
  if (session?.id === el.id) return;
  endTextEdit();
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
  session.unregister = registerFlush(() => commit(session));
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

async function commit(s) {
  if (!s) return;
  clearTimeout(s.timer);
  const html = cleanHtml(s.node.innerHTML);
  if (html === s.lastHtml) return;
  s.lastHtml = html;
  const patch = s.kind === 'shape' ? { content: { text: { html } } } : { content: { html } };
  await apply({ op: 'updateElements', slide: s.slide, updates: [{ id: s.id, patch }] }, { coalesce: 'text:' + s.id, navigate: false });
}

/** Drops the empty markup editors leave behind. */
function cleanHtml(html) {
  const trimmed = html.trim();
  return trimmed === '<p><br></p>' || trimmed === '<br>' || trimmed === '<div><br></div>' ? '' : html;
}

/** Finishes editing, committing any pending change. */
export async function endTextEdit() {
  const s = session;
  if (!s) return;
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
  await commit(s);
  seal();
  if (state.editing === s.id) set({ editing: null });
  window.dispatchEvent(new CustomEvent('cz-edit-ended'));
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

export function beginTableEdit(view, el, point = null) {
  endTableEdit();
  endTextEdit();
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
  tableSession = { id: el.id, slide: state.slideId, view, node: table, rows: el.content.rows };
  table.addEventListener('keydown', onTableKey);
  table.addEventListener('paste', onPaste);
  tableSession.unregister = registerFlush(() => commitTable(tableSession));
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

async function commitTable(s) {
  if (!s) return;
  const rows = readTable(s.node);
  if (JSON.stringify(rows) === JSON.stringify(s.rows)) return;
  s.rows = rows;
  await apply({ op: 'updateElements', slide: s.slide, updates: [{ id: s.id, patch: { content: { rows } } }] }, { coalesce: 'table:' + s.id, navigate: false });
}

export async function endTableEdit() {
  const s = tableSession;
  if (!s) return;
  tableSession = null;
  s.node.removeEventListener('keydown', onTableKey);
  s.node.removeEventListener('paste', onPaste);
  s.unregister?.();
  for (const c of s.node.querySelectorAll('[contenteditable]')) c.removeAttribute('contenteditable');
  s.view.editingId = null;
  await commitTable(s);
  seal();
  if (state.editingTable === s.id) set({ editingTable: null });
  window.dispatchEvent(new CustomEvent('cz-edit-ended'));
}

/** Ends any inline editing. */
export async function endAllEditing() {
  await endTextEdit();
  await endTableEdit();
}
