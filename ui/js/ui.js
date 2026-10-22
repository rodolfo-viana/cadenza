// Generic UI building blocks: buttons, menus, toasts and modal frames.

import { html, useEffect, useLayoutEffect, useRef, useState } from './lib.js';
import { Icon } from './icons.js';
import { state, set, openMenu, closeMenu, useStore } from './store.js';
import { cls, keyLabel } from './util.js';
import { t, isRtl } from './i18n.js';

export function Button({ icon, label, onClick, variant = '', title, disabled, active, size, class: extra, children, onMouseDown, autofocus }) {
  return html`<button
    type="button"
    class=${cls('btn', variant && `btn-${variant}`, size && `btn-${size}`, active && 'active', !label && !children && icon && 'btn-icon', extra)}
    title=${title}
    disabled=${disabled}
    autofocus=${autofocus}
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

let menuIds = 0;

/**
 * One level of a menu. The keyboard works in one level at a time, the
 * deepest it entered: Up/Down move, Enter/Space run, Right opens a submenu
 * and Left or Escape go back to its parent item (mirrored in RTL, where
 * submenus open to the left).
 */
function MenuList({ items, x, y, alignRight, flipX, onDone, depth = 0, keyboard = true, onKeyboard, onBack, labelledBy, nav: parentNav }) {
  const ref = useRef();
  const els = useRef([]);
  const [id] = useState(() => `cz-menu-${++menuIds}`);
  // Shared by the levels of one menu: where the focus was and whether keys were used.
  const [ownNav] = useState(() => ({ before: document.activeElement, keyed: false }));
  const nav = parentNav || ownNav;
  const [pos, setPos] = useState({ left: x, top: y, ready: false });
  const [active, setActive] = useState(-1);
  const [sub, setSub] = useState(null);
  // Whether the keyboard is in the open submenu rather than here.
  const [inSub, setInSub] = useState(false);
  const mine = keyboard && !inSub;

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    let left = alignRight ? x - r.width : x;
    let top = y;
    // Without room on its side, a submenu opens on the other side of its item.
    if (left + r.width > innerWidth - 6) left = flipX !== undefined ? flipX - r.width : innerWidth - r.width - 6;
    else if (left < 6 && flipX !== undefined) left = flipX;
    if (top + r.height > innerHeight - 6) top = Math.max(6, innerHeight - r.height - 6);
    setPos({ left: Math.max(6, left), top, ready: true });
  }, [x, y]);

  const selectable = items.map((it, i) => (it && it !== '-' && !it.heading && !it.disabled ? i : -1)).filter((i) => i >= 0);

  // A submenu entered from the keyboard starts at its first item.
  useEffect(() => {
    if (depth && keyboard && active < 0) setActive(selectable[0] ?? -1);
  }, [keyboard]);

  // Once keys are used, the focus follows the active item, for screen readers.
  useEffect(() => {
    if (mine && nav.keyed && active >= 0) els.current[active]?.focus();
  }, [active, mine]);

  // A menu used from the keyboard gives the focus back when it closes.
  useEffect(() => {
    if (depth) return;
    return () => {
      const at = document.activeElement;
      if (nav.keyed && nav.before?.isConnected && (!at || at === document.body || ref.current?.contains(at))) nav.before.focus();
    };
  }, []);

  const openSub = (i, keys) => {
    setSub(i);
    setInSub(keys);
  };

  useEffect(() => {
    if (!mine) return;
    const [forward, back] = isRtl() ? ['ArrowLeft', 'ArrowRight'] : ['ArrowRight', 'ArrowLeft'];
    const onKey = (e) => {
      const it = items[active];
      const run = e.key === 'Enter' || e.key === ' ';
      if (e.key === 'Escape' || (depth && e.key === back)) {
        if (depth) onBack();
        else onDone();
      } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        // From no active item, Down goes to the first and Up to the last.
        const n = selectable.length;
        const cur = selectable.indexOf(active);
        const next = e.key === 'ArrowDown' ? cur + 1 : cur < 0 ? n - 1 : cur - 1;
        if (n) setActive(selectable[(next + n) % n]);
        openSub(null, false);
      } else if (it && !it.disabled && it.children && (run || e.key === forward)) {
        openSub(active, true);
      } else if (it && !it.disabled && !it.children && run) {
        onDone();
        it.run?.();
      } else return;
      e.preventDefault();
      e.stopPropagation();
      nav.keyed = true;
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [mine, active, items]);

  return html`<div
    class=${cls('menu', pos.ready && 'ready')}
    ref=${ref}
    role="menu"
    aria-labelledby=${labelledBy}
    style=${{ left: pos.left + 'px', top: pos.top + 'px' }}
    onMouseDown=${(e) => e.stopPropagation()}
    onContextMenu=${(e) => e.preventDefault()}
  >
    ${items.map((it, i) => {
      if (!it) return null;
      if (it === '-') return html`<div class="menu-sep" role="separator" />`;
      if (it.heading) return html`<div class="menu-heading">${it.heading}</div>`;
      const open = sub === i;
      const labelId = `${id}-${i}`;
      const check = 'checked' in it;
      return html`<div
        ref=${(el) => (els.current[i] = el)}
        class=${cls('menu-item', it.disabled && 'disabled', active === i && 'active', it.danger && 'danger')}
        role=${check ? 'menuitemcheckbox' : 'menuitem'}
        tabindex="-1"
        aria-checked=${check ? !!it.checked : undefined}
        aria-disabled=${it.disabled ? 'true' : undefined}
        aria-haspopup=${it.children ? 'menu' : undefined}
        aria-expanded=${it.children ? open : undefined}
        aria-labelledby=${it.children ? labelId : undefined}
        onMouseEnter=${() => {
          // Disabled items never become active, by any route.
          setActive(it.disabled ? -1 : i);
          openSub(it.children && !it.disabled ? i : null, false);
          onKeyboard?.();
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
        <span class="menu-label" id=${labelId} style=${it.style}>${it.label}</span>
        ${it.keys && html`<span class="menu-keys" dir="ltr">${keyLabel(it.keys)}</span>`}
        ${it.children && html`<${Icon} name="chevron-right" size=${14} class="menu-sub" />`}
        ${open &&
        it.children &&
        html`<${SubMenu}
          items=${it.children}
          onDone=${onDone}
          depth=${depth + 1}
          keyboard=${keyboard && inSub}
          onKeyboard=${() => {
            setInSub(true);
            onKeyboard?.();
          }}
          onBack=${() => openSub(null, false)}
          labelledBy=${labelId}
          nav=${nav}
        />`}
      </div>`;
    })}
  </div>`;
}

function SubMenu(props) {
  const ref = useRef();
  const [p, setP] = useState(null);
  useLayoutEffect(() => {
    const host = ref.current?.parentElement;
    if (!host) return;
    const r = host.getBoundingClientRect();
    // On the side the (mirrored) chevron points to.
    setP(isRtl() ? { x: r.left + 4, y: r.top - 5, flipX: r.right - 4, alignRight: true } : { x: r.right - 4, y: r.top - 5, flipX: r.left + 4 });
  }, []);
  return html`<span ref=${ref} class="submenu-anchor">
    ${p && html`<${MenuList} ...${props} x=${p.x} y=${p.y} flipX=${p.flipX} alignRight=${p.alignRight} />`}
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

let modalIds = 0;

/** What Tab reaches inside `root`, in order. */
function tabbable(root) {
  if (!root) return [];
  return [...root.querySelectorAll('a[href], button, input, select, textarea, [tabindex], [contenteditable]')].filter(
    (el) => el.tabIndex >= 0 && !el.disabled && el.getClientRects().length > 0,
  );
}

/**
 * A dialog that keeps the keyboard inside: the first field (or an
 * `autofocus` one) gets the focus unless the caller focuses one itself, Tab
 * wraps around, and the focus goes back where it was on closing.
 */
export function Modal({ title, onClose, children, footer, width = 520, class: extra }) {
  const ref = useRef();
  const [titleId] = useState(() => `cz-modal-${++modalIds}`);
  const [before] = useState(() => document.activeElement);

  useEffect(() => {
    const dialog = ref.current;
    // What the dialog covers is out of reach of the keyboard and screen
    // readers; what comes after it (toasts, the busy overlay) and a menu
    // opened from the dialog later stay live.
    const behind = [];
    for (let el = dialog.parentElement.previousElementSibling; el; el = el.previousElementSibling) {
      if (!el.inert) {
        el.inert = true;
        behind.push(el);
      }
    }
    // After the caller's own effects, which may focus a field. A field comes
    // before the buttons ahead of it (a section's toggle, say).
    const timer = setTimeout(() => {
      if (dialog.contains(document.activeElement)) return;
      const body = tabbable(dialog.querySelector(':scope > .modal-body'));
      const field = body.find((el) => el.matches('input, select, textarea, [contenteditable]'));
      const first = dialog.querySelector('[autofocus]') || field || body[0] || tabbable(dialog.querySelector(':scope > .modal-foot'))[0];
      (first || dialog).focus();
    });
    return () => {
      clearTimeout(timer);
      for (const el of behind) el.inert = false;
      const at = document.activeElement;
      if (before?.isConnected && (!at || at === document.body || dialog.contains(at))) before.focus({ preventScroll: true });
    };
  }, []);

  useEffect(() => {
    const onKey = (e) => {
      const dialog = ref.current;
      // A menu opened from the dialog (drawn above it) handles its own keys.
      if (state.menu || !dialog) return;
      if (e.key === 'Escape') {
        // So does a popover in it: Escape closes just the popover.
        if (dialog.querySelector('.popover')) return;
        e.preventDefault();
        e.stopPropagation();
        onClose?.();
      } else if (e.key === 'Tab') {
        const list = tabbable(dialog);
        const at = document.activeElement;
        const edge = e.shiftKey ? list[0] : list[list.length - 1];
        if (!list.length || at === edge || at === dialog || !dialog.contains(at)) {
          e.preventDefault();
          ((e.shiftKey ? list[list.length - 1] : list[0]) || dialog).focus();
        }
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [onClose]);

  return html`<div class="modal-backdrop" onMouseDown=${(e) => e.target === e.currentTarget && onClose?.()}>
    <div
      ref=${ref}
      class=${cls('modal', extra)}
      style=${{ width: width + 'px' }}
      role="dialog"
      aria-modal="true"
      aria-labelledby=${title ? titleId : undefined}
      tabindex="-1"
    >
      ${title &&
      html`<div class="modal-head">
        <h2 id=${titleId}>${title}</h2>
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
