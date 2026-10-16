// Form controls used by the inspector.

import { html, useEffect, useLayoutEffect, useMemo, useRef, useState } from './lib.js';
import { Icon } from './icons.js';
import { state, registerFlush } from './store.js';
import { clamp, cls } from './util.js';
import { t, N_ } from './i18n.js';

export function Section({ title, children, actions, open: initialOpen = true, id }) {
  const key = id && `cz-sec-${id}`;
  const [open, setOpen] = useState(() => {
    if (!key) return initialOpen;
    try {
      const v = localStorage.getItem(key);
      return v === null ? initialOpen : v === '1';
    } catch {
      return initialOpen;
    }
  });
  const toggle = () => {
    setOpen(!open);
    try {
      if (key) localStorage.setItem(key, open ? '0' : '1');
    } catch {
      /* ignore */
    }
  };
  return html`<section class=${cls('isec', !open && 'closed')}>
    <header class="isec-head">
      <button class="isec-title" onClick=${toggle}><${Icon} name=${open ? 'chevron-down' : 'chevron-right'} size=${14} />${title}</button>
      <div class="isec-actions">${actions}</div>
    </header>
    ${open && html`<div class="isec-body">${children}</div>`}
  </section>`;
}

export function Row({ label, children, hint, stack }) {
  return html`<div class=${cls('irow', stack && 'stack')}>
    ${label && html`<label class="irow-label" title=${hint}>${label}</label>`}
    <div class="irow-ctl">${children}</div>
  </div>`;
}

export function Hint({ children }) {
  return html`<p class="ihint">${children}</p>`;
}

/**
 * Save and Close first commit what is typed in the focused field: text
 * waiting for Enter, the field losing the focus or a debounce would
 * otherwise be left out (Ctrl+S and Ctrl+W work while typing). `flush` gets
 * the field's current text, read from the field itself.
 */
function useFlushWhenFocused(field, flush) {
  const latest = useRef(flush);
  latest.current = flush;
  useEffect(() => registerFlush(() => (field.current && document.activeElement === field.current ? latest.current(field.current.value) : undefined)), []);
}

/**
 * Number input; drag its label horizontally to scrub the value. A
 * `nullable` field can be emptied (`null`), showing its `placeholder`.
 */
