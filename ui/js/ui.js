// Generic UI building blocks: buttons, menus, toasts and modal frames.

import { html, useEffect, useLayoutEffect, useRef, useState } from './lib.js';
import { Icon } from './icons.js';
import { state, set, openMenu, closeMenu, useStore } from './store.js';
import { cls, keyLabel } from './util.js';
import { t } from './i18n.js';

export function Button({ icon, label, onClick, variant = '', title, disabled, active, size, class: extra, children, onMouseDown }) {
  return html`<button
    type="button"
    class=${cls('btn', variant && `btn-${variant}`, size && `btn-${size}`, active && 'active', !label && !children && icon && 'btn-icon', extra)}
    title=${title}
    disabled=${disabled}
    onClick=${onClick}
    onMouseDown=${onMouseDown}
  >
    ${icon && html`<${Icon} name=${icon} size=${size === 'sm' ? 15 : 17} />`}
    ${label && html`<span>${label}</span>`} ${children}
  </button>`;
}

/** A toolbar button with an icon above a small label (Keynote-style). */
export function ToolButton({ icon, label, onClick, title, disabled, active, caret, onMouseDown }) {
  return html`<button
    type="button"
    class=${cls('tool', active && 'active')}
    title=${title || label}
    disabled=${disabled}
    onClick=${onClick}
    onMouseDown=${onMouseDown}
  >
    <span class="tool-icon"><${Icon} name=${icon} size=${20} />${caret && html`<${Icon} name="chevron-down" size=${11} class="caret" />`}</span>
    <span class="tool-label">${label}</span>
  </button>`;
}

/** Opens a menu below an anchor element. `items` may be a function. */
export function showMenuAt(anchor, items, align = 'left') {
  const r = anchor.getBoundingClientRect();
  openMenu(align === 'right' ? r.right : r.left, r.bottom + 4, typeof items === 'function' ? items() : items);
  if (align === 'right') set({ menu: { ...state.menu, alignRight: true } });
}

// ---------------------------------------------------------------------------
// Context / dropdown menu
// ---------------------------------------------------------------------------

function MenuList({ items, x, y, alignRight, flipX, onDone, depth = 0 }) {
  const ref = useRef();
  const [pos, setPos] = useState({ left: x, top: y, ready: false });
  const [active, setActive] = useState(-1);
  const [sub, setSub] = useState(null);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    let left = alignRight ? x - r.width : x;
    let top = y;
    if (left + r.width > innerWidth - 6) left = flipX !== undefined ? flipX - r.width : innerWidth - r.width - 6;
    if (top + r.height > innerHeight - 6) top = Math.max(6, innerHeight - r.height - 6);
    setPos({ left: Math.max(6, left), top, ready: true });
  }, [x, y]);

  const selectable = items.map((it, i) => (it && it !== '-' && !it.heading && !it.disabled ? i : -1)).filter((i) => i >= 0);

  useEffect(() => {
    if (depth) return;
    const onKey = (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        onDone();
      } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        e.stopPropagation();
        const cur = selectable.indexOf(active);
        const next = e.key === 'ArrowDown' ? cur + 1 : cur - 1;
        setActive(selectable[(next + selectable.length) % selectable.length]);
      } else if (e.key === 'Enter' && active >= 0) {
        e.preventDefault();
        e.stopPropagation();
        const it = items[active];
        if (it.children) setSub(active);
        else {
          onDone();
          it.run?.();
        }
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [active, items]);

  return html`<div
    class=${cls('menu', pos.ready && 'ready')}
    ref=${ref}
    style=${{ left: pos.left + 'px', top: pos.top + 'px' }}
    onMouseDown=${(e) => e.stopPropagation()}
    onContextMenu=${(e) => e.preventDefault()}
  >
    ${items.map((it, i) => {
      if (!it) return null;
      if (it === '-') return html`<div class="menu-sep" />`;
      if (it.heading) return html`<div class="menu-heading">${it.heading}</div>`;
      const open = sub === i;
      return html`<div
        class=${cls('menu-item', it.disabled && 'disabled', active === i && 'active', it.danger && 'danger')}
        onMouseEnter=${() => {
          setActive(i);
          setSub(it.children ? i : null);
        }}
        onClick=${(e) => {
          e.stopPropagation();
          if (it.disabled) return;
          if (it.children) {
            setSub(i);
            return;
          }
          onDone();
          it.run?.(e);
        }}
      >
        <span class="menu-check">${it.checked ? html`<${Icon} name="check" size=${14} />` : it.icon ? html`<${Icon} name=${it.icon} size=${15} />` : null}</span>
        <span class="menu-label" style=${it.style}>${it.label}</span>
        ${it.keys && html`<span class="menu-keys">${keyLabel(it.keys)}</span>`}
        ${it.children && html`<${Icon} name="chevron-right" size=${14} class="menu-sub" />`}
        ${open && it.children && html`<${SubMenu} items=${it.children} parent=${i} onDone=${onDone} depth=${depth + 1} />`}
      </div>`;
    })}
  </div>`;
}

