// Keeps a slide's DOM in sync with the HTML rendered by Rust.
//
// Updates are diffed per element (keyed by `data-cz-id`) so that unchanged
// elements keep their DOM — and the element whose text is being edited
// keeps its content and caret while its styles still update live.

import { t, N_, language } from './i18n.js';

const PRESERVED_ATTRS = new Set(['contenteditable', 'spellcheck']);

/**
 * The English placeholder texts Rust puts in empty boxes, listed so the
 * extraction tool picks them up; translatePlaceholders() shows them in the
 * interface language.
 */
export const PLACEHOLDERS = [
  N_('Presentation title'),
  N_('Subtitle'),
  N_('Slide title'),
  N_('Click to add text'),
  N_('SECTION 01'),
  N_('Section title'),
  N_('Left column'),
  N_('Right column'),
  N_('Describe the image'),
  N_('Caption'),
  N_('A bold statement.'),
  N_('“A memorable quote goes here.”'),
  N_('— Attribution'),
  N_('What this number means'),
  N_('Double-click to choose an image'),
  N_('Double-click to choose a video'),
  N_('Live camera'),
  N_('YouTube video'),
  N_('Vimeo video'),
  N_('Web page — set its address in the inspector'),
  N_('Rendering diagram…'),
];

/**
 * Translates the placeholder texts of freshly rendered nodes. Runs after the
 * node's source HTML was cached (`__czSrc`), so the diffing still compares
 * Rust's original HTML.
 */
function translatePlaceholders(root) {
  for (const el of root.querySelectorAll('.cz-placeholder, .cz-placeholder-media span:not(.cz-ph-icon)')) {
    el.textContent = t(el.textContent);
  }
}

let hljs = null;
function highlighter() {
  if (!hljs && window.RevealHighlight) {
    try {
      hljs = window.RevealHighlight().hljs;
    } catch {
      hljs = null;
    }
  }
  return hljs;
}

/** Same trimming reveal.js applies to `data-trim` code blocks. */
function trimCode(text) {
  const lines = text.replace(/\t/g, '  ').split('\n');
  while (lines.length && !lines[0].trim()) lines.shift();
  while (lines.length && !lines[lines.length - 1].trim()) lines.pop();
  const indent = Math.min(...lines.filter((l) => l.trim()).map((l) => l.match(/^ */)[0].length), Infinity);
  return lines.map((l) => (Number.isFinite(indent) ? l.slice(indent) : l)).join('\n');
}

/** Renders math, highlights code and translates placeholders inside freshly inserted nodes. */
export function postProcess(root) {
  translatePlaceholders(root);
  if (window.katex) {
    // Equation elements and inline math inside text ($…$, $$…$$).
    for (const el of root.querySelectorAll('.cz-math[data-latex], .cz-math-inline[data-latex], .cz-math-display[data-latex]')) {
      const display = !el.classList.contains('cz-math-inline');
      try {
        window.katex.render(el.getAttribute('data-latex') || '', el, { displayMode: display, throwOnError: false, output: 'html' });
      } catch {
        /* keep the source text */
      }
    }
  }
  const h = highlighter();
  for (const code of root.querySelectorAll('pre.cz-code code')) {
    code.textContent = trimCode(code.textContent);
    if (!h) continue;
    try {
      h.highlightElement(code);
      if (code.hasAttribute('data-line-numbers')) h.lineNumbersBlock(code, { singleLine: true });
    } catch {
      /* unknown language: leave plain */
    }
  }
}

const keyOf = (node) => node.getAttribute?.('data-cz-id') || (node.classList?.contains('cz-bg') ? '__bg' : null);

function syncAttributes(target, source) {
  for (const { name } of [...target.attributes]) {
    if (!source.hasAttribute(name) && !PRESERVED_ATTRS.has(name)) target.removeAttribute(name);
  }
  for (const { name, value } of [...source.attributes]) {
    if (target.getAttribute(name) !== value) target.setAttribute(name, value);
  }
}

/** Updates the element being edited without touching its text. */
function patchEditing(old, neu) {
  syncAttributes(old, neu);
  for (const sel of ['.cz-rot', '.cz-box', '.cz-shape-text', '.cz-text']) {
    const a = old.querySelector(sel);
    const b = neu.querySelector(sel);
    if (a && b) syncAttributes(a, b);
  }
  const oldSvg = old.querySelector('.cz-shape-svg');
  const newSvg = neu.querySelector('.cz-shape-svg');
  if (oldSvg && newSvg && oldSvg.outerHTML !== newSvg.outerHTML) oldSvg.replaceWith(newSvg);
}

export class SlideView {
  constructor(root) {
    this.root = root;
    this.html = null;
    this.lang = null;
    this.editingId = null;
  }

  update(html, force = false) {
    const lang = language();
    if (!force && html === this.html && lang === this.lang) return false;
    // Placeholders are translated when a node is inserted: a new interface
    // language re-renders every element.
    if (lang !== this.lang) {
      for (const node of this.root.children) node.__czSrc = null;
    }
    this.html = html;
    this.lang = lang;
    const tpl = document.createElement('template');
    tpl.innerHTML = html || '';
    const existing = new Map();
    for (const node of [...this.root.children]) {
      const k = keyOf(node);
      if (k) existing.set(k, node);
      else node.remove();
    }
    let cursor = null;
    for (const node of [...tpl.content.children]) {
      const k = keyOf(node);
      const src = node.outerHTML;
      let out = k ? existing.get(k) : null;
      if (out) {
        existing.delete(k);
        if (out.__czSrc !== src) {
          if (k === this.editingId) {
            patchEditing(out, node);
            // Content was kept as typed: mark stale so it is re-rendered
            // (math typeset, markup normalized) once editing ends.
            out.__czSrc = null;
          } else {
            node.__czSrc = src;
            postProcess(node);
            out.replaceWith(node);
            out = node;
          }
        }
      } else {
        node.__czSrc = src;
        postProcess(node);
        out = node;
      }
      const want = cursor ? cursor.nextSibling : this.root.firstChild;
      if (out !== want) this.root.insertBefore(out, want);
      cursor = out;
    }
    for (const node of existing.values()) node.remove();
    return true;
  }

  node(id) {
    return this.root.querySelector(`:scope > [data-cz-id="${CSS.escape(id)}"]`);
  }
}
