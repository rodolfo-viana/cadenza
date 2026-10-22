// Small, dependency-free helpers.

import { t, language } from './i18n.js';

export const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
export const round = (v, step = 1) => Math.round(v / step) * step;
export const isMac = navigator.platform.toLowerCase().includes('mac');
export const MOD = isMac ? '⌘' : 'Ctrl';

export function debounce(fn, ms) {
  let timer = null;
  const d = (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      fn(...args);
    }, ms);
  };
  d.flush = (...args) => {
    if (timer) {
      clearTimeout(timer);
      timer = null;
      fn(...args);
    }
  };
  d.cancel = () => clearTimeout(timer);
  return d;
}

/**
 * Follows a pointer drag on the window. `onEnd` gets the pointerup event, or
 * null when the drag was cancelled (by the system, or by calling the returned
 * function, e.g. when the component goes away mid-drag).
 */
export function trackPointer(onMove, onEnd) {
  let done = false;
  const end = (ev) => {
    if (done) return;
    done = true;
    window.removeEventListener('pointermove', onMove);
    window.removeEventListener('pointerup', end);
    window.removeEventListener('pointercancel', end);
    onEnd(ev?.type === 'pointerup' ? ev : null);
  };
  window.addEventListener('pointermove', onMove);
  window.addEventListener('pointerup', end);
  window.addEventListener('pointercancel', end);
  return () => end(null);
}

export function escapeHtml(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
}

/** Whether rich text shows no characters, only spaces (parsed inert: nothing runs or loads). */
export function isBlankHtml(html) {
  const tpl = document.createElement('template');
  tpl.innerHTML = html || '';
  return !tpl.content.textContent.replace(/[\s​]/g, '');
}

/** Plain text → paragraphs of escaped HTML (mirrors the Rust helper). */
export function textToHtml(text) {
  return String(text)
    .split(/\r?\n/)
    .map((l) => (l.trim() ? `<p>${escapeHtml(l)}</p>` : '<p><br></p>'))
    .join('');
}

export const fileExt = (p) => (String(p).split(/[\\/]/).pop().split('.').pop() || '').toLowerCase();
export const fileName = (p) => String(p).split(/[\\/]/).pop();
export const clone = (v) => (v === undefined ? v : JSON.parse(JSON.stringify(v)));

/** True when keyboard input should go to a text field rather than the app. */
export function isTyping(target) {
  const el = target || document.activeElement;
  if (!el || el === document.body) return false;
  if (el.isContentEditable) return true;
  const tag = el.tagName;
  if (tag === 'TEXTAREA' || tag === 'SELECT') return true;
  if (tag === 'INPUT') return !['checkbox', 'radio', 'range', 'button', 'color'].includes(el.type);
  return false;
}

/** Whether a control of the interface outside the slide canvas (a button, toggle, slider…) has the keyboard. */
export function isControl(target) {
  const el = target || document.activeElement;
  if (!el?.matches || el === document.body || el.closest('.canvas-wrap')) return false;
  return el.matches('button, a[href], input, select, textarea, [tabindex]:not([tabindex="-1"])');
}

const CODE_KEYS = {
  BracketLeft: '[',
  BracketRight: ']',
  Equal: '=',
  Minus: '-',
  Slash: '/',
  Comma: ',',
  Period: '.',
  Backquote: '`',
  Semicolon: ';',
  NumpadAdd: '+',
  NumpadSubtract: '-',
};

/**
 * Normalized key combo such as "Mod+Shift+Z" ("Mod" = Ctrl, or ⌘ on macOS).
 * Letters follow the keyboard layout; punctuation and digits use the
 * physical key so that Shift does not change them ("Mod+Shift+]").
 */