function SubMenu({ items, onDone, depth }) {
  const ref = useRef();
  const [p, setP] = useState(null);
  useLayoutEffect(() => {
    const host = ref.current?.parentElement;
    if (!host) return;
    const r = host.getBoundingClientRect();
    setP({ x: r.right - 4, y: r.top - 5, flipX: r.left + 4 });
  }, []);
  return html`<span ref=${ref} class="submenu-anchor">
    ${p && html`<${MenuList} items=${items} x=${p.x} y=${p.y} flipX=${p.flipX} onDone=${onDone} depth=${depth} />`}
  </span>`;
}

/** Renders the open menu from `state.menu`. */
export function MenuHost() {
  const st = useStore();
  useEffect(() => {
    if (!st.menu) return;
    const close = () => closeMenu();
    const timer = setTimeout(() => {
      window.addEventListener('mousedown', close);
      window.addEventListener('blur', close);
      window.addEventListener('resize', close);
    });
    return () => {
      clearTimeout(timer);
      window.removeEventListener('mousedown', close);
      window.removeEventListener('blur', close);
      window.removeEventListener('resize', close);
    };
  }, [st.menu]);
  if (!st.menu) return null;
  return html`<${MenuList} key=${st.menu.x + ':' + st.menu.y} items=${st.menu.items} x=${st.menu.x} y=${st.menu.y} alignRight=${st.menu.alignRight} onDone=${closeMenu} />`;
}

// ---------------------------------------------------------------------------
// Toasts
// ---------------------------------------------------------------------------

export function Toasts() {
  const st = useStore();
  return html`<div class="toasts">
    ${st.toasts.map(
      (toast) => html`<div key=${toast.id} class=${cls('toast', `toast-${toast.kind}`)}>
        <${Icon} name=${toast.kind === 'error' ? 'info' : 'check'} size=${16} />
        <span>${toast.message}</span>
        ${toast.action &&
        html`<button
          class="toast-action"
          onClick=${() => {
            toast.action.run();
            set({ toasts: state.toasts.filter((x) => x.id !== toast.id) });
          }}
        >
          ${toast.action.label}
        </button>`}
      </div>`,
    )}
  </div>`;
}

// ---------------------------------------------------------------------------
// Modal frame
// ---------------------------------------------------------------------------

export function Modal({ title, onClose, children, footer, width = 520, class: extra }) {
  useEffect(() => {
    const onKey = (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        onClose?.();
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [onClose]);
  return html`<div class="modal-backdrop" onMouseDown=${(e) => e.target === e.currentTarget && onClose?.()}>
    <div class=${cls('modal', extra)} style=${{ width: width + 'px' }} role="dialog" aria-modal="true">
      ${title &&
      html`<div class="modal-head">
        <h2>${title}</h2>
        ${onClose && html`<${Button} icon="x" variant="ghost" title=${t('Close')} onClick=${onClose} />`}
      </div>`}
      <div class="modal-body">${children}</div>
      ${footer && html`<div class="modal-foot">${footer}</div>`}
    </div>
  </div>`;
}

/** Small "busy" overlay for long operations. */
export function Busy() {
  const st = useStore();
  if (!st.busy) return null;
  return html`<div class="busy"><div class="spinner" />${st.busy}</div>`;
}
