import { html, useState, useRef, useEffect } from './lib.js';
import { state, closeModal, flushEdits } from './store.js';
import { invoke, dialog, opener } from './api.js';
import { Modal, Button } from './ui.js';
import { t } from './i18n.js';
import { ensureDiagrams } from './diagrams.js';

export function ConversionIssues({ issues }) {
  return issues?.length ? html`<div class="conversion-issues" role="status">
    <p>${t('Review these conversion details before relying on the result:')}</p>
    <ul>${issues.map((issue) => html`<li><strong>${issue.slide ? t('Slide {n}', { n: issue.slide }) : t('Presentation')} · ${t(issue.feature)}</strong><br />${t(issue.message)}</li>`)}</ul>
  </div>` : null;
}

export function PptxExport() {
  const [docId] = useState(() => state.doc.docId);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState('');
  const [error, setError] = useState('');
  const [preview, setPreview] = useState(null);
  const [saved, setSaved] = useState(null);
  const active = useRef(null);
  useEffect(() => () => active.current?.abort(), []);
  const run = async (previewOnly) => {
    const controller = new AbortController(); active.current = controller;
    setBusy(true); setError(''); setProgress(t('Preparing the presentation…'));
    let snapshot;
    try {
      const path = previewOnly ? null : await dialog.save({ title: t('Export PowerPoint'), defaultPath: `${state.doc.deck.title || 'Presentation'}.pptx`,
        filters: [{ name: 'PowerPoint', extensions: ['pptx'] }] });
      if ((!previewOnly && !path) || controller.signal.aborted) return;
      if (!(await flushEdits()) || state.doc?.docId !== docId) throw new Error(t('The presentation changed. Start the export again.'));
      await ensureDiagrams();
      if (state.doc?.docId !== docId) throw new Error(t('The presentation changed. Start the export again.'));
      controller.signal.throwIfAborted();
      snapshot = await invoke('pptx_begin_export', { docId });
      const release = () => invoke('pptx_end_export', { jobId: snapshot.jobId }).catch(() => {});
      controller.signal.addEventListener('abort', release, { once: true });
      if (controller.signal.aborted) { await release(); return; }
      const capture = (slideId, options = {}) => {
        controller.signal.throwIfAborted();
        return invoke('pptx_capture', { jobId: snapshot.jobId, slideId, elementId: options.elementId || null, background: !!options.background });
      };
      if (previewOnly) {
        const first = snapshot.deck.slides.find((slide) => !slide.hidden) || snapshot.deck.slides[0];
        if (!first) throw new Error(t('The presentation has no slides.'));
        const image = await capture(first.id);
        if (!controller.signal.aborted) setPreview(image.data);
      } else {
        const { buildPptx } = await import('./pptx-export.js');
        const result = await buildPptx(snapshot, { capture,
          asset: (name) => invoke('pptx_export_asset', { jobId: snapshot.jobId, name }),
          signal: controller.signal, onProgress: ({ phase, completed, total }) => !controller.signal.aborted && setProgress(phase === 'writing'
            ? t('Creating the PowerPoint file…') : t('Preparing slide {n} of {total}…', { n: completed + 1, total })),
        });
        controller.signal.throwIfAborted();
        const encodedPath = btoa(String.fromCharCode(...new TextEncoder().encode(path)));
        const output = await window.__TAURI__.core.invoke('pptx_save', result.bytes, { headers: { 'x-export-id': snapshot.jobId, 'x-export-path': encodedPath } });
        setSaved({ path: output, issues: result.issues });
      }
    } catch (e) {
      if (!controller.signal.aborted) setError(t(e?.message || String(e).replace(/^Error: /, '')));
    } finally {
      if (snapshot) await invoke('pptx_end_export', { jobId: snapshot.jobId }).catch(() => {});
      if (!controller.signal.aborted) { setBusy(false); setProgress(''); }
    }
  };
  const cancel = () => { active.current?.abort(); closeModal(); };
  return html`<${Modal} title=${t('Export PowerPoint')} width=${640} onClose=${cancel}>
    <p>${t('Text, shapes, tables and supported charts stay editable. Other objects keep their appearance as images. Live media and presentation effects are exported as static content.')}</p>
    ${preview && html`<img class="pptx-preview" src=${preview} alt=${t('PowerPoint export preview')} />`}
    ${busy && html`<p role="status">${progress}</p>`}
    ${error && html`<p role="alert" class="pptx-error">${error}</p>`}
    ${saved && html`<p>${t('Saved: {path}', { path: saved.path })}</p><${ConversionIssues} issues=${saved.issues} />`}
    <div class="modal-actions">
      ${saved && html`<${Button} label=${t('Show in folder')} onClick=${() => opener.revealItemInDir(saved.path)} />`}
      <${Button} label=${t('Preview first slide')} disabled=${busy} onClick=${() => run(true)} />
      <${Button} label=${t('Export PowerPoint')} variant="primary" disabled=${busy} onClick=${() => run(false)} />
      <${Button} label=${busy ? t('Cancel') : t('Close')} onClick=${cancel} />
    </div>
  <//>`;
}

export function PptxImportReport({ issues }) {
  return html`<${Modal} title=${t('PowerPoint import details')} width=${640} onClose=${closeModal}>
    <p>${t('The original PowerPoint file is unchanged. Imported objects can be edited and saved as a Cadenza presentation.')}</p>
    <${ConversionIssues} issues=${issues} />
    <div class="modal-actions"><${Button} label=${t('Close')} onClick=${closeModal} /></div>
  <//>`;
}
