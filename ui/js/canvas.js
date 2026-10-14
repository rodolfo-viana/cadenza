// The slide canvas: renders the current slide exactly as reveal.js will and
// handles direct manipulation (select, move, resize, rotate, marquee,
// smart guides, inline text editing).

import { html, useEffect, useLayoutEffect, useRef, useState } from './lib.js';
import { Icon } from './icons.js';
import { SlideView } from './slideview.js';
import {
  state,
  set,
  apply,
  useStore,
  currentSlide,
  settings,
  openMenu,
  select,
  updateDeck,
  updateSettings,
} from './store.js';
import { Ruler, RULER } from './rulers.js';
import * as A from './actions.js';
import { item } from './commands.js';
import { beginTextEdit, endTextEdit, editingNode, beginTableEdit, endTableEdit, tableNode, isEditing, isEditingTable } from './textedit.js';
import { bounds, unionBounds, intersects, rotate, clamp, cls } from './util.js';
import { t, language } from './i18n.js';

const PAD = 48; // space around the slide
const MIN = 8; // minimum element size
const HANDLES = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'];
const HANDLE_ANGLE = { n: 0, ne: 45, e: 90, se: 135, s: 180, sw: 225, w: 270, nw: 315 };
const CURSORS = ['ns-resize', 'nesw-resize', 'ew-resize', 'nwse-resize'];

/** Imperative access for other modules (drop targets, menus). */
export const canvasApi = {
  view: null,
  scale: 1,
  frame: null,
  clientToSlide(x, y) {
    if (!this.frame) return null;
    const r = this.frame.getBoundingClientRect();
    return { x: (x - r.left) / this.scale, y: (y - r.top) / this.scale, inside: x >= r.left && x <= r.right && y >= r.top && y <= r.bottom };
  },
};

function cursorFor(handle, rotation) {
  const a = (((HANDLE_ANGLE[handle] + rotation) % 360) + 360) % 360;
  return CURSORS[Math.round(a / 45) % 4];
}

/** Snaps a moving box to slide edges/centers, other elements and guides. */
function snapBox(b, others, W, H, thr, edges = { x: [0, 0.5, 1], y: [0, 0.5, 1] }, extra = null) {
  const xs = [0, W / 2, W, ...(extra?.x || [])];
  const ys = [0, H / 2, H, ...(extra?.y || [])];
  for (const o of others) {
    xs.push(o.x, o.x + o.w / 2, o.x + o.w);
    ys.push(o.y, o.y + o.h / 2, o.y + o.h);
  }
  let best = { dx: 0, dy: 0, gx: null, gy: null };
  let bdx = thr;
  let bdy = thr;
  for (const f of edges.x) {
    const v = b.x + b.w * f;
    for (const sx of xs) {
      const d = sx - v;
      if (Math.abs(d) < Math.abs(bdx)) {
        bdx = d;
        best.dx = d;
        best.gx = sx;
      }
    }
  }
  for (const f of edges.y) {
    const v = b.y + b.h * f;
    for (const sy of ys) {
      const d = sy - v;
      if (Math.abs(d) < Math.abs(bdy)) {
        bdy = d;
        best.dy = d;
        best.gy = sy;
      }
    }
  }
  if (best.gx === null) best.dx = 0;
  if (best.gy === null) best.dy = 0;
  return best;
}

