// Equation editing is separate from the generic element property panels.
import { html, useEffect, useLayoutEffect, useRef, useState } from './lib.js';
import { state, apply } from './store.js';
import { Section, Row, Hint, Segmented, TextField, NumberField, ColorField, Toggle } from './controls.js';
import { t } from './i18n.js';
import { createMathEditor } from './math-editor.js';
import { MathLiveEditor } from './mathlive-editor.js';

export function MathPanel({ els }) {
  const key = `${state.doc?.docId}:${state.slideId}:${els.map((e) => e.id).join(':')}`;
  return html`<${EquationPanel} key=${key} els=${els} />`;
}

function EquationPanel({ els }) {
  const single = els.length === 1 ? els[0] : null;
  const target = useRef({ docId: state.doc?.docId, slide: state.slideId, ids: els.map((e) => e.id) }).current;
  const [mode, setMode] = useState('visual');
  const [status, setStatus] = useState({ draft: single?.content.latex || '', error: null });
  const editor = useRef(null), ref = useRef(null), gesture = useRef(0);
  if (single && !editor.current) editor.current = createMathEditor({ ...target, id: single.id }, setStatus);
  useLayoutEffect(() => { if (single) editor.current.sync(single.content.latex); }, [single?.content.latex]);
  useEffect(() => () => editor.current?.dispose(), []);
  useLayoutEffect(() => {
    const host = ref.current;
    const start = (event) => { if (event.target.tagName === 'TEXTAREA') editor.current?.beginComposition(); };
    const end = (event) => {
      if (event.target.tagName === 'TEXTAREA') editor.current?.endComposition(event.target.value, historyKey('latex'));
    };
    host.addEventListener('compositionstart', start);
    host.addEventListener('compositionend', end);
    return () => {
      host.removeEventListener('compositionstart', start);
      host.removeEventListener('compositionend', end);
    };
  }, []);
  useEffect(() => {
    const focus = () => ref.current?.querySelector('math-field, textarea, input')?.focus();
    window.addEventListener('cz-focus-primary', focus);
    return () => window.removeEventListener('cz-focus-primary', focus);
  }, []);
  const c = (key) => els.every((el) => el.content[key] === els[0].content[key]) ? els[0].content[key] : undefined;
  const historyKey = (key) => `math:${target.ids.join(',')}:${key}:${gesture.current}`;
  const setLatex = (value, type = '') => {
    const history = type === 'historyUndo' || type === 'historyRedo';
    if (history) gesture.current++;
    return editor.current.setDraft(value, history ? null : historyKey('latex'));
  };
  const set_ = (patch, key, live) => apply(() => {
    const slide = state.doc?.deck.slides.find((s) => s.id === target.slide);
    const updates = target.ids.filter((id) => slide?.elements.some((e) => e.id === id && e.content.kind === 'math'))
      .map((id) => ({ id, patch: { content: patch } }));
    return updates.length ? { op: 'updateElements', slide: target.slide, updates } : null;
  }, { docId: target.docId, coalesce: live ? historyKey(key) : null, navigate: false });
  return html`<${Section} title=${t('Equation')} id="math">
    <div ref=${ref} class="math-editor" onFocusCapture=${() => { gesture.current++; }}>
      ${single && html`<${Row}><${Segmented} value=${mode} full options=${[
        { value: 'visual', label: t('Visual') }, { value: 'latex', label: t('LaTeX') },
      ]} onChange=${setMode} /><//>
      ${mode === 'visual'
        ? html`<${MathLiveEditor} value=${status.draft} onInput=${setLatex}
            onGesture=${() => { gesture.current++; }}
            onCompositionStart=${() => editor.current.beginComposition()}
            onCompositionEnd=${(value) => editor.current.endComposition(value, historyKey('latex'))} />`
        : html`<div>
            <${TextField} multiline mono rows=${4} live value=${status.draft}
              placeholder=${t('LaTeX, e.g. {example}', { example: '\\frac{a}{b}' })} onChange=${(value) => setLatex(value)} />
          </div>`}
      ${status.error && html`<p class="math-editor-error" role="alert">${status.error}</p>`}`}
      <${Hint}>${t('Write LaTeX — rendered with KaTeX. Example:')} <code>\\int_0^1 x^2\\,dx = \\tfrac13</code><//>
      <${Row} label=${t('Size')}><${NumberField} value=${c('fontSize')} min=${6} max=${400} unit="px" onChange=${(v, live) => set_({ fontSize: v }, 'fs', live)} /><//>
      <${Row} label=${t('Color')}><${ColorField} value=${c('color') ?? ''} noneLabel=${t('Theme text color')} onChange=${(v, live) => set_({ color: v }, 'color', live)} /><//>
      <${Row}><${Toggle} value=${!!c('fit')} label=${t('Fit box to equation')} onChange=${(v) => set_({ fit: v }, 'fit')} /><//>
      <${Row} label=${t('Align')}><${Segmented} value=${c('align')} options=${[
        { value: 'left', icon: 'text-align-start', title: t('Left') },
        { value: 'center', icon: 'text-align-center', title: t('Center') },
        { value: 'right', icon: 'text-align-end', title: t('Right') },
      ]} onChange=${(v) => set_({ align: v }, 'align')} /><//>
    </div>
  <//>`;
}
