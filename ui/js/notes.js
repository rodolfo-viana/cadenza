// Speaker notes for the current slide.

import { html, useEffect, useRef, useState } from './lib.js';
import { Icon } from './icons.js';
import { state, currentSlide, apply, registerFlush, updateSettings, useStore } from './store.js';
import { t } from './i18n.js';

export function NotesPanel() {
  useStore();
  const slide = currentSlide();
  const [text, setText] = useState(slide?.notes || '');
  const [height, setHeight] = useState(() => {
    try {
      return Number(localStorage.getItem('cz-notes-h')) || 140;
    } catch {
      return 140;
    }
  });
  const pending = useRef(null);
  const timer = useRef(null);
  const ref = useRef();

  // Stable flush: commits typed notes for the slide they belong to.
  const flush = useRef(async () => {
    clearTimeout(timer.current);
    while (pending.current) {
      const p = pending.current;
      // Notes of a slide deleted meanwhile (e.g. by the assistant) are dropped
      // rather than retried forever, which would block saving and closing.
      if (p.docId === state.doc?.docId && state.doc.deck.slides.some((s) => s.id === p.id)) {
        const out = await apply({ op: 'updateSlides', ids: [p.id], patch: { notes: p.value } }, { docId: p.docId, coalesce: `notes:${p.id}`, navigate: false });
        if (!out) return false;
      }
      if (pending.current === p) pending.current = null;
    }
    return true;
  }).current;

  useEffect(() => registerFlush(flush), []);

  useEffect(() => {
    flush();
    setText(slide?.notes || '');
  }, [slide?.id]);

  // Reflect external changes (undo/redo) when not typing.
  useEffect(() => {
    if (!pending.current && document.activeElement !== ref.current) setText(slide?.notes || '');
  }, [slide?.notes]);

  if (!slide) return null;

  const onResize = (e) => {
    const y0 = e.clientY;
    const h0 = height;
    let h = h0;
    const move = (ev) => {
      h = Math.max(70, Math.min(innerHeight * 0.6, h0 - (ev.clientY - y0)));
      setHeight(h);
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      try {
        localStorage.setItem('cz-notes-h', String(Math.round(h)));
      } catch {
        /* ignore */
      }
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  return html`<div class="notes-panel" style=${{ height: height + 'px' }}>
    <div class="notes-resize" onPointerDown=${onResize} />
    <div class="notes-head">
      <${Icon} name="pen-line" size=${14} /> ${t('Speaker notes')}
      <span class="spacer" />
      <button class="icon-btn" title=${t('Hide notes')} onClick=${() => updateSettings({ showNotes: false })}><${Icon} name="x" size=${14} /></button>
    </div>
    <textarea
      ref=${ref}
      class="notes-text"
      placeholder=${t('Add notes for this slide — only you see them, in the presenter display.')}
      value=${text}
      onInput=${(e) => {
        setText(e.target.value);
        pending.current = { docId: state.doc?.docId, id: slide.id, value: e.target.value };
        clearTimeout(timer.current);
        timer.current = setTimeout(flush, 600);
      }}
      onBlur=${flush}
    />
  </div>`;
}
