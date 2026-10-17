// Inline rich-text editing on the canvas (text boxes, shapes, table cells).
//
// The rendered element itself becomes `contenteditable`; changes are
// committed to Rust (debounced; typing and each formatting command are undo
// steps of their own) and the SlideView leaves the edited element's content
// alone while it is active.

import { state, set, apply, registerFlush, registerHistorySync, seal, richOf, undo, redo } from './store.js';
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
  // The editor changes what the box shows (math as LaTeX, no placeholder):
  // it is drawn again when editing ends, even if nothing was changed.
  wrapper.__czSrc = null;
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
    lastHtml: wasEmpty ? '' : cleanHtml(node.innerHTML),
    // The element's HTML in the document, to see what an undo changed.
    model: richOf(el).html,
    timer: null,
    range: null,
    // Undo steps: typing is one, each formatting command another.
    step: 0,
    last: null,
    inCommand: false,
    // Font, size or color for the whole box, sent with the next commit.
    boxStyle: null,
  };
  onSelectionChange();
  document.addEventListener('selectionchange', onSelectionChange);
  node.addEventListener('input', onInput);
  node.addEventListener('paste', onPaste);
  node.addEventListener('keydown', onKeyDown);
  const s = session;
  s.unregister = registerFlush(() => commit(s));
  set({ editing: el.id, selection: [el.id], focus: 'canvas' });
}

/**
 * Keeps the last selection in the edited text: the inspector's fields take
 * the focus (a font search, a size), and formatting still goes to that text.
 */
function onSelectionChange() {
  const sel = window.getSelection();
  if (session && sel?.rangeCount && session.node.contains(sel.anchorNode) && session.node.contains(sel.focusNode)) {
    session.range = sel.getRangeAt(0).cloneRange();
  }
}

function onInput() {
  if (!session) return;
  const s = session;
  if (!s.inCommand && s.last !== 'type') {
    s.last = 'type';
    s.step++;
  }
  clearTimeout(s.timer);
  s.timer = setTimeout(() => commit(s), 500);
  window.dispatchEvent(new CustomEvent('cz-text-input'));
}

/**
 * Runs a command on the text (formatting, a paste, an equation) as an undo
 * step of its own, sent at once; what was typed before it is another step.
 * Live changes of one `group` (a color being dragged) make a single step.
 */
function asStep(s, run, group = null) {
  if (s.inCommand) return run();
  clearTimeout(s.timer);
  commit(s);
  if (!group || s.last !== group) s.step++;
  s.last = group || 'command';
  s.inCommand = true;
  try {
    return run();
  } finally {
    s.inCommand = false;
    commit(s);
    window.dispatchEvent(new CustomEvent('cz-text-input'));
  }
}

function onPaste(e) {
  e.preventDefault();
  e.stopPropagation();
  const text = e.clipboardData.getData('text/plain');
  if (!text) return;
  if (session && session.node.contains(e.target)) asStep(session, () => document.execCommand('insertText', false, text));
  else document.execCommand('insertText', false, text);
}

/**
 * Ctrl+Z and Ctrl+Shift+Z (or Ctrl+Y) while editing undo the document's
 * steps. WebKitGTK binds no key to its own undo, and its undo stack holds
 * every field of the window, so it could undo text typed somewhere else.
 */
function historyKey(e) {
  const k = e.key.toLowerCase();
  if (!(e.ctrlKey || e.metaKey) || e.altKey || (k !== 'z' && k !== 'y')) return false;
  e.preventDefault();
  e.stopPropagation();
  if (k === 'y' || e.shiftKey) redo();
  else undo();
  return true;
}

function onKeyDown(e) {
  if (historyKey(e)) return;
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
  // typed while that request was pending (or a command run right after it).
  if (s.committing) return s.committing.then(() => commit(s));
  s.committing = commitText(s).finally(() => { s.committing = null; });
  return s.committing;
}

/**
 * Whether an edited element still exists. Once it is deleted, undone or
 * replaced, its pending edit has nowhere to go and must not block saving or
 * closing forever.
 */