export function NumberField({ value, onChange, min = -100000, max = 100000, step = 1, unit, label, title, width, disabled, precision, nullable, placeholder }) {
  const field = useRef(null);
  const [text, setText] = useState(fmt(value));
  const [focused, setFocused] = useState(false);
  // Escape leaves the field without committing what was typed.
  const cancel = useRef(false);
  // A click into the field selects its number; the click's mouseup would
  // otherwise put the caret back where the pointer is.
  const clicked = useRef(false);
  useEffect(() => {
    if (!focused) setText(fmt(value));
  }, [value, focused]);

  function fmt(v) {
    if (v === undefined || v === null || Number.isNaN(v)) return '';
    const p = precision ?? (step < 1 ? 2 : 0);
    return String(Math.round(v * 10 ** p) / 10 ** p);
  }

  const commit = (v, live = false) => {
    if (nullable && String(v ?? '').trim() === '') return value === null || value === undefined ? undefined : onChange(null, live);
    if (v === '' || v === null || Number.isNaN(Number(v))) return;
    const n = clamp(Number(v), min, max);
    if (n !== value) return onChange(n, live);
  };
  useFlushWhenFocused(field, (v) => commit(v));

  const onScrubStart = (e) => {
    if (disabled || e.button !== 0) return;
    e.preventDefault();
    const startX = e.clientX;
    const start = Number(value) || 0;
    let moved = false;
    const move = (ev) => {
      const dx = ev.clientX - startX;
      if (Math.abs(dx) > 2) moved = true;
      if (!moved) return;
      // One step per 3px; Shift = ×10, Alt = fine (÷10).
      const unit = step * (ev.shiftKey ? 10 : 1) * (ev.altKey ? 0.1 : 1);
      const n = clamp(Math.round((start + Math.round(dx / 3) * unit) * 1000) / 1000, min, max);
      onChange(n, true);
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      document.body.classList.remove('scrubbing');
    };
    document.body.classList.add('scrubbing');
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  return html`<div class=${cls('numfield', disabled && 'disabled')} style=${width ? { width: width + 'px' } : null} title=${title}>
    ${label && html`<span class="numfield-label" onPointerDown=${onScrubStart}>${label}</span>`}
    <input
      ref=${field}
      type="text"
      inputmode="decimal"
      value=${text}
      placeholder=${placeholder}
      disabled=${disabled}
      onMouseDown=${(e) => {
        clicked.current = document.activeElement !== e.target;
      }}
      onMouseUp=${(e) => {
        if (clicked.current) e.preventDefault();
        clicked.current = false;
      }}
      onFocus=${(e) => {
        cancel.current = false;
        setFocused(true);
        e.target.select();
      }}
      onBlur=${(e) => {
        setFocused(false);
        if (!cancel.current) commit(e.target.value);
        cancel.current = false;
      }}
      onInput=${(e) => setText(e.target.value)}
      onKeyDown=${(e) => {
        if (e.key === 'Enter') {
          // Committing may put the focus back in the text being edited, with
          // its selection: the Enter must not land there and replace it.
          e.preventDefault();
          e.target.blur();
        } else if (e.key === 'Escape') {
          cancel.current = true;
          setText(fmt(value));
          e.target.blur();
        } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
          e.preventDefault();
          const d = (e.key === 'ArrowUp' ? 1 : -1) * step * (e.shiftKey ? 10 : 1);
          const n = clamp((Number(text) || 0) + d, min, max);
          setText(fmt(n));
          onChange(n, true);
        }
      }}
    />
    ${unit && html`<span class="numfield-unit">${unit}</span>`}
  </div>`;
}

export function Slider({ value, onChange, min = 0, max = 100, step = 1, unit = '', format }) {
  return html`<div class="slider">
    <input type="range" min=${min} max=${max} step=${step} value=${value} onInput=${(e) => onChange(Number(e.target.value), true)} />
    <${NumberField} value=${value} min=${min} max=${max} step=${step} unit=${unit} width=${format || unit.length > 1 ? 80 : 64} onChange=${onChange} precision=${format} />
  </div>`;
}

export function Select({ value, options, onChange, title, style }) {
  return html`<div class="select" title=${title} style=${style}>
    <select value=${value} onChange=${(e) => onChange(e.target.value)}>
      ${options.map((o) =>
        o.group
          ? html`<optgroup label=${o.group}>${o.options.map((x) => html`<option value=${x.value}>${x.label}</option>`)}</optgroup>`
          : html`<option value=${o.value}>${o.label}</option>`,
      )}
    </select>
    <${Icon} name="chevron-down" size=${14} />
  </div>`;
}

export function Segmented({ value, options, onChange, full }) {
  return html`<div class=${cls('segmented', full && 'full')}>
    ${options.map(
      (o) => html`<button
        type="button"
        class=${cls((o.active ?? value === o.value) && 'active')}
        title=${o.title || o.label}
        onMouseDown=${(e) => o.keepFocus && e.preventDefault()}
        onClick=${() => onChange(o.value)}
      >
        ${o.icon ? html`<${Icon} name=${o.icon} size=${16} />` : html`<span>${o.label}</span>`}
      </button>`,
    )}
  </div>`;
}

export function Toggle({ value, onChange, label, hint }) {
  return html`<label class="toggle" title=${hint}>
    <input type="checkbox" checked=${!!value} onChange=${(e) => onChange(e.target.checked)} />
    <span class="toggle-track"><span class="toggle-thumb" /></span>
    ${label && html`<span class="toggle-label">${label}</span>`}
  </label>`;
}

/**
 * Text input that commits on Enter/blur (or live for multiline fields).
 * Escape leaves a one-line field without committing; in a multiline field
 * (code, LaTeX) it keeps what was typed.
 */
export function TextField({ value, onChange, placeholder, multiline, rows = 4, mono, live, spellcheck = false }) {
  const [text, setText] = useState(value ?? '');
  const [focused, setFocused] = useState(false);
  const timer = useRef(null);
  const field = useRef(null);
  const cancel = useRef(false);
  useEffect(() => {
    if (!focused) setText(value ?? '');
  }, [value, focused]);
  const flush = (v) => {
    clearTimeout(timer.current);
    if (v !== value) return onChange(v);
  };
  useFlushWhenFocused(field, flush);
  const props = {
    ref: field,
    value: text,
    placeholder,
    spellcheck,
    class: cls('textfield', mono && 'mono'),
    onFocus: () => {
      cancel.current = false;
      setFocused(true);
    },
    onBlur: (e) => {
      setFocused(false);
      if (!cancel.current) flush(e.target.value);
      cancel.current = false;
    },
    onInput: (e) => {
      setText(e.target.value);
      if (live) {
        clearTimeout(timer.current);
        const v = e.target.value;
        timer.current = setTimeout(() => onChange(v, true), 350);
      }
    },
    onKeyDown: (e) => {
      if (!multiline && e.key === 'Enter') {
        e.preventDefault();
        e.target.blur();
      }
      if (e.key === 'Escape') {
        if (!multiline) {
          cancel.current = true;
          setText(value ?? '');
        }
        e.target.blur();
      }
      if (multiline && e.key === 'Tab' && mono) {
        e.preventDefault();
        const field = e.target;
        const s = field.selectionStart;
        const v = text.slice(0, s) + '  ' + text.slice(field.selectionEnd);
        setText(v);
        requestAnimationFrame(() => field.setSelectionRange(s + 2, s + 2));
      }
    },
  };
  return multiline ? html`<textarea rows=${rows} ...${props} />` : html`<input type="text" ...${props} />`;
}

/**
 * A cell of a data grid. It commits when its text changes (Enter, leaving
 * it) and when Save or Close run while it is being typed in; while it has
 * the focus, other updates leave what is typed alone.
 */
/** A cell of a data grid; a `nullable` number cell can be left empty (`null`). */
export function Cell({ value, onChange, number, nullable, class: klass, list }) {
  const field = useRef(null);
  const shown = (v) => (v === null || v === undefined ? '' : String(v));
  const [text, setText] = useState(shown(value));
  const [focused, setFocused] = useState(false);
  useEffect(() => {
    if (!focused) setText(shown(value));
  }, [value, focused]);
  const commit = (v) => {
    const next = number ? (nullable && String(v).trim() === '' ? null : Number(v) || 0) : v;
    if (next !== (value ?? null)) return onChange(next);
  };
  useFlushWhenFocused(field, commit);
  return html`<input
    ref=${field}
    class=${klass}
    type=${number ? 'number' : 'text'}
    list=${list}
    value=${text}
    onFocus=${() => setFocused(true)}
    onBlur=${() => setFocused(false)}
    onInput=${(e) => setText(e.target.value)}
    onChange=${(e) => commit(e.target.value)}
  />`;
}

// ---------------------------------------------------------------------------
// Popover
// ---------------------------------------------------------------------------

export function Popover({ anchor, onClose, children, width = 248, class: extra }) {
  const ref = useRef();
  const [pos, setPos] = useState(null);
  useLayoutEffect(() => {
    const a = anchor.getBoundingClientRect();
    const h = ref.current?.offsetHeight || 300;
    let left = Math.min(a.left, innerWidth - width - 8);
    let top = a.bottom + 6;
    if (top + h > innerHeight - 8) top = Math.max(8, a.top - h - 6);
    setPos({ left: Math.max(8, left), top });
  }, [anchor]);
  useEffect(() => {
    const down = (e) => {
      if (ref.current && !ref.current.contains(e.target) && !anchor.contains(e.target)) onClose();
    };
    const key = (e) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener('mousedown', down, true);
    window.addEventListener('keydown', key, true);
    return () => {
      window.removeEventListener('mousedown', down, true);
      window.removeEventListener('keydown', key, true);
    };
  }, [anchor]);
  return html`<div
    class=${cls('popover', extra)}
    ref=${ref}
    style=${{ width: width + 'px', left: (pos?.left ?? -9999) + 'px', top: (pos?.top ?? -9999) + 'px' }}
  >
    ${children}
  </div>`;
}

// ---------------------------------------------------------------------------
// Colors
// ---------------------------------------------------------------------------

const PALETTE = [
  '#ffffff', '#e5e7eb', '#9ca3af', '#6b7280', '#374151', '#1f2937', '#111827', '#000000',
  '#fecaca', '#fed7aa', '#fef08a', '#bbf7d0', '#a5f3fc', '#bfdbfe', '#ddd6fe', '#fbcfe8',
  '#ef4444', '#f97316', '#eab308', '#22c55e', '#06b6d4', '#3b82f6', '#8b5cf6', '#ec4899',
  '#b91c1c', '#c2410c', '#a16207', '#15803d', '#0e7490', '#1d4ed8', '#6d28d9', '#be185d',
];

const THEME_COLORS = [
  ['var(--cz-accent)', N_('Theme accent')],
  ['var(--cz-heading)', N_('Theme heading')],
  ['var(--cz-text)', N_('Theme text')],
  ['var(--cz-chart-1)', N_('Theme color 1')],
  ['var(--cz-chart-2)', N_('Theme color 2')],
  ['var(--cz-chart-3)', N_('Theme color 3')],
  ['var(--cz-chart-4)', N_('Theme color 4')],
  ['var(--cz-chart-5)', N_('Theme color 5')],
];

/** Parses CSS colors (hex, rgb[a], var()) into rgba components. */
export function parseColor(c) {
  if (!c) return null;
  let s = c.trim();
  const m = s.match(/^var\((--[\w-]+)\)$/);
  if (m) s = getComputedStyle(document.documentElement).getPropertyValue(m[1]).trim();
  if (s.startsWith('#')) {
    let h = s.slice(1);
    if (h.length === 3 || h.length === 4) h = h.split('').map((x) => x + x).join('');
    if (h.length !== 6 && h.length !== 8) return null;
    const n = (i) => parseInt(h.slice(i, i + 2), 16);
    return { r: n(0), g: n(2), b: n(4), a: h.length === 8 ? n(6) / 255 : 1 };
  }
  const r = s.match(/rgba?\(([^)]+)\)/i);
  if (r) {
    const p = r[1].split(/[\s,/]+/).filter(Boolean).map(Number);
    return { r: p[0], g: p[1], b: p[2], a: p[3] ?? 1 };
  }
  return null;
}

