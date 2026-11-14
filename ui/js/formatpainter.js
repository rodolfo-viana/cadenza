import { html } from './lib.js';
import * as S from './store.js';
import { captureTextFormat, paintTextFormat, textSelected, endAllEditing } from './textedit.js';
import { Button } from './ui.js';
import { t } from './i18n.js';

const BOX = ['fill', 'borderColor', 'borderWidth', 'borderStyle', 'radius', 'shadow', 'shadowColor', 'padding'];
const TEXT = [
  'role',
  'fontFamily',
  'fontSize',
  'fontWeight',
  'italic',
  'upright',
  'color',
  'align',
  'valign',
  'lineHeight',
  'letterSpacing',
  'transform',
  'paragraphSpacing',
  'shadow',
];
const IMAGE = ['grayscale', 'blur', 'brightness', 'contrast', 'saturate'];
const pick = (source, keys) => Object.fromEntries(keys.filter((k) => source && source[k] !== undefined).map((k) => [k, source[k]]));
const rich = (el) => (['text', 'shape'].includes(el?.content.kind) ? S.richOf(el) : null);

export function formatSnapshot(el, characters = null) {
  return {
    scale: el.contentScale || 1,
    box: characters ? null : pick(el.style, BOX),
    text: rich(el) ? (characters ? {} : pick(rich(el).style, TEXT)) : null,
    image: el.content.kind === 'image' ? pick(el.content, IMAGE) : null,
    characters,
    textOnly: !!characters,
  };
}

function scaleCharacters(characters, ratio) {
  if (!characters) return null;
  const out = { ...characters };
  for (const key of ['font-size', 'letter-spacing']) {
    if (/^-?[\d.]+px$/.test(out[key] || '')) out[key] = `${parseFloat(out[key]) * ratio}px`;
  }
  return out;
}

function scaleLengths(style, keys, ratio) {
  const out = { ...style };
  for (const key of keys) if (typeof out[key] === 'number') out[key] *= ratio;
  return out;
}

/** Replace visual inline overrides without touching text, links or lists. */
export function paintHtml(source, characters, paragraphs = false) {
  const root = document.createElement('div');
  root.innerHTML = source;
  for (const el of [...root.querySelectorAll('*')]) {
    if (el.style)
      for (const key of [
        'font-family',
        'font-size',
        'font-weight',
        'font-style',
        'color',
        'background-color',
        'text-decoration',
        'text-decoration-line',
        'letter-spacing',
        'text-transform',
        'text-shadow',
      ])
        el.style.removeProperty(key);
    if (paragraphs)
      for (const key of ['text-align', 'line-height', 'text-indent', 'margin-top', 'margin-bottom', 'margin-left', 'margin-right'])
        el.style?.removeProperty(key);
    if (['B', 'STRONG', 'I', 'EM', 'U', 'S', 'STRIKE', 'DEL', 'MARK', 'SMALL', 'FONT'].includes(el.tagName)) el.replaceWith(...el.childNodes);
  }
  if (characters) {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT),
      nodes = [];
    let n;
    while ((n = walker.nextNode())) nodes.push(n);
    for (const node of nodes) {
      const span = document.createElement('span');
      for (const [k, v] of Object.entries(characters)) span.style.setProperty(k, v);
      node.replaceWith(span);
      span.appendChild(node);
    }
  }
  return root.innerHTML;
}

export function formatPatch(target, snapshot) {
  const patch = {};
  const ratio = (snapshot.scale || 1) / (target.contentScale || 1);
  if (!snapshot.textOnly && snapshot.box) patch.style = scaleLengths(snapshot.box, ['borderWidth', 'radius', 'padding'], ratio);
  const targetText = rich(target);
  if (targetText && snapshot.text) {
    const text = {
      html: paintHtml(targetText.html, scaleCharacters(snapshot.characters, ratio), !snapshot.textOnly),
      style: scaleLengths(snapshot.text, ['fontSize'], ratio),
    };
    // Auto height is a geometry choice of the target, not a source format.
    Object.assign(patch, S.richPatch(target, text));
  }
  if (!snapshot.textOnly && snapshot.image && target.content.kind === 'image') patch.content = scaleLengths(snapshot.image, ['blur'], ratio);
  return patch;
}

export async function copyFormat(activate = true) {
  const el = S.selectedElements()[0],
    docId = S.state.doc?.docId;
  if (!el || S.selectedElements().length !== 1) return;
  const characters = captureTextFormat();
  if (!(await S.flushEdits()) || S.state.doc?.docId !== docId) return;
  const current = S.currentSlide()?.elements.find((e) => e.id === el.id);
  if (!current) return;
  // A whole text box uses its first visible character as the source style.
  let inline = characters;
  if (!inline && rich(current)) {
    const { canvasApi } = await import('./canvas.js');
    const root = canvasApi.view?.node(el.id)?.querySelector('.cz-text');
    if (root) {
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      const first = walker.nextNode()?.parentElement || root;
      const css = getComputedStyle(first);
      inline = Object.fromEntries(
        ['font-family', 'font-size', 'font-weight', 'font-style', 'color', 'text-decoration-line', 'letter-spacing'].map((k) => [k, css.getPropertyValue(k)]),
      );
    }
  }
  if (S.state.doc?.docId !== docId) return;
  const snapshot = formatSnapshot(current, characters);
  snapshot.characters = inline;
  S.set({ formatPainter: { docId, snapshot, active: activate, repeat: false } });
}

export async function applyFormat(ids = S.state.selection, selection = true) {
  const brush = S.state.formatPainter;
  if (!brush || S.state.doc?.docId !== brush.docId) return;
  if (selection && textSelected() && brush.snapshot.characters) {
    const ratio = (brush.snapshot.scale || 1) / (S.selectedElements()[0]?.contentScale || 1);
    if (paintTextFormat(scaleCharacters(brush.snapshot.characters, ratio)) && !brush.repeat) S.set({ formatPainter: { ...brush, active: false } });
    return;
  }
  const slide = S.state.slideId;
  if (!(await endAllEditing()) || S.state.doc?.docId !== brush.docId) return;
  const result = await S.apply(
    () => {
      const elements = S.deck()?.slides.find((s) => s.id === slide)?.elements || [];
      const updates = elements
        .filter((e) => ids.includes(e.id) && !e.locked)
        .map((e) => ({ id: e.id, patch: formatPatch(e, brush.snapshot) }))
        .filter((u) => Object.keys(u.patch).length);
      return updates.length ? { op: 'paintFormat', slide, updates } : null;
    },
    { docId: brush.docId, select: false },
  );
  if (result && !brush.repeat && S.state.formatPainter === brush) S.set({ formatPainter: { ...brush, active: false } });
}

export function PainterBar() {
  const brush = S.state.formatPainter;
  if (!brush?.active) return null;
  return html`<div class="painter-bar" role="status">
    <span>${t('Format Painter: click an object to apply. Escape cancels.')}</span>
    <label
      ><input type="checkbox" checked=${brush.repeat} onChange=${(e) => S.set({ formatPainter: { ...brush, repeat: e.target.checked } })} /> ${t(
        'Keep applying',
      )}</label
    >
    <${Button} label=${t('Cancel')} onClick=${() => S.set({ formatPainter: { ...brush, active: false } })} />
  </div>`;
}
