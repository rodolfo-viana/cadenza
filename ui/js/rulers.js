// Canvas rulers (drawn on <canvas>, in slide units).

import { html, useLayoutEffect, useRef } from './lib.js';
import { t } from './i18n.js';

export const RULER = 22;
const STEPS = [1, 2, 5, 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000, 5000];

function cssVar(name, fallback) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}

/**
 * `orient` "h" (top, x axis) or "v" (left, y axis). `origin` is where slide
 * coordinate 0 falls along the ruler (CSS px); `extent` the slide size;
 * `highlight` an optional [from, to] range (the selection).
 */
export function Ruler({ orient, length, origin, scale, extent, highlight, onPointerDown, theme }) {
  const ref = useRef();
  useLayoutEffect(() => {
    const c = ref.current;
    if (!c || length <= 0) return;
    const dpr = window.devicePixelRatio || 1;
    const horiz = orient === 'h';
    const w = horiz ? length : RULER;
    const h = horiz ? RULER : length;
    c.width = Math.round(w * dpr);
    c.height = Math.round(h * dpr);
    c.style.width = w + 'px';
    c.style.height = h + 'px';
    const g = c.getContext('2d');
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const bg = cssVar('--chrome', '#1c1d23');
    const band = cssVar('--panel-2', '#2a2b34');
    const fg = cssVar('--muted', '#8a8d9b');
    const line = cssVar('--border-strong', '#3a3d49');
    const accent = cssVar('--accent', '#7c6cff');

    g.fillStyle = bg;
    g.fillRect(0, 0, w, h);
    // The slide's extent.
    const a = origin;
    const b = origin + extent * scale;
    g.fillStyle = band;
    if (horiz) g.fillRect(a, 0, b - a, h);
    else g.fillRect(0, a, w, b - a);
    // Selection range.
    if (highlight) {
      const p0 = origin + highlight[0] * scale;
      const p1 = origin + highlight[1] * scale;
      g.globalAlpha = 0.28;
      g.fillStyle = accent;
      if (horiz) g.fillRect(p0, 0, p1 - p0, h);
      else g.fillRect(0, p0, w, p1 - p0);
      g.globalAlpha = 1;
    }

    const major = STEPS.find((s) => s * scale >= 56) || STEPS[STEPS.length - 1];
    const div = major * scale >= 110 ? 10 : 5;
    const minor = major / div;
    const startV = Math.floor(-origin / scale / minor) * minor;
    const endV = (length - origin) / scale;
    g.strokeStyle = fg;
    g.fillStyle = fg;
    g.lineWidth = 1;
    g.font = `500 9.5px ${cssVar('--font', 'sans-serif')}`;
    g.textBaseline = 'top';
    g.beginPath();
    for (let k = 0, v = startV; v <= endV + minor; k++, v = startV + k * minor) {
      const p = Math.round(origin + v * scale) + 0.5;
      const ratio = v / major;
      const isMajor = Math.abs(ratio - Math.round(ratio)) < 1e-6;
      const isHalf = !isMajor && div === 10 && Math.abs(v / (major / 2) - Math.round(v / (major / 2))) < 1e-6;
      const len = isMajor ? RULER * 0.55 : isHalf ? RULER * 0.35 : RULER * 0.2;
      if (horiz) {
        g.moveTo(p, h);
        g.lineTo(p, h - len);
      } else {
        g.moveTo(w, p);
        g.lineTo(w - len, p);
      }
      if (isMajor) {
        const label = String(Math.round(v));
        if (horiz) g.fillText(label, p + 3, 2);
        else {
          g.save();
          g.translate(2, p - 3);
          g.rotate(-Math.PI / 2);
          g.fillText(label, 0, 0);
          g.restore();
        }
      }
    }
    g.stroke();
    g.strokeStyle = line;
    g.beginPath();
    if (horiz) {
      g.moveTo(0, h - 0.5);
      g.lineTo(w, h - 0.5);
    } else {
      g.moveTo(w - 0.5, 0);
      g.lineTo(w - 0.5, h);
    }
    g.stroke();
  }, [orient, length, origin, scale, extent, highlight?.[0], highlight?.[1], theme]);

  return html`<canvas
    ref=${ref}
    class=${'ruler ruler-' + orient}
    title=${orient === 'h' ? t('Drag down to add a horizontal guide') : t('Drag right to add a vertical guide')}
    onPointerDown=${onPointerDown}
  />`;
}