const hex2 = (n) => clamp(Math.round(n), 0, 255).toString(16).padStart(2, '0');
export const toHex = (c) => `#${hex2(c.r)}${hex2(c.g)}${hex2(c.b)}`;
export const toCss = (c) => (c.a >= 0.999 ? toHex(c) : `rgba(${Math.round(c.r)},${Math.round(c.g)},${Math.round(c.b)},${Math.round(c.a * 100) / 100})`);

function recentColors() {
  try {
    return JSON.parse(localStorage.getItem('cz-recent-colors') || '[]');
  } catch {
    return [];
  }
}

function pushRecent(c) {
  if (!c || c.startsWith('var(')) return;
  try {
    const list = [c, ...recentColors().filter((x) => x !== c)].slice(0, 8);
    localStorage.setItem('cz-recent-colors', JSON.stringify(list));
  } catch {
    /* ignore */
  }
}

function ColorPanel({ value, onChange, allowNone, noneLabel }) {
  const parsed = parseColor(value) || { r: 99, g: 102, b: 241, a: 1 };
  const [hex, setHex] = useState(value && !value.startsWith('var(') ? toHex(parsed) : '');
  const recent = useMemo(recentColors, []);
  const pick = (c, live = false) => {
    onChange(c, live);
    if (!live) pushRecent(c);
  };
  const sw = (c, title) => html`<button
    type="button"
    class=${cls('swatch', value === c && 'active')}
    title=${title || c}
    style=${{ '--c': c }}
    onClick=${() => pick(c)}
  />`;
  return html`<div class="colorpanel">
    ${allowNone &&
    html`<button type="button" class=${cls('color-none', !value && 'active')} onClick=${() => pick('')}>
      <span class="swatch none" />${noneLabel || t('Default (from theme)')}
    </button>`}
    <div class="color-label">${t('Theme')}</div>
    <div class="swatches">${THEME_COLORS.map(([c, name]) => sw(c, t(name)))}</div>
    <div class="color-label">${t('Palette')}</div>
    <div class="swatches">${PALETTE.map((c) => sw(c))}</div>
    ${recent.length > 0 && html`<div class="color-label">${t('Recent')}</div><div class="swatches">${recent.map((c) => sw(c))}</div>`}
    <div class="color-custom">
      <label class="native-color" title=${t('Pick any color')}>
        <input
          type="color"
          value=${toHex(parsed)}
          onInput=${(e) => {
            const c = parseColor(e.target.value);
            c.a = parsed.a;
            setHex(e.target.value);
            pick(toCss(c), true);
          }}
          onChange=${(e) => {
            const c = parseColor(e.target.value);
            c.a = parsed.a;
            pushRecent(toCss(c));
          }}
        />
        <span class="swatch" style=${{ '--c': value || 'transparent' }} />
      </label>
      <input
        class="textfield mono"
        value=${hex}
        placeholder="#hex"
        onInput=${(e) => setHex(e.target.value)}
        onKeyDown=${(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            const c = parseColor(hex.startsWith('#') ? hex : '#' + hex);
            if (c) pick(toCss({ ...c, a: c.a < 1 ? c.a : parsed.a }));
          }
        }}
      />
    </div>
    <div class="color-alpha">
      <span>${t('Opacity')}</span>
      <input
        type="range"
        min="0"
        max="100"
        value=${Math.round(parsed.a * 100)}
        onInput=${(e) => pick(toCss({ ...parsed, a: Number(e.target.value) / 100 }), true)}
        onChange=${(e) => pushRecent(toCss({ ...parsed, a: Number(e.target.value) / 100 }))}
      />
      <span class="alpha-val">${Math.round(parsed.a * 100)}%</span>
    </div>
  </div>`;
}

