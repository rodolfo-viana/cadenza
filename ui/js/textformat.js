// Bold, italic and underline for whole text boxes and shapes, the way the
// same buttons and shortcuts work on selected text while editing.
//
// A box's text can look bold or italic for three reasons: its own style, its
// role in the theme (titles and headings are bold, quotes italic) and inline
// markup. Toggling a whole box looks at what is shown and evens it out: the
// box's style says bold or not, against the theme's default, and inline bold
// goes away.

import { state, updateElements, richOf, richPatch, selectedElements } from './store.js';
import { exec, isEditing, queryState, beginTextEdit, endTextEdit } from './textedit.js';
import { canvasApi } from './canvas.js';
import { isTyping } from './util.js';

const hasText = (el) => el.content.kind === 'text' || (el.content.kind === 'shape' && el.content.shape !== 'line');

// What the inline markup of each kind looks like.
const KINDS = {
  bold: {
    prop: 'font-weight',
    tags: ['B', 'STRONG'],
    on: (v) => v === 'bold' || v === 'bolder' || Number(v) >= 600,
  },
  italic: {
    prop: 'font-style',
    tags: ['I', 'EM', 'BLOCKQUOTE'],
    on: (v) => v === 'italic' || v.startsWith('oblique'),
  },
};

/** Weight and slant the theme gives a text role, as the canvas shows it. */
function roleLook(role) {
  const slide = document.querySelector('.cz-editor-reveal .cz-slide');
  if (!slide) return { weight: role === 'title' || role === 'heading' ? 700 : 400, italic: role === 'quote' };
  const probe = document.createElement('div');
  const known = ['title', 'subtitle', 'heading', 'body', 'caption', 'quote'];
  probe.className = `cz-text cz-role-${known.includes(role) ? role : 'body'}`;
  probe.style.display = 'none';
  slide.appendChild(probe);
  const cs = getComputedStyle(probe);
  const look = { weight: parseInt(cs.fontWeight, 10) || 400, italic: KINDS.italic.on(cs.fontStyle) };
  probe.remove();
  return look;
}

const parse = (html) => {
  const tpl = document.createElement('template');
  tpl.innerHTML = html || '';
  return tpl.content;
};

/** Whether all of the text shows `kind` (also when the box has no text yet). */
function shows(el, kind, look = roleLook(richOf(el).style.role)) {
  const k = KINDS[kind];
  const style = richOf(el).style;
  const base = kind === 'bold' ? (style.fontWeight || look.weight) >= 600 : style.italic || (look.italic && !style.upright);
  const root = parse(richOf(el).html);
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    if (!n.data.trim()) continue;
    let on = base;
    for (let e = n.parentElement; e; e = e.parentElement) {
      const v = e.style.getPropertyValue(k.prop);
      if (v) {
        on = k.on(v);
        break;
      }
      if (k.tags.includes(e.tagName)) {
        on = true;
        break;
      }
    }
    if (!on) return false;
  }
  return base || !!root.textContent.trim();
}

/** The text without inline markup of `kind`, so that the box's style rules. */
function plainOf(html, kind, on) {
  const k = KINDS[kind];
  const root = parse(html);
  for (const e of [...root.querySelectorAll('*')]) {
    if (e.style.getPropertyValue(k.prop)) {
      e.style.removeProperty(k.prop);
      if (!e.getAttribute('style')) e.removeAttribute('style');
    }
    if (e.tagName === 'BLOCKQUOTE') {
      // Quotes are italic: say so when the rest isn't.
      if (kind === 'italic' && !on) e.style.setProperty('font-style', 'normal');
    } else if (e.tagName === 'SPAN' || k.tags.includes(e.tagName)) {
      if (!e.attributes.length) {
        e.replaceWith(...e.childNodes);
      } else if (e.tagName !== 'SPAN') {
        // Keeps its other formatting (a color, say).
        const span = document.createElement('span');
        for (const a of e.attributes) span.setAttribute(a.name, a.value);
        span.append(...e.childNodes);
        e.replaceWith(span);
      }
    }
  }
  const tpl = document.createElement('template');
  tpl.content.append(root);
  return tpl.innerHTML;
}

/** Whether every box in `els` shows `kind` ('bold' or 'italic') in all its text. */
export const allShow = (els, kind) => els.length > 0 && els.every((el) => shows(el, kind));

/**
 * Turns bold or italic on for whole boxes, or off when all their text shows
 * it already.
 */
export function toggleBoxes(els, kind) {
  els = els.filter(hasText);
  if (!els.length) return null;
  const on = !allShow(els, kind);
  return updateElements(
    els.map((e) => e.id),
    (id) => {
      const el = els.find((e) => e.id === id);
      const look = roleLook(richOf(el).style.role);
      const style =
        kind === 'bold'
          ? { fontWeight: on === look.weight >= 600 ? 0 : on ? 700 : 400 }
          : { italic: on && !look.italic, upright: !on && look.italic };
      const html = richOf(el).html;
      const plain = plainOf(html, kind, on);
      return richPatch(el, plain === html ? { style } : { style, html: plain });
    },
  );
}

/** Underline and other inline commands for a whole box: edits all of its text. */
export async function formatBox(el, cmd, value) {
  if (!canvasApi.view) return;
  await beginTextEdit(canvasApi.view, el, null, { selectAll: true });
  if (!isEditing()) return;
  exec(cmd, value);
  await endTextEdit();
}

/** Whether the text being edited, or else the selected boxes, show `kind`. */
export const formatActive = (els, kind) => (isEditing() ? queryState(kind) : allShow(els.filter(hasText), kind));

/**
 * The Bold, Italic and Underline shortcuts: the selected text while editing,
 * the whole of the selected boxes otherwise.
 */
export function formatShortcut(cmd) {
  if (isEditing()) return exec(cmd);
  const focus = document.activeElement;
  // Table cells being edited.
  if (focus?.isContentEditable) return document.execCommand(cmd);
  if (isTyping(focus) || state.focus !== 'canvas') return;
  const els = selectedElements().filter(hasText);
  if (cmd === 'underline') return els.length === 1 ? formatBox(els[0], cmd) : undefined;
  return toggleBoxes(els, cmd);
}
