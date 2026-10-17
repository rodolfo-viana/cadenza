// Teleprompter: a box in the presenter display that scrolls the current
// slide's speaker notes. It exists only in the presenter display, so the
// audience never sees it. Settings (look, speed, position) are app
// preferences, saved as they change.

import { t } from './i18n.js';
import { ICONS } from './icon-data.js';

export const SPEED_MIN = 10;
export const SPEED_MAX = 200;
export const SPEED_STEP = 5;
const MIN_W = 240;
const MIN_H = 120;
/** Pause before scrolling starts on a new slide, to find the first line. */
const START_DELAY = 1000;

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** The box in pixels inside a w×h window; `box` holds fractions of the window. */
export function boxRect(box, w, h) {
  const width = clamp(box.width * w, MIN_W, w);
  const height = clamp(box.height * h, MIN_H, h);
  return { left: clamp(box.x * w, 0, w - width), top: clamp(box.y * h, 0, h - height), width, height };
}

/** The box after dragging it (`dir` null) or one of its edges/corners ("n", "se", …) by dx, dy pixels. */
export function dragRect(r, dir, dx, dy, w, h) {
  let left = r.left;
  let top = r.top;
  let right = r.left + r.width;
  let bottom = r.top + r.height;
  if (!dir) {
    left = clamp(left + dx, 0, w - r.width);
    top = clamp(top + dy, 0, h - r.height);
    right = left + r.width;
    bottom = top + r.height;
  } else {
    if (dir.includes('w')) left = clamp(left + dx, 0, right - MIN_W);
    if (dir.includes('e')) right = clamp(right + dx, left + MIN_W, w);
    if (dir.includes('n')) top = clamp(top + dy, 0, bottom - MIN_H);
    if (dir.includes('s')) bottom = clamp(bottom + dy, top + MIN_H, h);
  }
  return { left, top, width: right - left, height: bottom - top };
}

/** Scroll position after `dt` seconds; a long frame (busy machine) doesn't jump ahead. */
export function advance(pos, speed, dt, max) {
  return clamp(pos + speed * Math.min(dt, 0.1), 0, max);
}

export const clampSpeed = (v) => clamp(Math.round(v / SPEED_STEP) * SPEED_STEP, SPEED_MIN, SPEED_MAX);

/** `#rrggbb` with an opacity, as rgba(). */
export function rgba(hex, alpha) {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex || '');
  const n = m ? parseInt(m[1], 16) : 0x111827;
  return `rgba(${n >> 16}, ${(n >> 8) & 255}, ${n & 255}, ${clamp(Number(alpha) || 0, 0, 1)})`;
}

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);

const icon = (name) =>
  `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICONS[name] || ''}</svg>`;

/** Settings restored by "Restore defaults" (everything but whether the box is shown). */
const RESETTABLE = ['speed', 'autoStart', 'fontSize', 'fontFamily', 'lineHeight', 'textColor', 'background', 'opacity', 'mirror', 'guide', 'x', 'y', 'width', 'height'];

/**
 * Creates the teleprompter box in the current document.
 * `settings`: the teleprompter preferences; `invoke`: Tauri's invoke;
 * `onVisibilityChange(visible)` reports showing/hiding.
 */
