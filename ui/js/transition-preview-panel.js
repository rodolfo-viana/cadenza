import { html, useState, useLayoutEffect, useRef } from './lib.js';
import { state, flushEdits } from './store.js';
import { invoke } from './api.js';
import { Hint } from './controls.js';
import { Button } from './ui.js';
import { t } from './i18n.js';

const PREVIEW_URL = new URL('../transition-preview.html', import.meta.url).href;

/** A disposable, read-only audience renderer, scoped to this document revision. */
export function TransitionPreview({ slide }) {
  const [preview, setPreview] = useState(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const generation = useRef(0);
  const docId = state.doc.docId, revision = state.doc.revision;
  useLayoutEffect(() => {
    generation.current++;
    setPreview(null); setBusy(false); setError('');
    return () => { generation.current++; };
  }, [docId, slide.id, revision]);
  const play = async () => {
    const request = ++generation.current;
    setBusy(true); setError(''); setPreview(null);
    try {
      if (!(await flushEdits())) return;
      const data = await invoke('present_data', { docId });
      if (request !== generation.current || state.doc?.docId !== docId || state.slideId !== slide.id) return;
      setPreview({ data, request });
    } catch (e) {
      if (request === generation.current) setError(String(e));
    } finally {
      if (request === generation.current) setBusy(false);
    }
  };
  const visible = !slide.hidden;
  return html`<div class="transition-preview">
    ${preview && html`<iframe key=${preview.request} title=${t('Transition preview')} src=${PREVIEW_URL}
      sandbox="allow-scripts allow-same-origin" tabindex="-1" style=${{ aspectRatio: `${preview.data.width} / ${preview.data.height}` }}
      onLoad=${async (e) => {
        try { await e.target.contentWindow.CadenzaTransitionPreview.load(preview.data, slide.id, t('Media paused in preview')); }
        catch (err) { if (preview.request === generation.current) { setError(String(err)); setPreview(null); } }
      }} />`}
    <${Button} size="sm" icon="play" label=${busy ? t('Loading…') : t('Preview transition')} disabled=${busy || !visible} onClick=${play} />
    <${Hint}>${visible ? t('Plays from the previous visible slide. Media and camera stay paused.') : t('Hidden slides are not shown in the presentation.')}</${Hint}>
    ${preview?.data.slides[0]?.id === slide.id && html`<${Hint}>${t('The first slide has no incoming transition.')}</${Hint}>`}
    ${error && html`<div role="alert" class="transition-preview-error">${error}</div>`}
  </div>`;
}