export function ColorField({ value, onChange, allowNone = true, noneLabel, label }) {
  const ref = useRef();
  const [open, setOpen] = useState(false);
  return html`<div class="colorfield">
    <button type="button" class="colorbtn" ref=${ref} onClick=${() => setOpen(!open)} title=${value || noneLabel || t('Default')}>
      <span class=${cls('swatch', !value && 'none')} style=${{ '--c': value || 'transparent' }} />
      <span class="colorbtn-label">${label || describeColor(value, noneLabel)}</span>
    </button>
    ${open &&
    html`<${Popover} anchor=${ref.current} onClose=${() => setOpen(false)}>
      <${ColorPanel} value=${value} onChange=${onChange} allowNone=${allowNone} noneLabel=${noneLabel} />
    <//>`}
  </div>`;
}

function describeColor(v, noneLabel) {
  if (!v) return noneLabel || t('Default');
  const theme = THEME_COLORS.find(([c]) => c === v);
  if (theme) return t(theme[1]);
  const c = parseColor(v);
  if (!c) return v;
  return c.a < 1 ? `${toHex(c)} · ${Math.round(c.a * 100)}%` : toHex(c);
}

// ---------------------------------------------------------------------------
// Fonts
// ---------------------------------------------------------------------------

const CATEGORY = { s: N_('Sans'), f: N_('Serif'), d: N_('Display'), h: N_('Handwriting'), m: N_('Mono') };
let catalog = null;
let catalogPromise = null;

/** The bundled Google Fonts catalog: [[family, category, variants], …]. */
export function googleCatalog() {
  if (!catalogPromise) {
    catalogPromise = fetch('data/google-fonts.json')
      .then((r) => r.json())
      .then((rows) => (catalog = rows))
      .catch(() => (catalog = []));
  }
  return catalogPromise;
}

// Lightweight previews: only the glyphs of the family name, loaded lazily
// under a private family name so they never shadow the real (embedded) font.
const previewed = new Set();
const previewName = (family) => `cz-preview-${family}`;
function loadPreview(family) {
  if (previewed.has(family) || !navigator.onLine) return;
  previewed.add(family);
  const url = `https://fonts.googleapis.com/css2?family=${encodeURIComponent(family)}&text=${encodeURIComponent(family)}&display=swap`;
  fetch(url)
    .then((r) => (r.ok ? r.text() : ''))
    .then((css) => {
      if (!css) return;
      const style = document.createElement('style');
      style.textContent = css.replace(/font-family:\s*['"][^'"]+['"]/g, `font-family: '${previewName(family)}'`);
      document.head.appendChild(style);
    })
    .catch(() => {});
}