export function createTeleprompter({ settings, invoke, onVisibilityChange = () => {} }) {
  const s = { ...settings };
  let pos = 0;
  let max = 0;
  let playing = false;
  let frame = 0;
  let last = 0;
  let startTimer = 0;
  let pending = {};
  let saveTimer = 0;
  let fontsLoaded = false;

  const root = document.createElement('section');
  root.className = 'tp';
  root.setAttribute('aria-label', t('Teleprompter'));
  root.innerHTML = `
    <div class="tp-stage"><div class="tp-text" dir="auto"></div><div class="tp-guide"></div></div>
    <div class="tp-bar">
      <div class="tp-progress"><i></i></div>
      <button type="button" data-act="play"></button>
      <button type="button" data-act="restart" title="${esc(t('Back to the top'))}">${icon('rotate-ccw')}</button>
      <span class="tp-speed">
        <button type="button" data-act="slower" title="${esc(t('Slower ([)'))}">${icon('minus')}</button>
        <span class="tp-speed-val"></span>
        <button type="button" data-act="faster" title="${esc(t('Faster (])'))}">${icon('plus')}</button>
      </span>
      <span class="tp-spacer"></span>
      <button type="button" data-act="settings" title="${esc(t('Teleprompter settings'))}">${icon('settings')}</button>
      <button type="button" data-act="hide" title="${esc(t('Hide the teleprompter (T)'))}">${icon('x')}</button>
    </div>
    ${['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'].map((d) => `<div class="tp-h tp-h-${d}" data-dir="${d}"></div>`).join('')}`;

  const panel = document.createElement('div');
  panel.className = 'tp-panel';
  panel.hidden = true;
  panel.innerHTML = `
    <label class="tp-row"><span>${esc(t('Text size'))}</span><output data-out="fontSize"></output><input type="range" data-key="fontSize" min="16" max="120" step="1"></label>
    <label class="tp-row"><span>${esc(t('Font'))}</span><select data-key="fontFamily"></select></label>
    <label class="tp-row"><span>${esc(t('Line spacing'))}</span><output data-out="lineHeight"></output><input type="range" data-key="lineHeight" min="1" max="2.5" step="0.05"></label>
    <label class="tp-row"><span>${esc(t('Text color'))}</span><input type="color" data-key="textColor"></label>
    <label class="tp-row"><span>${esc(t('Background'))}</span><input type="color" data-key="background"></label>
    <label class="tp-row"><span>${esc(t('Opacity'))}</span><output data-out="opacity"></output><input type="range" data-key="opacity" min="0" max="1" step="0.05"></label>
    <label class="tp-check"><input type="checkbox" data-key="autoStart"> ${esc(t('Start scrolling on each slide'))}</label>
    <label class="tp-check"><input type="checkbox" data-key="guide"> ${esc(t('Reading line'))}</label>
    <label class="tp-check"><input type="checkbox" data-key="mirror"> ${esc(t('Mirror the text (for teleprompter glass)'))}</label>
    <div class="tp-panel-foot"><button type="button" data-act="defaults">${esc(t('Restore defaults'))}</button></div>`;

  const $ = (sel, from = root) => from.querySelector(sel);
  const stage = $('.tp-stage');
  const text = $('.tp-text');
  const bar = $('.tp-bar');
  const playBtn = $('[data-act="play"]');
  const settingsBtn = $('[data-act="settings"]');
  const fontSelect = $('select', panel);
  document.body.append(root, panel);

  // ---- Saving ----------------------------------------------------------------
  function flush() {
    clearTimeout(saveTimer);
    if (!Object.keys(pending).length) return;
    const patch = pending;
    pending = {};
    invoke('settings_update', { patch: { teleprompter: patch } }).catch(() => {});
  }

  function save(patch, delay = 0) {
    Object.assign(pending, patch);
    clearTimeout(saveTimer);
    saveTimer = setTimeout(flush, delay);
  }

  addEventListener('pagehide', flush);

  // ---- Layout and look -------------------------------------------------------
  function render() {
    text.style.transform = `translate3d(0, ${-pos}px, 0)`;
    $('.tp-progress i').style.width = (max > 0 ? (pos / max) * 100 : 0) + '%';
  }

  /** Recomputes how far the text can scroll: long texts scroll until their last line reaches the reading line. */
  function measure() {
    text.style.paddingBottom = '';
    const overflows = text.offsetHeight > stage.clientHeight;
    if (overflows) text.style.paddingBottom = Math.round(stage.clientHeight / 2) + 'px';
    max = overflows ? text.offsetHeight - stage.clientHeight : 0;
    pos = clamp(pos, 0, max);
    render();
  }

  function layout() {
    const r = boxRect(s, innerWidth, innerHeight);
    for (const k of ['left', 'top', 'width', 'height']) root.style[k] = r[k] + 'px';
    measure();
  }

  function fontStack(family) {
    const name = String(family || '').replace(/["\\]/g, '');
    return name ? `"${name}", var(--font)` : 'var(--font)';
  }

  function apply() {
    root.style.setProperty('--tp-bg', rgba(s.background, s.opacity));
    root.style.setProperty('--tp-fg', s.textColor);
    root.style.setProperty('--tp-size', s.fontSize + 'px');
    root.style.setProperty('--tp-line', String(s.lineHeight));
    root.style.setProperty('--tp-font', fontStack(s.fontFamily));
    root.classList.toggle('mirror', !!s.mirror);
    root.classList.toggle('guide', !!s.guide);
    $('.tp-speed-val').textContent = t('{n} px/s', { n: s.speed });
    syncPanel();
    measure();
  }

  // ---- Scrolling -------------------------------------------------------------
  function updatePlay() {
    root.classList.toggle('playing', playing);
    playBtn.innerHTML = icon(playing ? 'pause' : 'play');
    playBtn.title = playing ? t('Pause scrolling (S)') : t('Start scrolling (S)');
  }

  function tick(now) {
    frame = 0;
    if (!playing) return;
    const dt = last ? (now - last) / 1000 : 0;
    last = now;
    pos = advance(pos, s.speed, dt, max);
    render();
    if (pos >= max) return stop();
    frame = requestAnimationFrame(tick);
  }

  function play() {
    clearTimeout(startTimer);
    if (root.hidden) return;
    measure();
    if (max <= 0) return;
    if (pos >= max) pos = 0;
    playing = true;
    last = 0;
    updatePlay();
    if (!frame) frame = requestAnimationFrame(tick);
  }

  function stop() {
    clearTimeout(startTimer);
    playing = false;
    cancelAnimationFrame(frame);
    frame = 0;
    updatePlay();
  }

  function changeSpeed(steps) {
    s.speed = clampSpeed(s.speed + steps * SPEED_STEP);
    $('.tp-speed-val').textContent = t('{n} px/s', { n: s.speed });
    save({ speed: s.speed }, 400);
  }

  /** Shows the notes of the slide on screen, from the top. */
  function setSlide(notes) {
    stop();
    const value = String(notes || '').trim();
    text.textContent = value || t('No notes for this slide.');
    root.classList.toggle('empty', !value);
    pos = 0;
    measure();
    if (value && s.autoStart && !root.hidden) startTimer = setTimeout(play, START_DELAY);
  }

  function show(visible) {
    root.hidden = !visible;
    if (!visible) {
      stop();
      closePanel();
    } else {
      layout();
      // Opened during a slide: it starts as it would on a new slide.
      if (s.autoStart && !playing && pos === 0 && !root.classList.contains('empty')) {
        clearTimeout(startTimer);
        startTimer = setTimeout(play, START_DELAY);
      }
    }
    if (s.enabled !== visible) {
      s.enabled = visible;
      save({ enabled: visible });
    }
    onVisibilityChange(visible);
  }

  // ---- Settings panel ----------------------------------------------------------
  function syncPanel() {
    panel.querySelectorAll('[data-key]').forEach((input) => {
      const v = s[input.dataset.key];
      if (input.type === 'checkbox') input.checked = !!v;
      else if (input !== fontSelect) input.value = String(v);
    });
    $('[data-out="fontSize"]', panel).textContent = `${s.fontSize} px`;
    $('[data-out="lineHeight"]', panel).textContent = Number(s.lineHeight).toFixed(2);
    $('[data-out="opacity"]', panel).textContent = `${Math.round(s.opacity * 100)}%`;
    fillFonts();
  }

  function fillFonts(families = []) {
    const names = [...new Set([...families, ...[...fontSelect.options].map((o) => o.value).filter(Boolean)])];
    if (s.fontFamily && !names.includes(s.fontFamily)) names.unshift(s.fontFamily);
    names.sort((a, b) => a.localeCompare(b));
    if (fontSelect.options.length !== names.length + 1) {
      fontSelect.innerHTML = '';
      fontSelect.add(new Option(t('Interface font'), ''));
      for (const n of names) fontSelect.add(new Option(n, n));
    }
    fontSelect.value = s.fontFamily || '';
  }

  function openPanel() {
    panel.hidden = false;
    if (!fontsLoaded) {
      fontsLoaded = true;
      invoke('fonts_list')
        .then((families) => fillFonts(families))
        .catch(() => {});
    }
    // Next to the box, on the side with more room.
    const b = settingsBtn.getBoundingClientRect();
    const box = root.getBoundingClientRect();
    const ph = panel.offsetHeight;
    const pw = panel.offsetWidth;
    const below = innerHeight - box.bottom >= box.top;
    panel.style.top = clamp(below ? box.bottom + 8 : box.top - ph - 8, 8, Math.max(8, innerHeight - ph - 8)) + 'px';
    panel.style.left = clamp(b.right - pw, 8, Math.max(8, innerWidth - pw - 8)) + 'px';
    settingsBtn.classList.add('on');
  }

  function closePanel() {
    if (panel.hidden) return false;
    panel.hidden = true;
    settingsBtn.classList.remove('on');
    if (panel.contains(document.activeElement)) document.activeElement.blur();
    return true;
  }

  const valueOf = (input) => {
    if (input.type === 'checkbox') return input.checked;
    if (input.type === 'range') return Number(input.value);
    return input.value;
  };

  panel.addEventListener('input', (e) => {
    const input = e.target.closest('[data-key]');
    if (!input) return;
    s[input.dataset.key] = valueOf(input);
    apply();
  });

  panel.addEventListener('change', (e) => {
    const input = e.target.closest('[data-key]');
    if (!input) return;
    s[input.dataset.key] = valueOf(input);
    apply();
    save({ [input.dataset.key]: s[input.dataset.key] });
  });

  $('[data-act="defaults"]', panel).addEventListener('click', () => {
    flush();
    invoke('settings_update', { patch: { teleprompter: Object.fromEntries(RESETTABLE.map((k) => [k, null])) } })
      .then((all) => {
        Object.assign(s, all.teleprompter);
        apply();
        layout();
        openPanel();
      })
      .catch(() => {});
  });

  document.addEventListener('pointerdown', (e) => {
    if (!panel.hidden && !panel.contains(e.target) && !settingsBtn.contains(e.target)) closePanel();
  });

  // ---- Controls ------------------------------------------------------------------
  // Buttons never take the keyboard focus: keys keep driving the show.
  root.addEventListener('mousedown', (e) => {
    if (e.target.closest('button')) e.preventDefault();
  });

  bar.addEventListener('click', (e) => {
    const act = e.target.closest('button')?.dataset.act;
    if (act === 'play') playing ? stop() : play();
    else if (act === 'restart') {
      stop();
      pos = 0;
      render();
    } else if (act === 'slower') changeSpeed(-1);
    else if (act === 'faster') changeSpeed(1);
    else if (act === 'settings') panel.hidden ? openPanel() : closePanel();
    else if (act === 'hide') show(false);
  });

  stage.addEventListener(
    'wheel',
    (e) => {
      e.preventDefault();
      const d = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaMode === 2 ? e.deltaY * stage.clientHeight : e.deltaY;
      pos = clamp(pos + d, 0, max);
      render();
    },
    { passive: false },
  );

  /** Moves the box (from its bar) or resizes it (from an edge or corner). */
  function startDrag(e, dir) {
    if (e.button !== 0) return;
    e.preventDefault();
    closePanel();
    const handle = e.currentTarget;
    handle.setPointerCapture(e.pointerId);
    const x0 = e.clientX;
    const y0 = e.clientY;
    const r0 = boxRect(s, innerWidth, innerHeight);
    root.classList.add('moving');
    const move = (ev) => {
      const r = dragRect(r0, dir, ev.clientX - x0, ev.clientY - y0, innerWidth, innerHeight);
      Object.assign(s, { x: r.left / innerWidth, y: r.top / innerHeight, width: r.width / innerWidth, height: r.height / innerHeight });
      layout();
    };
    const up = () => {
      handle.removeEventListener('pointermove', move);
      handle.removeEventListener('pointerup', up);
      handle.removeEventListener('pointercancel', up);
      root.classList.remove('moving');
      save({ x: s.x, y: s.y, width: s.width, height: s.height });
    };
    handle.addEventListener('pointermove', move);
    handle.addEventListener('pointerup', up);
    handle.addEventListener('pointercancel', up);
  }

  bar.addEventListener('pointerdown', (e) => {
    if (!e.target.closest('button')) startDrag(e, null);
  });
  root.querySelectorAll('.tp-h').forEach((h) => h.addEventListener('pointerdown', (e) => startDrag(e, h.dataset.dir)));

  addEventListener('resize', layout);
  if (typeof ResizeObserver === 'function') new ResizeObserver(() => measure()).observe(text);

  updatePlay();
  apply();
  show(!!s.enabled);

  return {
    setSlide,
    toggle: () => show(root.hidden),
    playPause: () => (root.hidden ? undefined : playing ? stop() : play()),
    faster: () => changeSpeed(1),
    slower: () => changeSpeed(-1),
    /** Closes the settings panel; false when it was not open. */
    closePanel,
  };
}
