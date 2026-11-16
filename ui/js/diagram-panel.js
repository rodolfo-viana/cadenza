// Diagram-specific editing stays outside the general element inspector panels.
import { html, useEffect, useRef, useState } from './lib.js';
import { state, updateElements } from './store.js';
import { Section, Row, Hint, Select, Segmented, TextField } from './controls.js';
import { Button } from './ui.js';
import { opener } from './api.js';
import { t, N_ } from './i18n.js';
import { svgAspect, DIAGRAM_TEMPLATES, diagramInputs, diagramPreviewCurrent } from './diagrams.js';
import { parseVisualDiagram, visualDiagramSource } from './diagram-visual.js';
import { createDiagramEditor } from './diagram-edit.js';
import { DiagramGallery, VisualDiagramEditor } from './visual-diagram-editor.js';

const STYLES = [
  { value: 'classic', label: N_('Classic') }, { value: 'neutral', label: N_('Neutral') },
  { value: 'forest', label: N_('Forest') }, { value: 'neo', label: N_('Neo') },
  { value: 'redux', label: N_('Redux') }, { value: 'redux-color', label: N_('Redux (color)') },
];

// Key the session to its actual document target, not just the current selection.
export function DiagramPanel({ els }) {
  return html`<${DiagramEditorPanel} key=${`${state.doc?.docId}:${state.slideId}:${els[0].id}`} el=${els[0]} />`;
}

function DiagramEditorPanel({ el }) {
  const d = el.content;
  const ref = useRef();
  const gesture = useRef(0);
  const [mode, setMode] = useState(() => parseVisualDiagram(d.source) ? 'visual' : 'mermaid');
  const [status, setStatus] = useState(() => ({ draft: diagramInputs(d), busy: false, error: null }));
  const editor = useRef(null);
  if (!editor.current) editor.current = createDiagramEditor({ docId: state.doc?.docId, slide: state.slideId, id: el.id }, setStatus);
  useEffect(() => () => editor.current.dispose(), []);
  useEffect(() => editor.current.sync(d), [d.source, d.style, d.look]);
  useEffect(() => {
    const focus = () => ref.current?.querySelector('textarea, input')?.focus();
    window.addEventListener('cz-focus-primary', focus);
    return () => window.removeEventListener('cz-focus-primary', focus);
  }, []);
  const update = (patch, live) => editor.current.update(patch, live ? `diagram:${el.id}:${gesture.current}` : null);
  const graph = parseVisualDiagram(status.draft.source);
  const fit = () => {
    const aspect = svgAspect(d.svgLight || d.svgDark);
    if (aspect) updateElements([el.id], { h: Math.max(20, Math.round(el.w / aspect)) });
  };
  const chooseTemplate = (next) => { setMode('visual'); update({ source: visualDiagramSource(next) }); };
  return html`<${Section} title=${t('Diagram')} id="diagram">
    <div ref=${ref} class="diagram-editor" onFocusCapture=${() => { gesture.current++; }}>
      <${Row}><${Segmented} value=${mode} full options=${[
        { value: 'visual', label: t('Visual') }, { value: 'mermaid', label: t('Mermaid') },
      ]} onChange=${setMode} /><//>
      ${mode === 'visual'
        ? graph
          ? html`<${VisualDiagramEditor} graph=${graph} onChange=${(next, live) => update({ source: visualDiagramSource(next) }, live)} />`
          : html`<${Hint}>${t('Visual editing supports simple flowcharts. This source uses another diagram type or additional Mermaid syntax. Edit it in Mermaid to preserve every detail.')}<//>
              <${Button} size="sm" label=${t('Edit Mermaid source')} onClick=${() => setMode('mermaid')} />`
        : html`<div class="diagram-source">
            <${TextField} multiline mono rows=${12} live value=${status.draft.source} onChange=${(source, live) => update({ source }, live)} />
          </div>
          <${Hint}>${t('Write Mermaid syntax: flowcharts, sequence, class, state, ER, Gantt, pie, mindmaps, timelines and more.')}
            ${' '}<a href="#" onClick=${(e) => { e.preventDefault(); opener.openUrl('https://mermaid.js.org/intro/'); }}>${t('Learn more')}</a><//>`}
      ${status.busy && html`<div class="diagram-render-status" role="status"><span class="spinner small" />${t('Rendering…')}</div>`}
      ${status.error && html`<pre class="diagram-error" role="alert">${status.error.replace(/^Error:\s*/, '').split('\n').slice(0, 6).join('\n')}</pre>`}
      ${!status.busy && !diagramPreviewCurrent(d) && (d.svgLight || d.svgDark) && html`<${Hint}>${t('The slide is showing the last rendered diagram. Its drawing does not yet match the current source.')}<//>`}
      <${Row} label=${t('Style')}><${Select} value=${status.draft.style} options=${STYLES.map((o) => ({ ...o, label: t(o.label) }))} onChange=${(style) => update({ style })} /><//>
      <${Row} label=${t('Look')}><${Segmented} value=${status.draft.look} options=${[
        { value: 'classic', label: t('Classic') }, { value: 'handDrawn', label: t('Sketch') }, { value: 'neo', label: t('Neo') },
      ]} onChange=${(look) => update({ look })} /><//>
      <${Row} label=${t('Colors')}><${Segmented} value=${d.mode} options=${[
        { value: 'auto', label: t('Auto') }, { value: 'light', label: t('Light') }, { value: 'dark', label: t('Dark') },
      ]} onChange=${(mode) => updateElements([el.id], { content: { mode } })} /><//>
      <div class="row-inline"><${Button} size="sm" icon="ratio" label=${t('Fit Box to Diagram')} onClick=${fit} /></div>
      <h4 class="diagram-subheading">${t('Replace with a template')}</h4>
      <${DiagramGallery} onChoose=${chooseTemplate} />
      <${Row} label=${t('Mermaid templates')}><${Select} value="" options=${[
        { value: '', label: t('Start from a template…') }, ...DIAGRAM_TEMPLATES.map(([name], i) => ({ value: String(i), label: t(name) })),
      ]} onChange=${(value) => {
        if (value !== '') { setMode('mermaid'); update({ source: DIAGRAM_TEMPLATES[Number(value)][1] }); }
      }} /><//>
    </div>
  <//>`;
}