const elementOf = (s) => state.doc?.deck.slides.find((x) => x.id === s.slide)?.elements.find((e) => e.id === s.id);
const elementExists = (s) => !!elementOf(s);

async function commitText(s) {
  for (;;) {
    const html = cleanHtml(s.node.innerHTML);
    const style = s.boxStyle;
    if ((html === s.lastHtml && !style) || !elementExists(s)) return true;
    s.boxStyle = null;
    const text = style ? { html, style } : { html };
    const patch = s.kind === 'shape' ? { content: { text } } : { content: text };
    const out = await apply({ op: 'updateElements', slide: s.slide, updates: [{ id: s.id, patch }] }, { docId: s.docId, coalesce: `text:${s.id}:${s.step}`, navigate: false });
    if (!out) return false;
    s.lastHtml = html;
    s.model = elementOf(s) ? richOf(elementOf(s)).html : s.model;
  }
}

/**
 * Drops the empty markup editors leave behind, and the styles WebKit copies
 * from the theme into the text (turning a list back into a paragraph, joining
 * paragraphs): pinned there, they would keep the text from following its
 * box's font, color or alignment.
 */
function cleanHtml(html) {
  const trimmed = html.trim();
  if (trimmed === '<p><br></p>' || trimmed === '<br>' || trimmed === '<div><br></div>') return '';
  if (!/var\(--r-|inherit/.test(html)) return html;
  const tpl = document.createElement('template');
  tpl.innerHTML = html;
  for (const e of [...tpl.content.querySelectorAll('[style]')]) {
    for (const prop of [...e.style]) {
      const v = e.style.getPropertyValue(prop);
      if (v.includes('var(--r-') || v === 'inherit') e.style.removeProperty(prop);
    }
    if (!e.getAttribute('style')) e.removeAttribute('style');
    if (e.tagName === 'SPAN' && !e.attributes.length) e.replaceWith(...e.childNodes);
  }
  return tpl.innerHTML;
}

/** Finishes editing, committing any pending change. */
export async function endTextEdit() {
  const s = session;
  if (!s) return true;
  if (!(await commit(s))) return false;
  if (session !== s) return true;
  session = null;
  document.removeEventListener('selectionchange', onSelectionChange);
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
  const sel = window.getSelection();
  const inside = sel.rangeCount > 0 && session.node.contains(sel.anchorNode);
  session.node.focus({ preventScroll: true });
  if (!inside && session.range) {
    sel.removeAllRanges();
    sel.addRange(session.range);
  }
  const v = concrete(value);
  const ok = asStep(session, () => document.execCommand(cmd, false, v));
  // Now, not when selectionchange comes: the focus may go back to a field.
  onSelectionChange();
  return ok;
}

/** A theme color (`var(…)`) as its current value: parts of a text keep that. */
function concrete(v) {
  if (typeof v !== 'string' || !v.startsWith('var(')) return v;
  const c = parseColor(v);
  return c ? toCss(c) : v;
}

/** Whether some text is selected (not just a caret) in the text being edited. */
export const textSelected = () => !!session?.range && !session.range.collapsed && session.node.contains(session.range.commonAncestorContainer);

/**
 * Sets a style on the selected text: a font, a size in px or a color; ''
 * takes it away, so that the text looks like the rest of its box again.
 */
export function styleSelection(prop, value, live = false) {
  if (!session) return;
  // Commands WebKit knows, then their marker values become the real ones.
  const [cmd, marker, found] = {
    'font-family': ['fontName', 'cz-mark', (v) => v.replace(/["']/g, '') === 'cz-mark'],
    'font-size': ['fontSize', '7', (v) => v === '-webkit-xxx-large' || v === 'xxx-large'],
    color: ['foreColor', '#010203', (v) => v === 'rgb(1, 2, 3)' || v === '#010203'],
  }[prop];
  const back = document.activeElement;
  const s = session;
  if (selectsAll(s)) {
    // All of the text: its box takes the style, so that line heights, bullets
    // and numbers follow (smaller text in a span keeps the box's line height).
    const key = { 'font-family': 'fontFamily', 'font-size': 'fontSize', color: 'color' }[prop];
    // The text stays selected, as when a part of it is styled.
    s.node.focus({ preventScroll: true });
    const sel = window.getSelection();
    sel.removeAllRanges();
    sel.addRange(s.range);
    asStep(
      s,
      () => {
        unstyle(s, prop);
        s.boxStyle = { ...s.boxStyle, [key]: prop === 'font-size' ? parseFloat(value) : value };
      },
      live && prop,
    );
  } else asStep(s, () => restyle(s, prop, concrete(value), cmd, marker, found), live && prop);
  // The inspector's field keeps the focus (Up and Down in the size field).
  if (back?.tagName === 'INPUT' && !s.node.contains(back)) back.focus({ preventScroll: true });
}

/** Whether the selection holds all of the text being edited. */
function selectsAll(s) {
  const r = s.range;
  if (!r || r.collapsed || !s.node.contains(r.commonAncestorContainer)) return false;
  const rest = document.createRange();
  rest.selectNodeContents(s.node);
  rest.setEnd(r.startContainer, r.startOffset);
  const before = rest.toString();
  rest.selectNodeContents(s.node);
  rest.setStart(r.endContainer, r.endOffset);
  return !before.trim() && !rest.toString().trim();
}

/** Takes a style away from every part of the text, keeping the selection. */
function unstyle(s, prop) {
  const at = s.range && textOffsets(s.node, s.range);
  const attr = { 'font-family': 'face', 'font-size': 'size', color: 'color' }[prop];
  for (const e of [...s.node.querySelectorAll('[style], font')]) {
    e.style?.removeProperty(prop);
    if (e.getAttribute('style') === '') e.removeAttribute('style');
    if (e.tagName === 'FONT') e.removeAttribute(attr);
    if ((e.tagName === 'SPAN' || e.tagName === 'FONT') && !e.attributes.length) e.replaceWith(...e.childNodes);
  }
  if (!at) return;
  s.range = rangeAt(s.node, at);
  if (document.activeElement === s.node) selectOffsets(s.node, at);
}

function restyle(s, prop, value, cmd, marker, found) {
  if (!exec(cmd, marker)) return;
  for (const e of s.node.querySelectorAll('[style], font')) {
    if (e.tagName === 'FONT') {
      // Older WebKit writes <font size="7">.
      const span = document.createElement('span');
      if (e.getAttribute('size') === '7') span.style.setProperty('font-size', value);
      if (e.getAttribute('face') === 'cz-mark') span.style.setProperty('font-family', value);
      if (e.getAttribute('color') === '#010203') span.style.setProperty('color', value);
      span.append(...e.childNodes);
      e.replaceWith(span);
    } else if (found(e.style.getPropertyValue(prop))) {
      if (value) e.style.setProperty(prop, value);
      else e.style.removeProperty(prop);
      if (!e.getAttribute('style')) e.removeAttribute('style');
      if (!value && e.tagName === 'SPAN' && !e.attributes.length) e.replaceWith(...e.childNodes);
    }
  }
}

/** A style of the selected text where it differs from its box ('' if not). */
export function selectionStyle(prop) {
  if (!textSelected()) return '';
  for (let e = selectionStart(session.range); e && e !== session.node; e = e.parentElement) {
    const v = e.style?.getPropertyValue(prop);
    if (v) return v;
  }
  return '';
}

/** The font size of the selected text, in px. */
export function selectionFontSize() {
  if (!textSelected()) return 0;
  return parseFloat(getComputedStyle(selectionStart(session.range)).fontSize) || 0;
}

/**
 * The element holding the first selected character. A selection made with
 * the keyboard often starts at the very end of the text before it, which can
 * lie outside the styled span the selected text is in.
 */
function selectionStart(range) {
  let n = range.startContainer;
  if (n.nodeType === Node.TEXT_NODE && range.startOffset >= n.length) {
    const walker = document.createTreeWalker(range.commonAncestorContainer, NodeFilter.SHOW_TEXT);
    walker.currentNode = n;
    for (let t = walker.nextNode(); t && range.intersectsNode(t); t = walker.nextNode()) {
      if (t.length) {
        n = t;
        break;
      }
    }
  } else if (n.nodeType === Node.ELEMENT_NODE && n.childNodes[range.startOffset]) {
    n = n.childNodes[range.startOffset];
  }
  return n.nodeType === Node.ELEMENT_NODE ? n : n.parentElement;
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
  return asStep(session, () => insertMath(display));
}

function insertMath(display) {
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
  // Drawn again when editing ends: cells show their math as LaTeX meanwhile.
  wrapper.__czSrc = null;
  const cells = [...table.querySelectorAll('td, th')];
  for (const c of cells) {
    // Show the LaTeX source of typeset math while editing.
    if (c.querySelector('.cz-math-inline, .cz-math-display')) {
      c.innerHTML = el.content.rows[Number(c.dataset.r)]?.[Number(c.dataset.c)] ?? c.innerHTML;
    }
    c.setAttribute('contenteditable', 'true');
  }
  tableSession = { docId, id: el.id, slide, view, node: table, rows: el.content.rows, model: JSON.stringify(el.content.rows) };
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
  if (historyKey(e)) return;
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
    s.model = elementOf(s) ? JSON.stringify(elementOf(s).content.rows) : s.model;
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

// ---------------------------------------------------------------------------
// Undo and redo while editing
// ---------------------------------------------------------------------------

/** The selection inside `root` (or a range), as character offsets. */
function textOffsets(root, range = null) {
  const sel = window.getSelection();
  if (!range && (!sel?.rangeCount || !root.contains(sel.anchorNode))) return null;
  const r = range || sel.getRangeAt(0);
  const at = (node, offset) => {
    const before = document.createRange();
    before.selectNodeContents(root);
    before.setEnd(node, offset);
    return before.toString().length;
  };
  return [at(r.startContainer, r.startOffset), at(r.endContainer, r.endOffset)];
}

function rangeAt(root, [start, end]) {
  const point = (n) => {
    const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    let last = null;
    for (let t = walk.nextNode(); t; t = walk.nextNode()) {
      if (n <= t.length) return [t, n];
      n -= t.length;
      last = t;
    }
    return last ? [last, last.length] : [root, root.childNodes.length];
  };
  const r = document.createRange();
  r.setStart(...point(start));
  r.setEnd(...point(end));
  return r;
}

function selectOffsets(root, at) {
  const sel = window.getSelection();
  sel.removeAllRanges();
  sel.addRange(rangeAt(root, at));
}

/**
 * After an undo or redo, the element being edited shows what the document
 * now holds, keeping the selection where it was. Editing ends when the
 * element is gone or the undo went to another slide.
 */
function syncAfterHistory() {
  const s = session;
  const el = s && elementOf(s);
  if (s && (!el || state.slideId !== s.slide || state.editing !== s.id)) endTextEdit();
  else if (s && richOf(el).html !== s.model) {
    const at = textOffsets(s.node);
    s.model = richOf(el).html;
    s.node.innerHTML = s.model || '<p><br></p>';
    s.lastHtml = cleanHtml(s.node.innerHTML);
    s.last = null;
    s.range = null;
    if (at) selectOffsets(s.node, at);
    onSelectionChange();
    window.dispatchEvent(new CustomEvent('cz-text-input'));
  }

  const t = tableSession;
  const rows = t && elementOf(t)?.content.rows;
  if (!t) return;
  const shape = (rr) => rr.map((r) => r.length).join();
  if (!rows || state.slideId !== t.slide || state.editingTable !== t.id || shape(rows) !== shape(readTable(t.node))) {
    endTableEdit();
  } else if (JSON.stringify(rows) !== t.model) {
    const cell = document.activeElement?.closest?.('td, th');
    const at = cell && t.node.contains(cell) ? textOffsets(cell) : null;
    for (const c of t.node.querySelectorAll('td, th')) c.innerHTML = rows[Number(c.dataset.r)]?.[Number(c.dataset.c)] ?? '';
    t.rows = readTable(t.node);
    t.model = JSON.stringify(rows);
    if (at) selectOffsets(cell, at);
  }
}

registerHistorySync(syncAfterHistory);