const previewObserver =
  'IntersectionObserver' in window
    ? new IntersectionObserver((entries) => {
        for (const e of entries) {
          if (e.isIntersecting) {
            loadPreview(e.target.dataset.family);
            previewObserver.unobserve(e.target);
          }
        }
      })
    : null;

const familyOf = (v) => (v || '').split(',')[0].replace(/['"]/g, '').trim();

/** Installs a Google font into the open presentation (downloads it once). */
export async function installGoogleFont(family, variants) {
  const { invoke } = await import('./api.js');
  const { ingestView } = await import('./store.js');
  ingestView(await invoke('font_install', { family, variants }), { navigate: false });
}

export function FontField({ value, onChange, placeholder: ph }) {
  const placeholder = ph ?? t('Theme font');
  const ref = useRef();
  const [open, setOpen] = useState(false);
  return html`<div class="fontfield">
    <button type="button" class="selectbtn" ref=${ref} onClick=${() => setOpen(!open)}>
      <span style=${{ fontFamily: value || 'inherit' }}>${familyOf(value) || placeholder}</span>
      <${Icon} name="chevron-down" size=${14} />
    </button>
    ${open &&
    html`<${Popover} anchor=${ref.current} onClose=${() => setOpen(false)} width=${320} class="fontpop">
      <${FontPicker}
        value=${value}
        placeholder=${placeholder}
        onPick=${(v) => {
          onChange(v);
          setOpen(false);
        }}
      />
    <//>`}
  </div>`;
}

function FontPicker({ value, onPick, placeholder }) {
  const [q, setQ] = useState('');
  const [tab, setTab] = useState('google');
  const [cat, setCat] = useState('');
  const [rows, setRows] = useState(catalog);
  const [busy, setBusy] = useState(null);
  const [error, setError] = useState(null);
  const listRef = useRef();
  useEffect(() => {
    if (!rows) googleCatalog().then(setRows);
  }, []);
  const embedded = (state.doc?.deck.webFonts || []).map((f) => f.family);
  const system = state.fonts || [];
  const current = familyOf(value);
  const s = q.trim().toLowerCase();

  const list = useMemo(() => {
    if (tab === 'google') {
      return (rows || []).filter((r) => (!cat || r[1] === cat) && (!s || r[0].toLowerCase().includes(s))).slice(0, 250);
    }
    return [...new Set([...system, ...WEB_SAFE])].sort((a, b) => a.localeCompare(b)).filter((f) => !s || f.toLowerCase().includes(s)).slice(0, 400);
  }, [tab, rows, cat, s, system]);

  // Observe rows for lazy previews.
  useLayoutEffect(() => {
    if (tab !== 'google' || !previewObserver || !listRef.current) return;
    for (const el of listRef.current.querySelectorAll('[data-family]')) previewObserver.observe(el);
  }, [list, tab]);

  const pickGoogle = async (r) => {
    setError(null);
    if (embedded.includes(r[0])) return onPick(`'${r[0]}'`);
    setBusy(r[0]);
    try {
      await installGoogleFont(r[0], r[2]);
      onPick(`'${r[0]}'`);
    } catch (e) {
      setError(t(String(e)));
    } finally {
      setBusy(null);
    }
  };

  return html`<div class="fontpicker">
    <input class="textfield" placeholder=${t('Search fonts…')} value=${q} onInput=${(e) => setQ(e.target.value)} ref=${(el) => el && !el.dataset.f && ((el.dataset.f = 1), setTimeout(() => el.focus()))} />
    <div class="font-tabs">
      <button type="button" class=${cls(tab === 'google' && 'active')} onClick=${() => setTab('google')}>Google Fonts</button>
      <button type="button" class=${cls(tab === 'system' && 'active')} onClick=${() => setTab('system')}>${t('On this computer')}</button>
    </div>
    ${tab === 'google' &&
    html`<div class="font-cats">
      ${[['', N_('All')], ...Object.entries(CATEGORY)].map(
        ([k, label]) => html`<button type="button" class=${cls(cat === k && 'active')} onClick=${() => setCat(k)}>${t(label)}</button>`,
      )}
    </div>`}
    ${error && html`<div class="font-error">${error}</div>`}
    <div class="fontlist" ref=${listRef}>
      <button type="button" class=${cls('fontopt', !value && 'active')} onClick=${() => onPick('')}>${placeholder}</button>
      ${embedded.length > 0 &&
      !s &&
      html`<div class="font-group">${t('In this presentation')}</div>
        ${embedded.map(
          (f) => html`<button type="button" class=${cls('fontopt', current === f && 'active')} style=${{ fontFamily: `'${f}'` }} onClick=${() => onPick(`'${f}'`)}>
            ${f}<${Icon} name="check" size=${13} class="font-embedded" />
          </button>`,
        )}
        <div class="font-group">${tab === 'google' ? 'Google Fonts' : t('System fonts')}</div>`}
      ${tab === 'google'
        ? !rows
          ? html`<div class="ihint">${t('Loading catalog…')}</div>`
          : list.map(
              (r) => html`<button
                type="button"
                data-family=${r[0]}
                class=${cls('fontopt', current === r[0] && 'active')}
                style=${{ fontFamily: `'${previewName(r[0])}', '${r[0]}', var(--font)` }}
                disabled=${!!busy}
                onClick=${() => pickGoogle(r)}
              >
                <span class="fo-name">${r[0]}</span>
                ${busy === r[0]
                  ? html`<span class="fo-meta"><span class="spinner small" /> ${t('Downloading…')}</span>`
                  : html`<span class="fo-meta">${embedded.includes(r[0]) ? t('embedded') : t(CATEGORY[r[1]])}</span>`}
              </button>`,
            )
        : list.map(
            (f) => html`<button type="button" class=${cls('fontopt', current === f && 'active')} style=${{ fontFamily: `'${f}'` }} onClick=${() => onPick(`'${f}'`)}>
              ${f}
            </button>`,
          )}
      ${tab === 'system' && !system.length && html`<div class="ihint">${t('Loading system fonts…')}</div>`}
    </div>
    <p class="ihint">${tab === 'google'
      ? t('Google fonts are downloaded once and embedded in the presentation, so they work offline and in exports.')
      : t('System fonts only display correctly on computers that have them installed.')}</p>
  </div>`;
}

const WEB_SAFE = ['Georgia', 'Times New Roman', 'Arial', 'Helvetica', 'Verdana', 'Courier New', 'Trebuchet MS'];