export function combo(e) {
  const parts = [];
  if (isMac ? e.metaKey : e.ctrlKey) parts.push('Mod');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  let k = e.key;
  if (k === ' ') k = 'Space';
  else if (/^[a-z]$/i.test(k)) k = k.toUpperCase();
  else if (CODE_KEYS[e.code]) k = CODE_KEYS[e.code];
  else if (/^Digit\d$/.test(e.code)) k = e.code.slice(5);
  else if (k.length === 1) k = k.toUpperCase();
  parts.push(k);
  return parts.join('+');
}

/** Human-readable shortcut label. */
export const keyLabel = (c) =>
  c
    .replace('Mod', MOD)
    .replace('Shift', isMac ? '⇧' : 'Shift')
    .replace('Alt', isMac ? '⌥' : 'Alt')
    .replace('ArrowUp', '↑')
    .replace('ArrowDown', '↓')
    .replace('ArrowLeft', '←')
    .replace('ArrowRight', '→')
    .replace(/\+/g, isMac ? '' : '+');

export function rotate(px, py, cx, cy, deg) {
  const r = (deg * Math.PI) / 180;
  const c = Math.cos(r);
  const s = Math.sin(r);
  const dx = px - cx;
  const dy = py - cy;
  return [cx + dx * c - dy * s, cy + dx * s + dy * c];
}

/** Axis-aligned bounds of a (possibly rotated) element. */
export function bounds(el) {
  if (!el.rotation) return { x: el.x, y: el.y, w: el.w, h: el.h };
  const cx = el.x + el.w / 2;
  const cy = el.y + el.h / 2;
  const pts = [
    [el.x, el.y],
    [el.x + el.w, el.y],
    [el.x + el.w, el.y + el.h],
    [el.x, el.y + el.h],
  ].map(([x, y]) => rotate(x, y, cx, cy, el.rotation));
  const xs = pts.map((p) => p[0]);
  const ys = pts.map((p) => p[1]);
  const x = Math.min(...xs);
  const y = Math.min(...ys);
  return { x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y };
}

export const intersects = (a, b) => a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;

export function unionBounds(list) {
  if (!list.length) return null;
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const b of list) {
    x0 = Math.min(x0, b.x);
    y0 = Math.min(y0, b.y);
    x1 = Math.max(x1, b.x + b.w);
    y1 = Math.max(y1, b.y + b.h);
  }
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/** "just now", "5 minutes ago", "yesterday"… in the interface language; older dates in full. */
export function timeAgo(ms) {
  if (!ms) return '';
  const s = (Date.now() - ms) / 1000;
  const rtf = new Intl.RelativeTimeFormat(language(), { numeric: 'auto' });
  if (s < 60) return t('just now');
  if (s < 3600) return rtf.format(-Math.floor(s / 60), 'minute');
  if (s < 86400) return rtf.format(-Math.floor(s / 3600), 'hour');
  if (s < 86400 * 7) return rtf.format(-Math.floor(s / 86400), 'day');
  return new Date(ms).toLocaleDateString(language());
}

export const cls = (...xs) => xs.filter(Boolean).join(' ');

/**
 * Positions in `text` of the characters in `chars` that are outside
 * strings, comments and brackets.
 */
function topLevelAt(text, chars) {
  const at = [];
  let depth = 0;
  let quote = null;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quote) {
      if (c === '\\') i++;
      else if (c === quote) quote = null;
    } else if (c === '"' || c === "'") quote = c;
    else if (c === '/' && text[i + 1] === '*') {
      const end = text.indexOf('*/', i + 2);
      i = end < 0 ? text.length : end + 1;
    } else if ('([{'.includes(c)) {
      if (depth === 0 && chars.includes(c)) at.push(i);
      depth++;
    } else if (')]}'.includes(c)) depth = Math.max(0, depth - 1);
    else if (depth === 0 && chars.includes(c)) at.push(i);
  }
  return at;
}

/**
 * The top-level parts of `css`: rules, which end at their closing brace,
 * and statements, which end at `;`. Strings and comments are skipped; the
 * parts joined give `css` back.
 */
