// The MathLive custom element is an editor adapter. It never owns a document.
import { html, useEffect, useLayoutEffect, useRef, useState } from './lib.js';
import { Button } from './ui.js';
import { Hint } from './controls.js';
import { t, N_, language } from './i18n.js';
import { opener } from './api.js';

let loading = null;
export function loadMathLive() {
  if (!loading) loading = import('../vendor/mathlive/mathlive.min.mjs').then((module) => {
    const { MathfieldElement } = module;
    MathfieldElement.fontsDirectory = new URL('../vendor/katex/fonts/', import.meta.url).href;
    MathfieldElement.soundsDirectory = null;
    MathfieldElement.keypressVibration = false;
    MathfieldElement.openUrl = (url) => opener.openUrl(url);
    return MathfieldElement;
  }).catch((error) => { loading = null; throw error; });
  return loading;
}

const INSERTS = [
  ['a/b', N_('Fraction'), '\\frac{#0}{#?}'], ['x²', N_('Power'), '#@^{#?}'],
  ['√x', N_('Square root'), '\\sqrt{#0}'], ['∑', N_('Sum'), '\\sum_{#?}^{#?}'],
  ['∫', N_('Integral'), '\\int_{#?}^{#?}'], ['[a b]', N_('Matrix'), '\\begin{bmatrix}#? & #? \\\\ #? & #?\\end{bmatrix}'],
];

export function MathLiveEditor({ value, onInput, onCompositionStart, onCompositionEnd, onGesture }) {
  const host = useRef(null), field = useRef(null);
  const latest = useRef({ value, onInput, onCompositionStart, onCompositionEnd });
  latest.current = { value, onInput, onCompositionStart, onCompositionEnd };
  const emitted = useRef(null);
  const [ready, setReady] = useState(false);
  const [unsupported, setUnsupported] = useState(false);
  const [error, setError] = useState(null);
  const [keyboard, setKeyboard] = useState(false);

  const sync = (mf, latex) => {
    // MathLive may normalize its own string. Never write that normalization
    // back on mount, focus or mode switches, and preserve its caret on echo.
    if (emitted.current === latex) return;
    mf.resetUndo();
    mf.setValue(latex, { silenceNotifications: true });
    emitted.current = latex;
    const errors = mf.errors.length > 0;
    mf.readOnly = errors;
    setUnsupported(errors);
  };

  useLayoutEffect(() => {
    if (field.current) sync(field.current, value);
  }, [value]);

  useEffect(() => {
    let alive = true;
    let cleanup = () => {};
    loadMathLive().then((MathfieldElement) => {
      if (!alive || !host.current?.isConnected) return;
      MathfieldElement.locale = language();
      const mf = new MathfieldElement();
      field.current = mf;
      mf.setAttribute('aria-label', t('Equation editor'));
      mf.mathVirtualKeyboardPolicy = 'manual';
      const input = (event) => {
        if (!alive || event.isComposing) return;
        const latex = mf.getValue('latex-without-placeholders');
        emitted.current = latex;
        latest.current.onInput(latex, event.inputType || event.data || 'insertText');
      };
      const start = () => latest.current.onCompositionStart?.();
      const end = () => {
        const latex = mf.getValue('latex-without-placeholders');
        emitted.current = latex;
        latest.current.onCompositionEnd?.(latex);
      };
      const virtualKeyboard = window.mathVirtualKeyboard;
      const keyboardChanged = () => { if (alive) setKeyboard(virtualKeyboard.visible); };
      mf.addEventListener('input', input);
      mf.addEventListener('compositionstart', start);
      mf.addEventListener('compositionend', end);
      virtualKeyboard.addEventListener('virtual-keyboard-toggle', keyboardChanged);
      host.current.appendChild(mf);
      sync(mf, latest.current.value);
      setReady(true);
      cleanup = () => {
        virtualKeyboard.hide({ animate: false });
        virtualKeyboard.removeEventListener('virtual-keyboard-toggle', keyboardChanged);
        mf.removeEventListener('input', input);
        mf.removeEventListener('compositionstart', start);
        mf.removeEventListener('compositionend', end);
        mf.remove();
        field.current = null;
      };
    }).catch((reason) => { if (alive) setError(String(reason?.message || reason)); });
    return () => { alive = false; cleanup(); };
  }, []);

  return html`<div class="mathlive-editor">
    <div class="mathlive-field" ref=${host} />
    ${!ready && !error && html`<p class="ihint" role="status">${t('Loading equation editor…')}</p>`}
    ${error && html`<p class="math-editor-error" role="alert">${t('The visual editor could not load. You can still edit the LaTeX source.')} ${error}</p>`}
    ${unsupported && html`<${Hint}>${t('This source contains LaTeX that the visual editor cannot interpret. Edit it in the LaTeX tab to preserve its contents.')}<//>`}
    <div class="math-inserts" aria-label=${t('Equation structures')}>
      ${INSERTS.map(([label, title, latex]) => html`<${Button} size="sm" label=${label} title=${t(title)} disabled=${!ready || unsupported}
        onMouseDown=${(e) => e.preventDefault()} onClick=${() => {
          onGesture?.();
          field.current?.insert(latex, { focus: true, selectionMode: 'placeholder' });
        }} />`)}
    </div>
    <${Button} size="sm" icon="keyboard" label=${keyboard ? t('Hide math keyboard') : t('Show math keyboard')}
      disabled=${!ready || unsupported} onMouseDown=${(e) => e.preventDefault()} onClick=${() => {
        field.current?.focus();
        window.mathVirtualKeyboard.visible = !window.mathVirtualKeyboard.visible;
      }} />
  </div>`;
}