export function Canvas() {
  const st = useStore();
  const doc = st.doc;
  const deck = doc.deck;
  const slide = currentSlide();
  const rendered = slide ? doc.slides[slide.id] : null;
  const W = deck.width;
  const H = deck.height;

  const wrapRef = useRef();
  const frameRef = useRef();
  const slideRef = useRef();
  const viewRef = useRef(null);
  const prevEditing = useRef(null);
  const [box, setBox] = useState({ w: 900, h: 600 });
  const [live, setLive] = useState(null);
  const [guides, setGuides] = useState(null);
  const [marquee, setMarquee] = useState(null);
  const [measured, setMeasured] = useState({});
  const [hover, setHover] = useState(null);
  const [tip, setTip] = useState(null);
  const [scroll, setScroll] = useState({ x: 0, y: 0 });
  const [gdrag, setGdrag] = useState(null);
  const showRulers = settings().showRulers !== false;
  const showGuides = settings().showGuides !== false;
  const userGuides = showGuides ? deck.guides : null;

  // Fit-to-window scale.
  useLayoutEffect(() => {
    const el = wrapRef.current;
    const ro = new ResizeObserver(() => setBox({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    setBox({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, []);
  const fit = Math.max(0.05, Math.min((box.w - PAD * 2) / W, (box.h - PAD * 2) / H));
  const scale = st.zoom === 'fit' ? fit : st.zoom;
  useEffect(() => {
    if (state.fitScale !== fit) state.fitScale = fit;
  }, [fit]);
  canvasApi.scale = scale;

  const stageW = Math.max(box.w, W * scale + PAD * 2);
  const stageH = Math.max(box.h, H * scale + PAD * 2);
  const frameX = Math.round((stageW - W * scale) / 2);
  const frameY = Math.round((stageH - H * scale) / 2);

  // Keep the slide DOM in sync with Rust's rendering.
  useLayoutEffect(() => {
    if (!viewRef.current) viewRef.current = new SlideView(slideRef.current);
    const v = viewRef.current;
    canvasApi.view = v;
    canvasApi.frame = frameRef.current;
    const editingNow = st.editing || st.editingTable;
    v.editingId = editingNow;
    const force = prevEditing.current && !editingNow;
    prevEditing.current = editingNow;
    v.update(rendered?.html || '', force);
    measure();
  }, [rendered?.html, slide?.id, st.editing, st.editingTable, language()]);

  // Leaving the slide ends inline editing.
  useEffect(() => {
    return () => {
      endTextEdit();
      endTableEdit();
    };
  }, [slide?.id]);

  // Measure auto-height text boxes (their real height can exceed `h`).
  function measure() {
    const v = viewRef.current;
    const s = currentSlide();
    if (!v || !s) return;
    const next = {};
    for (const el of s.elements) {
      if (el.content.kind === 'text' && el.content.style.autoHeight) {
        const n = v.node(el.id);
        if (n) next[el.id] = n.offsetHeight;
      }
    }
    setMeasured((prev) => {
      const keys = Object.keys(next);
      if (keys.length === Object.keys(prev).length && keys.every((k) => prev[k] === next[k])) return prev;
      return next;
    });
  }

  useEffect(() => {
    const onInput = () => measure();
    window.addEventListener('cz-text-input', onInput);
    window.addEventListener('cz-edit-ended', onInput);
    document.fonts?.addEventListener?.('loadingdone', onInput);
    return () => {
      window.removeEventListener('cz-text-input', onInput);
      window.removeEventListener('cz-edit-ended', onInput);
      document.fonts?.removeEventListener?.('loadingdone', onInput);
    };
  }, []);

  if (!slide) return html`<div class="canvas-area"><div class="canvas-wrap" ref=${wrapRef} /></div>`;

  const geom = (el) =>
    live?.[el.id] || {
      x: el.x,
      y: el.y,
      w: el.w,
      h: el.content.kind === 'text' && el.content.style.autoHeight ? Math.max(el.h, measured[el.id] || 0) : el.h,
      rotation: el.rotation,
    };

  const toSlide = (e) => {
    const r = frameRef.current.getBoundingClientRect();
    return { x: (e.clientX - r.left) / scale, y: (e.clientY - r.top) / scale };
  };

  const nodeOf = (id) => viewRef.current?.node(id);

  /** Applies temporary geometry straight to the DOM during a gesture. */
  function paint(id, g, el) {
    const n = nodeOf(id);
    if (!n) return;
    n.style.left = g.x + 'px';
    n.style.top = g.y + 'px';
    n.style.width = g.w + 'px';
    if (el.content.kind === 'text' && el.content.style.autoHeight) n.style.minHeight = g.h + 'px';
    else n.style.height = g.h + 'px';
    const rot = n.querySelector('.cz-rot');
    if (rot) rot.style.transform = g.rotation ? `rotate(${g.rotation}deg)` : '';
  }

  function track(onMove, onUp) {
    const move = (ev) => onMove(ev);
    const up = (ev) => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      window.removeEventListener('pointercancel', up);
      document.body.classList.remove('dragging');
      onUp(ev);
    };
    document.body.classList.add('dragging');
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    window.addEventListener('pointercancel', up);
  }

  function others(excludeIds) {
    return slide.elements.filter((e) => !excludeIds.includes(e.id)).map((e) => bounds(geom(e)));
  }

  async function commit(changes, coalesce) {
    const updates = Object.entries(changes).map(([id, g]) => ({
      id,
      patch: { x: Math.round(g.x * 2) / 2, y: Math.round(g.y * 2) / 2, w: Math.round(g.w * 2) / 2, h: Math.round(g.h * 2) / 2, rotation: Math.round(g.rotation * 10) / 10 },
    }));
    await apply({ op: 'updateElements', slide: slide.id, updates }, { coalesce });
    setLive(null);
  }

  // -- gestures ----------------------------------------------------------------

  function startMove(e, ids) {
    const els = slide.elements.filter((el) => ids.includes(el.id) && !el.locked);
    if (!els.length) return;
    const p0 = toSlide(e);
    const orig = Object.fromEntries(els.map((el) => [el.id, { ...geom(el) }]));
    const group = unionBounds(els.map((el) => bounds(orig[el.id])));
    const rest = others(ids);
    const snapOn = settings().snap !== false;
    const grid = settings().showGrid ? settings().gridSize || 40 : 0;
    let moved = false;
    let last = null;
    track(
      (ev) => {
        const p = toSlide(ev);
        let dx = p.x - p0.x;
        let dy = p.y - p0.y;
        if (!moved && Math.hypot(dx, dy) * scale < 3) return;
        moved = true;
        if (ev.shiftKey) {
          if (Math.abs(dx) > Math.abs(dy)) dy = 0;
          else dx = 0;
        }
        let gl = null;
        const mod = ev.ctrlKey || ev.metaKey;
        if (grid && !mod) {
          dx = Math.round((group.x + dx) / grid) * grid - group.x;
          dy = Math.round((group.y + dy) / grid) * grid - group.y;
        } else if (snapOn && !mod) {
          const s = snapBox({ ...group, x: group.x + dx, y: group.y + dy }, rest, W, H, 6 / scale, undefined, userGuides);
          dx += s.dx;
          dy += s.dy;
          gl = { x: s.gx, y: s.gy };
        }
        const next = {};
        for (const el of els) {
          const o = orig[el.id];
          next[el.id] = { ...o, x: o.x + dx, y: o.y + dy };
          paint(el.id, next[el.id], el);
        }
        last = next;
        setLive(next);
        setGuides(gl);
        setTip({ x: group.x + dx, y: group.y + dy - 8 / scale, text: `${Math.round(group.x + dx)}, ${Math.round(group.y + dy)}` });
      },
      () => {
        setGuides(null);
        setTip(null);
        if (moved && last) commit(last);
        else setLive(null);
      },
    );
  }

  function startResize(e, el, handle) {
    e.stopPropagation();
    e.preventDefault();
    const g0 = { ...geom(el) };
    const hx = handle.includes('e') ? 1 : handle.includes('w') ? -1 : 0;
    const hy = handle.includes('s') ? 1 : handle.includes('n') ? -1 : 0;
    const c0 = { x: g0.x + g0.w / 2, y: g0.y + g0.h / 2 };
    const [ax, ay] = rotate(c0.x - (hx * g0.w) / 2, c0.y - (hy * g0.h) / 2, c0.x, c0.y, g0.rotation);
    const kind = el.content.kind;
    // Pictures keep their proportions on corner handles (Shift toggles).
    const naturalAspect = kind === 'image' || kind === 'video';
    const rest = others([el.id]);
    const snapOn = settings().snap !== false && !g0.rotation;
    let last = null;
    track(
      (ev) => {
        const p = toSlide(ev);
        const [lx, ly] = rotate(p.x, p.y, ax, ay, -g0.rotation);
        const dx = lx - ax;
        const dy = ly - ay;
        let w = hx ? Math.max(MIN, hx * dx) : g0.w;
        let h = hy ? Math.max(MIN, hy * dy) : g0.h;
        const keep = hx && hy ? naturalAspect !== ev.shiftKey : ev.shiftKey;
        if (keep) {
          const ratio = g0.w / g0.h;
          if (hx && hy) {
            const s = Math.max(w / g0.w, h / g0.h);
            w = g0.w * s;
            h = g0.h * s;
          } else if (hx) h = w / ratio;
          else w = h * ratio;
        }
        let [cx, cy] = rotate(ax + (hx * w) / 2, ay + (hy * h) / 2, ax, ay, g0.rotation);
        let g = { x: cx - w / 2, y: cy - h / 2, w, h, rotation: g0.rotation };
        let gl = null;
        if (snapOn && !keep && !(ev.ctrlKey || ev.metaKey)) {
          const s = snapBox(g, rest, W, H, 6 / scale, { x: hx ? [hx > 0 ? 1 : 0] : [], y: hy ? [hy > 0 ? 1 : 0] : [] }, userGuides);
          if (hx > 0) g.w += s.dx;
          if (hx < 0) {
            g.x += s.dx;
            g.w -= s.dx;
          }
          if (hy > 0) g.h += s.dy;
          if (hy < 0) {
            g.y += s.dy;
            g.h -= s.dy;
          }
          gl = { x: s.gx, y: s.gy };
        }
        last = g;
        paint(el.id, g, el);
        setLive({ [el.id]: g });
        setGuides(gl);
        setTip({ x: g.x, y: g.y - 8 / scale, text: `${Math.round(g.w)} × ${Math.round(g.h)}` });
      },
      () => {
        setGuides(null);
        setTip(null);
        if (last) commit({ [el.id]: last });
        else setLive(null);
      },
    );
  }

  function startRotate(e, el) {
    e.stopPropagation();
    e.preventDefault();
    const g0 = { ...geom(el) };
    const c = { x: g0.x + g0.w / 2, y: g0.y + g0.h / 2 };
    const p0 = toSlide(e);
    const a0 = Math.atan2(p0.y - c.y, p0.x - c.x);
    let last = null;
    track(
      (ev) => {
        const p = toSlide(ev);
        let r = g0.rotation + ((Math.atan2(p.y - c.y, p.x - c.x) - a0) * 180) / Math.PI;
        r = ((((r + 180) % 360) + 360) % 360) - 180;
        if (ev.shiftKey) r = Math.round(r / 15) * 15;
        else
          for (const snap of [-180, -90, 0, 90, 180]) {
            if (Math.abs(r - snap) < 3) r = snap;
          }
        last = { ...g0, rotation: r };
        paint(el.id, last, el);
        setLive({ [el.id]: last });
        setTip({ x: g0.x, y: g0.y - 8 / scale, text: `${Math.round(r)}°` });
      },
      () => {
        setTip(null);
        if (last) commit({ [el.id]: last });
        else setLive(null);
      },
    );
  }

  function startMarquee(e) {
    const p0 = toSlide(e);
    const base = e.shiftKey || e.ctrlKey || e.metaKey ? [...state.selection] : [];
    let moved = false;
    track(
      (ev) => {
        const p = toSlide(ev);
        if (!moved && Math.hypot(p.x - p0.x, p.y - p0.y) * scale < 4) return;
        moved = true;
        const r = { x: Math.min(p0.x, p.x), y: Math.min(p0.y, p.y), w: Math.abs(p.x - p0.x), h: Math.abs(p.y - p0.y) };
        setMarquee(r);
        const hit = slide.elements.filter((el) => intersects(bounds(geom(el)), r)).map((el) => el.id);
        select([...new Set([...base, ...hit])]);
      },
      () => {
        setMarquee(null);
        if (!moved && !base.length) select([]);
      },
    );
  }

  /**
   * Moves a guide (index ≥ 0) or creates one dragged out of a ruler
   * (index = -1). Dropping it outside the canvas removes it.
   */
  function startGuide(e, axis, index) {
    if (e.button !== 0) return;
    e.stopPropagation();
    e.preventDefault();
    let v = index >= 0 ? deck.guides[axis][index] : null;
    let removing = index < 0;
    let moved = false;
    const x0 = e.clientX;
    const y0 = e.clientY;
    const lim = axis === 'x' ? W : H;
    const cands = [0, lim / 2, lim];
    for (const el of slide.elements) {
      const b = bounds(geom(el));
      if (axis === 'x') cands.push(b.x, b.x + b.w / 2, b.x + b.w);
      else cands.push(b.y, b.y + b.h / 2, b.y + b.h);
    }
    track(
      (ev) => {
        if (!moved && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 3) return;
        moved = true;
        const p = toSlide(ev);
        let val = axis === 'x' ? p.x : p.y;
        let best = null;
        for (const c of cands) if (Math.abs(c - val) * scale < 6 && (best === null || Math.abs(c - val) < Math.abs(best - val))) best = c;
        val = best ?? Math.round(val);
        const wr = wrapRef.current.getBoundingClientRect();
        removing = ev.clientX < wr.left || ev.clientY < wr.top || ev.clientX > wr.right || ev.clientY > wr.bottom;
        v = val;
        setGdrag({ axis, index, v, removing });
        setTip({ x: axis === 'x' ? val + 6 / scale : (ev.clientX - frameRef.current.getBoundingClientRect().left) / scale + 12 / scale, y: axis === 'x' ? p.y : val - 4 / scale, text: removing ? t('Remove guide') : `${axis === 'x' ? 'x' : 'y'} ${Math.round(val)}` });
      },
      (ev) => {
        setGdrag(null);
        setTip(null);
        // A click (no drag) on a guide goes to the object underneath.
        if (!moved) {
          if (index >= 0) select(underGuide(ev) ? [underGuide(ev).getAttribute('data-cz-id')] : []);
          return;
        }
        if (v == null) return;
        const list = [...deck.guides[axis]];
        if (index >= 0) {
          if (removing) list.splice(index, 1);
          else list[index] = v;
        } else if (!removing) list.push(v);
        else return;
        if (!showGuides) updateSettings({ showGuides: true });
        updateDeck({ guides: { ...deck.guides, [axis]: list } });
      },
    );
  }

  function underGuide(ev) {
    const hit = document.elementsFromPoint(ev.clientX, ev.clientY).find((n) => !n.classList?.contains('uguide') && n.closest?.('.cz-el[data-cz-id]'));
    const elNode = hit?.closest('.cz-el[data-cz-id]');
    return elNode && slideRef.current.contains(elNode) ? elNode : null;
  }

  function guideDoubleClick(ev) {
    const elNode = underGuide(ev);
    if (elNode) onDoubleClick({ target: elNode, clientX: ev.clientX, clientY: ev.clientY });
  }

  function guideMenu(e, axis, index) {
    e.preventDefault();
    e.stopPropagation();
    openMenu(e.clientX, e.clientY, [
      { label: t('Remove Guide'), icon: 'trash-2', run: () => updateDeck({ guides: { ...deck.guides, [axis]: deck.guides[axis].filter((_, i) => i !== index) } }) },
      { label: t('Clear All Guides'), icon: 'x', run: () => updateDeck({ guides: { x: [], y: [] } }) },
      '-',
      { label: t('Hide Guides'), icon: 'eye-off', run: () => updateSettings({ showGuides: false }) },
    ]);
  }

  // -- event handlers ------------------------------------------------------------

  async function onPointerDown(e) {
    if (e.button !== 0) return;
    set({ focus: 'canvas', menu: null });
    const target = e.target;
    const editNode = editingNode();
    if (editNode && editNode.contains(target)) return;
    const tbl = tableNode();
    if (tbl && tbl.contains(target)) return;
    if (isEditing() && !(await endTextEdit())) return;
    if (isEditingTable() && !(await endTableEdit())) return;
    if (document.activeElement && document.activeElement !== document.body && !wrapRef.current.contains(document.activeElement)) {
      document.activeElement.blur();
    }
    const elNode = target.closest?.('.cz-el[data-cz-id]');
    if (elNode && slideRef.current.contains(elNode)) {
      e.preventDefault();
      const id = elNode.getAttribute('data-cz-id');
      const additive = e.shiftKey || e.ctrlKey || e.metaKey;
      let sel = state.selection;
      if (additive) {
        sel = sel.includes(id) ? sel.filter((x) => x !== id) : [...sel, id];
        select(sel);
        return;
      }
      if (!sel.includes(id)) {
        sel = [id];
        select(sel);
      }
      startMove(e, sel);
      return;
    }
    e.preventDefault();
    startMarquee(e);
  }

  function onDoubleClick(e) {
    const elNode = e.target.closest?.('.cz-el[data-cz-id]');
    if (!elNode) return;
    const el = slide.elements.find((x) => x.id === elNode.getAttribute('data-cz-id'));
    if (!el) return;
    const k = el.content.kind;
    const point = { x: e.clientX, y: e.clientY };
    if (k === 'text' || (k === 'shape' && el.content.shape !== 'line')) beginTextEdit(viewRef.current, el, point);
    else if (k === 'table') beginTableEdit(viewRef.current, el, point);
    else if (k === 'image') A.replaceImage(el);
    else if (k === 'video' && !el.content.src) A.replaceVideo(el);
    else {
      set({ tab: 'format' });
      setTimeout(() => window.dispatchEvent(new CustomEvent('cz-focus-primary')), 30);
    }
  }

  function onContextMenu(e) {
    e.preventDefault();
    const elNode = e.target.closest?.('.cz-el[data-cz-id]');
    if (elNode && slideRef.current.contains(elNode)) {
      const id = elNode.getAttribute('data-cz-id');
      if (!state.selection.includes(id)) select([id]);
      openMenu(e.clientX, e.clientY, elementMenu());
    } else {
      select([]);
      openMenu(e.clientX, e.clientY, slideMenu());
    }
  }

  function onPointerMove(e) {
    if (document.body.classList.contains('dragging')) return;
    const elNode = e.target.closest?.('.cz-el[data-cz-id]');
    const id = elNode && slideRef.current.contains(elNode) ? elNode.getAttribute('data-cz-id') : null;
    if (id !== hover) setHover(id);
  }

  function onWheel(e) {
    if (!(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    const f = e.deltaY < 0 ? 1.1 : 1 / 1.1;
    set({ zoom: clamp(Math.round(scale * f * 100) / 100, 0.1, 4) });
  }

  // -- overlay ---------------------------------------------------------------------

  const selected = slide.elements.filter((el) => st.selection.includes(el.id));
  const single = selected.length === 1 ? selected[0] : null;
  const editingId = st.editing || st.editingTable;

  const box2screen = (g) => ({
    left: g.x * scale + 'px',
    top: g.y * scale + 'px',
    width: g.w * scale + 'px',
    height: g.h * scale + 'px',
    transform: g.rotation ? `rotate(${g.rotation}deg)` : null,
  });

  const hoverEl = hover && !st.selection.includes(hover) ? slide.elements.find((x) => x.id === hover) : null;

  const handlesFor = (el, g) => {
    if (el.locked || editingId === el.id || live) return null;
    const isLine = el.content.kind === 'shape' && el.content.shape === 'line';
    const hs = isLine ? ['e', 'w'] : HANDLES;
    return html`${hs.map(
      (h) => html`<div
        class=${'handle h-' + h}
        style=${{ cursor: cursorFor(h, g.rotation) }}
        onPointerDown=${(e) => startResize(e, el, h)}
      />`,
    )}
      <div class="rot-stem" />
      <div class="rot-handle" title=${t('Rotate (Shift: 15° steps)')} onPointerDown=${(e) => startRotate(e, el)}><${Icon} name="rotate-cw" size=${11} /></div>`;
  };

  const groupB = selected.length > 1 ? unionBounds(selected.map((el) => bounds(geom(el)))) : null;
  const selB = selected.length ? unionBounds(selected.map((el) => bounds(geom(el)))) : null;

  // User guides, with the one being dragged at its live position.
  const guideLines = [];
  if (showGuides || gdrag) {
    for (const axis of ['x', 'y']) {
      deck.guides[axis].forEach((v, i) => {
        if (gdrag && gdrag.axis === axis && gdrag.index === i) return;
        guideLines.push({ axis, index: i, v, removing: false });
      });
    }
    if (gdrag && gdrag.v != null) guideLines.push({ ...gdrag, active: true });
  }

  const theme = document.documentElement.dataset.ui;
  return html`<div class=${cls('canvas-area', showRulers && 'with-rulers')}>
    ${showRulers &&
    html`<div class="ruler-corner" title=${t('Hide rulers (Ctrl+R)')} onClick=${() => updateSettings({ showRulers: false })} />
      <${Ruler}
        orient="h"
        length=${box.w}
        origin=${frameX - scroll.x}
        scale=${scale}
        extent=${W}
        theme=${theme}
        highlight=${selB ? [selB.x, selB.x + selB.w] : null}
        onPointerDown=${(e) => startGuide(e, 'y', -1)}
      />
      <${Ruler}
        orient="v"
        length=${box.h}
        origin=${frameY - scroll.y}
        scale=${scale}
        extent=${H}
        theme=${theme}
        highlight=${selB ? [selB.y, selB.y + selB.h] : null}
        onPointerDown=${(e) => startGuide(e, 'x', -1)}
      />`}
    <div class="canvas-wrap" ref=${wrapRef} onWheel=${onWheel} onScroll=${(e) => setScroll({ x: e.currentTarget.scrollLeft, y: e.currentTarget.scrollTop })}>
    <div
      class="stage"
      style=${{ width: stageW + 'px', height: stageH + 'px' }}
      onPointerDown=${onPointerDown}
      onDblClick=${onDoubleClick}
      onContextMenu=${onContextMenu}
      onPointerMove=${onPointerMove}
      onPointerLeave=${() => setHover(null)}
    >
      <div
        class="slide-frame reveal-viewport"
        ref=${frameRef}
        style=${{ left: frameX + 'px', top: frameY + 'px', width: W * scale + 'px', height: H * scale + 'px' }}
      >
        <div class="reveal cz-editor-reveal" style=${{ width: W + 'px', height: H + 'px', transform: `scale(${scale})` }}>
          <div class="slides">
            <section class=${cls('present', rendered?.classes)} style=${{ width: W + 'px', height: H + 'px' }}>
              <div class="cz-slide" ref=${slideRef} style=${{ width: W + 'px', height: H + 'px' }} />
            </section>
          </div>
        </div>
        ${settings().showGrid && html`<div class="grid-overlay" style=${{ backgroundSize: `${(settings().gridSize || 40) * scale}px ${(settings().gridSize || 40) * scale}px` }} />`}
      </div>
      <div class="overlay" style=${{ left: frameX + 'px', top: frameY + 'px', width: W * scale + 'px', height: H * scale + 'px' }}>
        ${hoverEl && html`<div class="hover-box" style=${box2screen(geom(hoverEl))} />`}
        ${selected.map((el) => {
          const g = geom(el);
          return html`<div
            key=${el.id}
            class=${cls('sel-box', el.locked && 'locked', editingId === el.id && 'editing', selected.length > 1 && 'multi')}
            style=${box2screen(g)}
          >
            ${single && handlesFor(el, g)}
            ${el.locked && html`<div class="lock-badge" title=${t('Locked')}><${Icon} name="lock" size=${11} /></div>`}
          </div>`;
        })}
        ${groupB && html`<div class="group-box" style=${box2screen(groupB)} />`}
        ${guideLines.map((g) =>
          g.axis === 'x'
            ? html`<div
                key=${'gx' + g.index}
                class=${cls('uguide v', g.active && 'active', g.removing && 'removing')}
                style=${{ left: g.v * scale + 'px', top: -frameY + 'px', height: stageH + 'px' }}
                onPointerDown=${(e) => g.index >= 0 && startGuide(e, 'x', g.index)}
                onContextMenu=${(e) => g.index >= 0 && guideMenu(e, 'x', g.index)}
                onDblClick=${guideDoubleClick}
              />`
            : html`<div
                key=${'gy' + g.index}
                class=${cls('uguide h', g.active && 'active', g.removing && 'removing')}
                style=${{ top: g.v * scale + 'px', left: -frameX + 'px', width: stageW + 'px' }}
                onPointerDown=${(e) => g.index >= 0 && startGuide(e, 'y', g.index)}
                onContextMenu=${(e) => g.index >= 0 && guideMenu(e, 'y', g.index)}
                onDblClick=${guideDoubleClick}
              />`,
        )}
        ${guides?.x != null && html`<div class="guide v" style=${{ left: guides.x * scale + 'px', top: -frameY + 'px', height: stageH + 'px' }} />`}
        ${guides?.y != null && html`<div class="guide h" style=${{ top: guides.y * scale + 'px', left: -frameX + 'px', width: stageW + 'px' }} />`}
        ${marquee && html`<div class="marquee" style=${box2screen(marquee)} />`}
        ${tip && html`<div class="geo-tip" style=${{ left: tip.x * scale + 'px', top: tip.y * scale + 'px' }}>${tip.text}</div>`}
      </div>
      ${slide.hidden && html`<div class="slide-flag" style=${{ left: frameX + 'px', top: frameY - 28 + 'px' }}><${Icon} name="eye-off" size=${13} /> ${t('Skipped during the presentation')}</div>`}
    </div>
  </div>
  </div>`;
}

// ---------------------------------------------------------------------------
// Context menus
// ---------------------------------------------------------------------------

function elementMenu() {
  const els = state.doc.deck.slides.find((s) => s.id === state.slideId)?.elements.filter((e) => state.selection.includes(e.id)) || [];
  const one = els.length === 1 ? els[0] : null;
  const k = one?.content.kind;
  const locked = els.length && els.every((e) => e.locked);
  return [
    one && (k === 'text' || (k === 'shape' && one.content.shape !== 'line')) && {
      label: t('Edit Text'),
      icon: 'text-cursor-input',
      run: () => beginTextEdit(canvasApi.view, one, null, { selectAll: true }),
    },
    one && k === 'table' && { label: t('Edit Cells'), icon: 'table', run: () => beginTableEdit(canvasApi.view, one) },
    one && k === 'image' && { label: one.content.src ? t('Replace Image…') : t('Choose Image…'), icon: 'image-up', run: () => A.replaceImage(one) },
    one && k === 'image' && { label: t('Find Photo Online…'), icon: 'images', run: () => import('./media.js').then((m) => m.openMediaLibrary('photos', one.id)) },
    one && k === 'image' && one.content.naturalWidth && { label: t('Original Proportions'), icon: 'ratio', run: () => A.resetImageSize(one) },
    one && k === 'video' && { label: t('Choose Video File…'), icon: 'film', run: () => A.replaceVideo(one) },
    one && '-',
    { label: t('Cut'), icon: 'scissors', keys: 'Mod+X', run: () => document.execCommand('cut') },
    { label: t('Copy'), icon: 'copy', keys: 'Mod+C', run: () => document.execCommand('copy') },
    item('edit.duplicate'),
    item('edit.delete', { danger: true }),
    '-',
    {
      label: t('Arrange'),
      icon: 'layers',
      children: [item('arrange.front'), item('arrange.forward'), item('arrange.backward'), item('arrange.back')],
    },
    {
      label: t('Align'),
      icon: 'align-center-vertical',
      children: [
        item('align.left'),
        item('align.hcenter'),
        item('align.right'),
        '-',
        item('align.top'),
        item('align.vcenter'),
        item('align.bottom'),
        '-',
        item('align.distH', { disabled: els.length < 3 }),
        item('align.distV', { disabled: els.length < 3 }),
        '-',
        item('align.centerSlide'),
      ],
    },
    { label: locked ? t('Unlock') : t('Lock'), icon: locked ? 'lock-open' : 'lock', keys: 'Mod+L', run: A.toggleLock },
    '-',
    { label: t('Animate…'), icon: 'sparkles', run: () => set({ tab: 'animate' }) },
  ].filter(Boolean);
}

function slideMenu() {
  return [
    { label: t('Paste'), icon: 'clipboard-paste', keys: 'Mod+V', run: A.pasteFromMenu },
    item('edit.selectAll'),
    '-',
    {
      label: t('Insert'),
      icon: 'plus',
      children: [item('insert.text'), item('insert.title'), item('insert.shape'), item('insert.image'), item('insert.table'), item('insert.chart'), item('insert.code'), item('insert.math')],
    },
    '-',
    {
      label: t('Guides'),
      icon: 'ratio',
      children: [
        { label: t('Add Vertical Guide at Center'), run: () => addGuide('x') },
        { label: t('Add Horizontal Guide at Center'), run: () => addGuide('y') },
        '-',
        { label: t('Show Guides'), checked: settings().showGuides !== false, keys: 'Mod+;', run: () => updateSettings({ showGuides: settings().showGuides === false }) },
        { label: t('Show Rulers'), checked: settings().showRulers !== false, keys: 'Mod+R', run: () => updateSettings({ showRulers: settings().showRulers === false }) },
        { label: t('Clear All Guides'), run: () => updateDeck({ guides: { x: [], y: [] } }) },
      ],
    },
    { label: t('Slide Background…'), icon: 'palette', run: () => set({ tab: 'format', selection: [] }) },
    { label: t('Transition…'), icon: 'sparkles', run: () => set({ tab: 'animate', selection: [] }) },
    '-',
    item('slide.new'),
    item('slide.duplicate'),
    item('slide.skip'),
  ];
}

/** Adds a guide through the middle of the slide. */
export function addGuide(axis) {
  const d = state.doc?.deck;
  if (!d) return;
  const v = axis === 'x' ? d.width / 2 : d.height / 2;
  if (d.guides[axis].some((g) => Math.abs(g - v) < 0.5)) return;
  if (settings().showGuides === false) updateSettings({ showGuides: true });
  updateDeck({ guides: { ...d.guides, [axis]: [...d.guides[axis], v] } });
}
