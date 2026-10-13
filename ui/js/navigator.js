// Slide navigator: live thumbnails, multi-selection, drag-to-reorder and
// the slide layout picker.

import { html, useLayoutEffect, useRef, useState, useEffect } from './lib.js';
import { Icon } from './icons.js';
import { SlideView } from './slideview.js';
import { state, set, useStore, goToSlide, openMenu, play } from './store.js';
import * as A from './actions.js';
import { item } from './commands.js';
import { Popover } from './controls.js';
import { cls } from './util.js';
import { t, tn } from './i18n.js';

const THUMB_W = 164;

/** Renders one slide at a fixed width (also used by the layout picker). */
export function SlidePreview({ render, deck, width, class: extra }) {
  const ref = useRef();
  const view = useRef(null);
  useLayoutEffect(() => {
    if (!view.current) view.current = new SlideView(ref.current);
    view.current.update(render?.html || '');
  }, [render?.html]);
  const s = width / deck.width;
  return html`<div class=${cls('preview-frame reveal-viewport', extra)} style=${{ width: width + 'px', height: Math.round(deck.height * s) + 'px' }}>
    <div class="reveal cz-editor-reveal" style=${{ width: deck.width + 'px', height: deck.height + 'px', transform: `scale(${s})` }}>
      <div class="slides">
        <section class=${cls('present', render?.classes)} style=${{ width: deck.width + 'px', height: deck.height + 'px' }}>
          <div class="cz-slide" ref=${ref} style=${{ width: deck.width + 'px', height: deck.height + 'px' }} />
        </section>
      </div>
    </div>
  </div>`;
}