function cssParts(css) {
  const parts = [];
  let depth = 0;
  let start = 0;
  let quote = null;
  for (let i = 0; i < css.length; i++) {
    const c = css[i];
    if (quote) {
      if (c === '\\') i++;
      else if (c === quote) quote = null;
    } else if (c === '"' || c === "'") quote = c;
    else if (c === '/' && css[i + 1] === '*') {
      const end = css.indexOf('*/', i + 2);
      i = end < 0 ? css.length : end + 1;
    } else if (c === '{') depth++;
    else if ((c === '}' && depth > 0 && --depth === 0) || (c === ';' && depth === 0)) {
      parts.push(css.slice(start, i + 1));
      start = i + 1;
    }
  }
  if (start < css.length) parts.push(css.slice(start));
  return parts;
}

/** A selector's leading part that stands for the page itself (html, body…). */
const PAGE_ROOT = /^(?:html|body|:root|\.reveal-viewport)+(?![\w-])/;

/**
 * One selector, the page's root (`html`, `body`, `:root` or reveal.js'
 * `.reveal-viewport`, as in `body .reveal h1`) made the scope's root.
 * Whitespace and comments around it are kept.
 */
function scopeSelector(part) {
  const [, lead, core, trail] = part.match(/^((?:\s|\/\*[\s\S]*?\*\/)*)([\s\S]*?)(\s*)$/);
  let rest = core;
  let rooted = false;
  for (let m = rest.match(PAGE_ROOT); m; m = rest.match(PAGE_ROOT)) {
    rooted = true;
    rest = rest.slice(m[0].length);
    // `html body`, `html > body`: the root again.
    const combinator = rest.match(/^\s*>\s*|^\s+/);
    if (!combinator || !PAGE_ROOT.test(rest.slice(combinator[0].length))) break;
    rest = rest.slice(combinator[0].length);
  }
  return lead + (rooted ? ':scope' + rest : rest).replace(/:root(?![\w-])/g, ':scope') + trail;
}

/** One rule with its selectors scoped, and those of the rules inside @media, @supports…. */
function scopeRule(rule) {
  const [open] = topLevelAt(rule, '{');
  const close = rule.lastIndexOf('}');
  if (open === undefined || close < open) return rule;
  const head = rule.slice(0, open);
  const inner = rule.slice(open + 1, close);
  const tail = rule.slice(close);
  if (head.replace(/\/\*[\s\S]*?\*\//g, '').trimStart().startsWith('@')) {
    const group = /^\s*@(?:-[a-z]+-)?(?:media|supports|container|layer|document)\b/i.test(head.replace(/\/\*[\s\S]*?\*\//g, ''));
    return group ? head + '{' + cssParts(inner).map(scopeRule).join('') + tail : rule;
  }
  const cuts = [-1, ...topLevelAt(head, ','), head.length];
  return cuts.slice(1).map((end, k) => scopeSelector(head.slice(cuts[k] + 1, end))).join(',') + '{' + inner + tail;
}

/**
 * Keeps CSS to one part of the page: its rules go inside `@scope (root)`,
 * and selectors that start at the page itself (`:root`, `html`, `body`,
 * `.reveal-viewport`) start at the scope's root instead. At-rules that
 * can't be scoped (@font-face, @keyframes…) stay outside, as they style
 * nothing by themselves.
 */
export function scopeCss(css, root) {
  const global = [];
  const scoped = [];
  for (const rule of cssParts(css)) {
    if (!rule.trim()) continue;
    const head = rule.replace(/\/\*[\s\S]*?\*\//g, '').trimStart();
    (/^@(?:-[a-z]+-)?(?:font-face|keyframes|property|counter-style|font-feature-values|import|charset|namespace|page)\b/i.test(head) ? global : scoped).push(rule);
  }
  return `${global.join('')}\n@scope (${root}) {${scoped.map(scopeRule).join('')}\n}`;
}