export function Navigator() {
  const st = useStore();
  const deck = st.doc.deck;
  const slides = deck.slides;
  const listRef = useRef();
  const anchor = useRef(null);
  const [drop, setDrop] = useState(null);

  // Keep the current slide visible.
  useEffect(() => {
    const el = listRef.current?.querySelector(`[data-slide="${st.slideId}"]`);
    el?.scrollIntoView({ block: 'nearest' });
  }, [st.slideId]);

  function selectRange(to) {
    const from = slides.findIndex((s) => s.id === (anchor.current || st.slideId));
    const [a, b] = from < to ? [from, to] : [to, from];
    set({ selectedSlides: slides.slice(a, b + 1).map((s) => s.id), focus: 'slides' });
    goToSlide(slides[to].id, { keepSelection: true });
  }

  function dropIndexAt(y) {
    const thumbs = [...listRef.current.querySelectorAll('.thumb')];
    for (let i = 0; i < thumbs.length; i++) {
      const r = thumbs[i].getBoundingClientRect();
      if (y < r.top + r.height / 2) return i;
    }
    return thumbs.length;
  }

  function onThumbDown(e, slide, index) {
    if (e.button !== 0) return;
    listRef.current.focus({ preventScroll: true });
    if (e.shiftKey) return selectRange(index);
    if (e.ctrlKey || e.metaKey) {
      const sel = st.selectedSlides.includes(slide.id) ? st.selectedSlides.filter((x) => x !== slide.id) : [...st.selectedSlides, slide.id];
      set({ selectedSlides: sel.length ? sel : [slide.id], focus: 'slides' });
      anchor.current = slide.id;
      return;
    }
    const wasSelected = st.selectedSlides.includes(slide.id);
    if (!wasSelected) {
      goToSlide(slide.id);
      set({ focus: 'slides' });
    } else {
      goToSlide(slide.id, { keepSelection: true });
      set({ focus: 'slides' });
    }
    anchor.current = slide.id;
    const ids = wasSelected ? slides.filter((s) => state.selectedSlides.includes(s.id)).map((s) => s.id) : [slide.id];
    const y0 = e.clientY;
    let dragging = false;
    const move = (ev) => {
      if (!dragging && Math.abs(ev.clientY - y0) < 5) return;
      dragging = true;
      document.body.classList.add('dragging-slides');
      setDrop(dropIndexAt(ev.clientY));
      const r = listRef.current.getBoundingClientRect();
      if (ev.clientY < r.top + 30) listRef.current.scrollTop -= 12;
      if (ev.clientY > r.bottom - 30) listRef.current.scrollTop += 12;
    };
    const up = (ev) => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      document.body.classList.remove('dragging-slides');
      setDrop(null);
      if (dragging) {
        A.moveSlides(ids, dropIndexAt(ev.clientY));
      } else if (wasSelected && st.selectedSlides.length > 1) {
        set({ selectedSlides: [slide.id] });
      }
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }

  function onKeyDown(e) {
    const i = slides.findIndex((s) => s.id === st.slideId);
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      e.stopPropagation();
      const j = Math.max(0, Math.min(slides.length - 1, i + (e.key === 'ArrowDown' ? 1 : -1)));
      if (e.shiftKey) selectRange(j);
      else {
        anchor.current = slides[j].id;
        goToSlide(slides[j].id);
      }
    } else if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      e.stopPropagation();
      goToSlide(slides[e.key === 'Home' ? 0 : slides.length - 1].id);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      e.stopPropagation();
      A.addSlide();
    } else if (e.key === 'Tab') {
      e.preventDefault();
      e.stopPropagation();
      A.setNested(!e.shiftKey);
    }
  }

  function onContextMenu(e, slide) {
    e.preventDefault();
    e.stopPropagation();
    if (slide && !st.selectedSlides.includes(slide.id)) goToSlide(slide.id);
    set({ focus: 'slides' });
    const sel = deck.slides.filter((s) => state.selectedSlides.includes(s.id));
    const allHidden = sel.length && sel.every((s) => s.hidden);
    const allNested = sel.length && sel.every((s) => s.nested);
    openMenu(e.clientX, e.clientY, [
      item('slide.new'),
      { label: t('New Slide with Layout'), icon: 'layout-template', children: st.info.layouts.map((l) => ({ label: t(l.name), run: () => A.addSlide(l.id) })) },
      '-',
      { label: t('Copy'), icon: 'copy', keys: 'Mod+C', run: () => document.execCommand('copy') },
      { label: t('Paste'), icon: 'clipboard-paste', keys: 'Mod+V', run: A.pasteFromMenu },
      item('slide.duplicate'),
      item('slide.delete', { danger: true, label: sel.length > 1 ? tn('Delete {n} Slide', 'Delete {n} Slides', sel.length) : t('Delete Slide') }),
      '-',
      { label: allHidden ? t('Don’t Skip Slide') : t('Skip Slide'), icon: allHidden ? 'eye' : 'eye-off', run: () => A.toggleSkip() },
      allNested
        ? { label: t('Make Top-level Slide'), icon: 'indent-decrease', keys: 'Shift+Tab', run: () => A.setNested(false) }
        : { label: t('Make Vertical Sub-slide'), icon: 'indent-increase', keys: 'Tab', run: () => A.setNested(true) },
      '-',
      { label: t('Play from This Slide'), icon: 'play', run: () => play(true) },
    ]);
  }

  let number = 0;
  return html`<aside class="navigator" onContextMenu=${(e) => onContextMenu(e, null)}>
    <div class="nav-list" ref=${listRef} tabIndex="0" onKeyDown=${onKeyDown} onFocus=${() => set({ focus: 'slides' })}>
      ${slides.map((slide, i) => {
        number += 1;
        const sel = st.selectedSlides.includes(slide.id);
        return html`<div
          key=${slide.id}
          data-slide=${slide.id}
          class=${cls('thumb', sel && 'selected', slide.id === st.slideId && 'current', slide.hidden && 'skipped', slide.nested && 'nested', drop === i && 'drop-before', drop === slides.length && i === slides.length - 1 && 'drop-after')}
          onPointerDown=${(e) => onThumbDown(e, slide, i)}
          onDblClick=${() => play(true)}
          onContextMenu=${(e) => onContextMenu(e, slide)}
        >
          <div class="thumb-num">${number}</div>
          <div class="thumb-body">
            <${SlidePreview} render=${st.doc.slides[slide.id]} deck=${deck} width=${slide.nested ? THUMB_W - 16 : THUMB_W} />
            <div class="thumb-badges">
              ${slide.hidden && html`<span title=${t('Skipped')}><${Icon} name="eye-off" size=${12} /></span>`}
              ${slide.autoAnimate && html`<span title=${t('Magic Move')}><${Icon} name="wand-sparkles" size=${12} /></span>`}
              ${slide.elements.some((e) => e.build.effect || e.build.outEffect) && html`<span title=${t('Has builds')}><${Icon} name="sparkles" size=${12} /></span>`}
              ${slide.notes.trim() && html`<span title=${t('Has speaker notes')}><${Icon} name="pen-line" size=${12} /></span>`}
            </div>
          </div>
        </div>`;
      })}
    </div>
    <div class="nav-foot">
      <${NewSlideButton} />
    </div>
  </aside>`;
}

// ---------------------------------------------------------------------------
// New slide + layout picker
// ---------------------------------------------------------------------------

export function NewSlideButton({ compact }) {
  const ref = useRef();
  const [open, setOpen] = useState(false);
  return html`<div class=${cls('split', compact && 'compact')} ref=${ref}>
    <button type="button" class="split-main" title=${t('New slide ({shortcut})', { shortcut: 'Ctrl+M' })} onClick=${() => A.addSlide()}>
      <${Icon} name="plus" size=${16} /><span>${t('New Slide')}</span>
    </button>
    <button type="button" class="split-caret" title=${t('Choose a layout')} onClick=${() => setOpen(!open)}>
      <${Icon} name="chevron-down" size=${14} />
    </button>
    ${open &&
    html`<${Popover} anchor=${ref.current} onClose=${() => setOpen(false)} width=${396} class="layout-pop">
      <div class="layout-grid">
        ${state.info.layouts.map(
          (l) => html`<button
            type="button"
            class="layout-opt"
            onClick=${() => {
              setOpen(false);
              A.addSlide(l.id);
            }}
          >
            <${LayoutIcon} id=${l.id} />
            <span>${t(l.name)}</span>
          </button>`,
        )}
      </div>
    <//>`}
  </div>`;
}

/** Schematic preview of a layout. */
export function LayoutIcon({ id }) {
  const T = (x, y, w, h = 7) => html`<rect x=${x} y=${y} width=${w} height=${h} rx="1.5" class="li-title" />`;
  const L = (x, y, w) => html`<rect x=${x} y=${y} width=${w} height="3" rx="1.5" class="li-line" />`;
  const lines = (x, y, w, n) => Array.from({ length: n }, (_, i) => L(x, y + i * 7, i === n - 1 ? w * 0.6 : w));
  const IMG = (x, y, w, h) => html`<rect x=${x} y=${y} width=${w} height=${h} rx="2" class="li-img" /><path d=${`M${x + w * 0.2} ${y + h * 0.75} l${w * 0.2} -${h * 0.3} l${w * 0.15} ${h * 0.15} l${w * 0.15} -${h * 0.2} l${w * 0.15} ${h * 0.35}`} class="li-imgline" />`;
  const parts = {
    title: [T(20, 22, 56, 9), L(30, 38, 36)],
    'title-content': [T(8, 8, 50), ...lines(8, 22, 76, 4)],
    section: [L(8, 22, 20), T(8, 29, 64, 9)],
    'two-column': [T(8, 8, 50), ...lines(8, 22, 36, 4), ...lines(52, 22, 36, 4)],
    'image-text': [IMG(0, 0, 46, 54), T(52, 14, 36, 6), ...lines(52, 26, 36, 3)],
    'big-image': [IMG(0, 0, 96, 54), L(6, 46, 30)],
    statement: [T(16, 20, 64, 7), T(28, 30, 40, 7)],
    quote: [T(14, 18, 68, 6), T(22, 27, 52, 6), L(36, 40, 24)],
    'big-number': [html`<text x="48" y="34" text-anchor="middle" class="li-num">100%</text>`, L(28, 42, 40)],
    code: [T(8, 8, 50), html`<rect x="8" y="20" width="80" height="28" rx="2" class="li-code" />`, L(13, 26, 30), L(17, 32, 40), L(17, 38, 26)],
    'title-only': [T(8, 8, 50)],
    blank: [],
  };
  return html`<svg class="layout-icon" viewBox="0 0 96 54">${parts[id] || []}</svg>`;
}
