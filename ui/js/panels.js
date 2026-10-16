// Inspector panels for elements.

import { html, useEffect, useRef, useState } from './lib.js';
import { Icon } from './icons.js';
import { state, set, apply, updateElements, deck, currentSlide, richOf, richPatch, themeInfo, fitsText, openModal } from './store.js';
import { Section, Row, Hint, NumberField, Slider, Select, Segmented, Toggle, TextField, ColorField, FontField, Cell } from './controls.js';
import * as A from './actions.js';
import { exec, setLink, isEditing, beginTextEdit, beginTableEdit, insertInlineMath, textSelected, styleSelection, selectionStyle, selectionFontSize } from './textedit.js';
import { formatActive, formatBox, toggleBoxes } from './textformat.js';
import { canvasApi } from './canvas.js';
import { cls } from './util.js';
import { Button } from './ui.js';
import { renderDiagram, svgAspect, DIAGRAM_TEMPLATES } from './diagrams.js';
import { opener, invoke } from './api.js';
import { t, N_, language } from './i18n.js';
import { CHART_ICONS, MAP_AREAS, MARKERS, ERROR_MODES, chartKind, errorColumns, hasAxes, hasErrorBars, hasMarkers, isSample, linkChart, oneSeries, switchChart } from './charts.js';

const ids = (els) => els.map((e) => e.id).join(',');

/** Patches every selected element; `patch` may be a function of the element. */
function patchAll(els, patch, key) {
  return updateElements(
    els.map((e) => e.id),
    (id) => {
      const el = els.find((e) => e.id === id);
      return typeof patch === 'function' ? patch(el) : patch;
    },
    key && `${key}:${ids(els)}`,
  );
}

const same = (els, get) => {
  const v = get(els[0]);
  return els.every((e) => JSON.stringify(get(e)) === JSON.stringify(v)) ? v : undefined;
};

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

const ROLES = [
  { value: 'title', label: N_('Title'), size: 56 },
  { value: 'subtitle', label: N_('Subtitle'), size: 36 },
  { value: 'heading', label: N_('Heading'), size: 44 },
  { value: 'body', label: N_('Body'), size: 32 },
  { value: 'caption', label: N_('Caption'), size: 22 },
  { value: 'quote', label: N_('Quote'), size: 44 },
];

export function TextPanel({ els, title = t('Text') }) {
  const editing = isEditing();
  // While editing, the buttons show the selected text: follow the selection.
  const [, setTick] = useState(0);
  useEffect(() => {
    if (!editing) return;
    const update = () => setTick((n) => n + 1);
    document.addEventListener('selectionchange', update);
    return () => document.removeEventListener('selectionchange', update);
  }, [editing]);
  // Selected text gets fonts, sizes and colors of its own; else the box.
  const part = editing && textSelected();
  const style = (el) => richOf(el).style;
  const setStyle = (patch, key, live) => patchAll(els, (el) => richPatch(el, { style: typeof patch === 'function' ? patch(el) : patch }), key && (live ? key : null));
  const role = same(els, (e) => style(e).role);
  const size = same(els, (e) => style(e).fontSize);
  const family = same(els, (e) => style(e).fontFamily);
  const color = same(els, (e) => style(e).color);
  const align = same(els, (e) => style(e).align);
  const valign = same(els, (e) => style(e).valign);
  const lh = same(els, (e) => style(e).lineHeight);
  const ls = same(els, (e) => style(e).letterSpacing);
  const ps = same(els, (e) => style(e).paragraphSpacing);
  const transform = same(els, (e) => style(e).transform);
  const shadow = same(els, (e) => style(e).shadow);
  const auto = same(els, (e) => style(e).autoHeight);
  const scale = Math.min(deck().width / 1280, deck().height / 720);
  const single = els.length === 1 ? els[0] : null;
  const isText = els.every((e) => e.content.kind === 'text');

  /** Applies an inline command, editing the whole text when not editing. */
  const inline = (cmd, value) => (isEditing() ? exec(cmd, value) : single && formatBox(single, cmd, value));

  /** Smaller or larger text: the selected text, or else whole boxes. */
  const resize = (f) => {
    if (isEditing() && textSelected()) {
      const px = Math.max(4, Math.round(selectionFontSize() * f));
      return styleSelection('font-size', `${px}px`);
    }
    setStyle((el) => ({ fontSize: Math.max(4, Math.round(style(el).fontSize * f)) }), 'fontSize', true);
  };

  const boldActive = formatActive(els, 'bold');
  const italicActive = formatActive(els, 'italic');

  return html`<${Section} title=${title} id="text">
    <${Row} label=${t('Style')}>
      <${Select}
        value=${role ?? ''}
        options=${[...(role === undefined ? [{ value: '', label: t('Mixed') }] : []), ...ROLES.map((r) => ({ ...r, label: t(r.label) }))]}
        onChange=${(v) => {
          const r = ROLES.find((x) => x.value === v);
          setStyle({ role: v, fontSize: Math.round(r.size * scale) }, 'role');
        }}
      />
    <//>
    <${Row} label=${t('Font')}>
      <${FontField}
        value=${(part && selectionStyle('font-family')) || family || ''}
        onChange=${(v) => (isEditing() && textSelected() ? styleSelection('font-family', v) : setStyle({ fontFamily: v }, 'font'))}
      />
    <//>
    <${Row} label=${t('Size')}>
      <div class="row-inline">
        <${NumberField}
          value=${part ? Math.round(selectionFontSize()) || size : size}
          min=${4}
          max=${800}
          unit="px"
          onChange=${(v, live) => (isEditing() && textSelected() ? styleSelection('font-size', `${v}px`, live) : setStyle({ fontSize: v }, 'fontSize', live))}
        />
        <div class="btn-group">
          <${Button} icon="minus" size="sm" title=${t('Smaller')} onMouseDown=${(e) => e.preventDefault()} onClick=${() => resize(1 / 1.12)} />
          <${Button} icon="plus" size="sm" title=${t('Larger')} onMouseDown=${(e) => e.preventDefault()} onClick=${() => resize(1.12)} />
        </div>
      </div>
    <//>
    <${Row} label=${t('Color')}>
      <${ColorField}
        value=${(part && selectionStyle('color')) || color || ''}
        noneLabel=${t('Theme color')}
        onChange=${(v, live) => {
          if (!isEditing() || !textSelected()) setStyle({ color: v }, 'color', live);
          else styleSelection('color', v, live);
        }}
      />
    <//>
    <div class="format-bar">
      <${Segmented}
        value=${null}
        options=${[
          { value: 'b', icon: 'bold', title: t('Bold (Ctrl+B)'), keepFocus: true },
          { value: 'i', icon: 'italic', title: t('Italic (Ctrl+I)'), keepFocus: true },
          { value: 'u', icon: 'underline', title: t('Underline (Ctrl+U)'), keepFocus: true },
          { value: 's', icon: 'strikethrough', title: t('Strikethrough'), keepFocus: true },
        ].map((o) => ({ ...o, active: (o.value === 'b' && boldActive) || (o.value === 'i' && italicActive) }))}
        onChange=${(v) => {
          if (v === 'b') isEditing() ? exec('bold') : toggleBoxes(els, 'bold');
          if (v === 'i') isEditing() ? exec('italic') : toggleBoxes(els, 'italic');
          if (v === 'u') inline('underline');
          if (v === 's') inline('strikeThrough');
        }}
      />
      <${Segmented}
        value=${null}
        options=${[
          { value: 'ul', icon: 'list', title: t('Bulleted list'), keepFocus: true },
          { value: 'ol', icon: 'list-ordered', title: t('Numbered list'), keepFocus: true },
          { value: 'sup', icon: 'superscript', title: t('Superscript'), keepFocus: true },
          { value: 'math', icon: 'sigma', title: t('Inline equation ($…$)'), keepFocus: true },
          { value: 'clear', icon: 'remove-formatting', title: t('Clear formatting'), keepFocus: true },
        ]}
        onChange=${(v) => {
          if (v === 'ul') inline('insertUnorderedList');
          if (v === 'ol') inline('insertOrderedList');
          if (v === 'sup') inline('superscript');
          if (v === 'clear') inline('removeFormat');
          if (v === 'math') {
            if (!isEditing() && single) beginTextEdit(canvasApi.view, single, null);
            insertInlineMath();
          }
        }}
      />
    </div>
    <${Hint}>${t('Math uses LaTeX, typeset with KaTeX.')} ${t('Inline equation:')} <code>$x^2$</code> · ${t('Centered equation:')} <code>$$…$$</code> · ${t('Dollar sign:')} <code>\$</code></${Hint}>
    ${editing && html`<${InlineExtras} />`}
    <${Row} label=${t('Align')}>
      <${Segmented}
        value=${align}
        options=${[
          { value: 'left', icon: 'text-align-start', title: t('Left') },
          { value: 'center', icon: 'text-align-center', title: t('Center') },
          { value: 'right', icon: 'text-align-end', title: t('Right') },
          { value: 'justify', icon: 'text-align-justify', title: t('Justify') },
        ]}
        onChange=${(v) => setStyle({ align: v }, 'align')}
      />
    <//>
    ${!(isText && auto) &&
    html`<${Row} label=${t('vertical alignment::Vertical')}>
      <${Segmented}
        value=${valign}
        options=${[
          { value: 'top', icon: 'align-start-horizontal', title: t('Top') },
          { value: 'middle', icon: 'align-center-horizontal', title: t('Middle') },
          { value: 'bottom', icon: 'align-end-horizontal', title: t('Bottom') },
        ]}
        onChange=${(v) => setStyle({ valign: v }, 'valign')}
      />
    <//>`}
    <${Row} label=${t('Spacing')}>
      <div class="row-inline">
        <${NumberField} label=${t('line height::Line')} value=${lh || 0} step=${0.05} min=${0} max=${4} title=${t('Line height (0 = automatic)')} onChange=${(v, live) => setStyle({ lineHeight: v }, 'lh', live)} />
        <${NumberField} label=${t('Letter')} value=${ls ?? 0} step=${0.01} min=${-0.5} max=${2} title=${t('Letter spacing (em)')} onChange=${(v, live) => setStyle({ letterSpacing: v }, 'ls', live)} />
      </div>
    <//>
    <${Row} label=${t('Paragraph')}>
      <${NumberField} value=${ps ?? 0.4} step=${0.05} min=${0} max=${5} unit="em" title=${t('Space after each paragraph')} onChange=${(v, live) => setStyle({ paragraphSpacing: v }, 'ps', live)} />
    <//>
    <${Row} label=${t('Case')}>
      <${Select}
        value=${transform ?? ''}
        options=${[
          { value: '', label: t('Theme default') },
          { value: 'none', label: t('As typed') },
          { value: 'uppercase', label: t('UPPERCASE') },
          { value: 'lowercase', label: t('lowercase') },
          { value: 'capitalize', label: t('Capitalize') },
        ]}
        onChange=${(v) => setStyle({ transform: v }, 'transform')}
      />
    <//>
    <${Row} label=${t('Shadow')}>
      <${Select}
        value=${shadow ?? 'none'}
        options=${[
          { value: 'none', label: t('None') },
          { value: 'soft', label: t('Soft') },
          { value: 'hard', label: t('Hard') },
          { value: 'glow', label: t('Glow (accent)') },
        ]}
        onChange=${(v) => setStyle({ shadow: v }, 'tshadow')}
      />
    <//>
    ${isText && html`<${Row}><${Toggle} value=${auto} label=${t('Fit box to text')} onChange=${(v) => setStyle({ autoHeight: v }, 'auto')} /><//>`}
  <//>`;
}

function InlineExtras() {
  const [url, setUrl] = useState('');
  return html`<div class="inline-extras">
    <${Hint}>${t('Formatting applies to the selected text.')}</${Hint}>
    <div class="row-inline">
      <${ColorField} value="" allowNone=${false} label=${t('Highlight')} onChange=${(v) => exec('hiliteColor', v)} />
    </div>
    <div class="row-inline link-row">
      <${Icon} name="link" size=${15} />
      <input class="textfield" placeholder=${t('Link address…')} value=${url} onInput=${(e) => setUrl(e.target.value)} onKeyDown=${(e) => {
          if (e.key !== 'Enter') return;
          // The link goes on the selected text: the Enter must not replace it.
          e.preventDefault();
          setLink(url);
        }} />
      <${Button} size="sm" label=${t('Link')} onMouseDown=${(e) => e.preventDefault()} onClick=${() => setLink(url)} />
      <${Button} size="sm" icon="unlink" title=${t('Remove link')} onMouseDown=${(e) => e.preventDefault()} onClick=${() => setLink('')} />
    </div>
  </div>`;
}

// ---------------------------------------------------------------------------
// Box (fill, border, corners, shadow)
// ---------------------------------------------------------------------------

export function BoxPanel({ els, title = t('Fill & Border'), fill = true, padding = true }) {
  const st = (e) => e.style;
  const set_ = (patch, key, live) => patchAll(els, { style: patch }, live ? key : null);
  const v = (k) => same(els, (e) => st(e)[k]);
  return html`<${Section} title=${title} id="box">
    ${fill && html`<${Row} label=${t('Fill')}><${ColorField} value=${v('fill') ?? ''} noneLabel=${t('No fill')} onChange=${(c, live) => set_({ fill: c }, 'fill', live)} /><//>`}
    <${Row} label=${t('Border')}>
      <div class="row-inline">
        <${NumberField} value=${v('borderWidth') ?? 0} min=${0} max=${200} unit="px" onChange=${(n, live) => set_({ borderWidth: n }, 'bw', live)} />
        <${Select}
          value=${v('borderStyle') ?? 'solid'}
          options=${[
            { value: 'solid', label: t('Solid') },
            { value: 'dashed', label: t('Dashed') },
            { value: 'dotted', label: t('Dotted') },
          ]}
          onChange=${(s) => set_({ borderStyle: s }, 'bs')}
        />
      </div>
    <//>
    ${(v('borderWidth') || 0) > 0 && html`<${Row} label=${t('Border color')}><${ColorField} value=${v('borderColor') ?? ''} allowNone=${false} onChange=${(c, live) => set_({ borderColor: c }, 'bc', live)} /><//>`}
    <${Row} label=${t('Corners')}><${Slider} value=${v('radius') ?? 0} min=${0} max=${400} unit="px" onChange=${(n, live) => set_({ radius: n }, 'radius', live)} /><//>
    ${padding && html`<${Row} label=${t('Padding')}><${Slider} value=${v('padding') ?? 0} min=${0} max=${200} unit="px" onChange=${(n, live) => set_({ padding: n }, 'pad', live)} /><//>`}
    <${Row} label=${t('Shadow')}>
      <${Select}
        value=${v('shadow') ?? 'none'}
        options=${[
          { value: 'none', label: t('None') },
          { value: 'soft', label: t('Soft') },
          { value: 'medium', label: t('Medium') },
          { value: 'hard', label: t('Hard (offset)') },
          { value: 'glow', label: t('Glow') },
        ]}
        onChange=${(s) => set_({ shadow: s }, 'shadow')}
      />
    <//>
    ${(v('shadow') ?? 'none') !== 'none' && html`<${Row} label=${t('Shadow color')}><${ColorField} value=${v('shadowColor') ?? ''} allowNone=${false} onChange=${(c, live) => set_({ shadowColor: c }, 'sc', live)} /><//>`}
  <//>`;
}

// ---------------------------------------------------------------------------
// Shapes
// ---------------------------------------------------------------------------

export function ShapePanel({ els }) {
  const shape = same(els, (e) => e.content.shape);
  const isLine = shape === 'line';
  const set_ = (patch, key, live) => patchAll(els, patch, live ? key : null);
  const v = (k) => same(els, (e) => e.style[k]);
  const caps = CAPS.map((c) => ({ ...c, label: t(c.label) }));
  return html`<${Section} title=${t('Shape')} id="shape">
    <div class="shape-grid">
      ${state.info.shapes.map(
        (s) => html`<button type="button" class=${cls('shape-opt', shape === s.id && 'active')} title=${t(s.name)} onClick=${() => set_({ content: { shape: s.id } }, 'shape')}>
          <${ShapeGlyph} id=${s.id} />
        </button>`,
      )}
    </div>
    ${!isLine && html`<${Row} label=${t('Fill')}><${ColorField} value=${v('fill') ?? ''} noneLabel=${t('Theme accent')} onChange=${(c, live) => set_({ style: { fill: c } }, 'fill', live)} /><//>`}
    <${Row} label=${isLine ? t('shape stroke::Line') : t('Outline')}>
      <div class="row-inline">
        <${NumberField} value=${v('borderWidth') ?? 0} min=${0} max=${100} unit="px" onChange=${(n, live) => set_({ style: { borderWidth: n } }, 'bw', live)} />
        <${Select}
          value=${v('borderStyle') ?? 'solid'}
          options=${[
            { value: 'solid', label: t('Solid') },
            { value: 'dashed', label: t('Dashed') },
            { value: 'dotted', label: t('Dotted') },
          ]}
          onChange=${(s) => set_({ style: { borderStyle: s } }, 'bs')}
        />
      </div>
    <//>
    <${Row} label=${isLine ? t('Line color') : t('Outline color')}>
      <${ColorField} value=${v('borderColor') ?? ''} allowNone=${isLine} noneLabel=${t('Theme text color')} onChange=${(c, live) => set_({ style: { borderColor: c } }, 'bc', live)} />
    <//>
    ${shape === 'rounded' && html`<${Row} label=${t('Corners')}><${Slider} value=${v('radius') ?? 0} min=${0} max=${300} unit="px" onChange=${(n, live) => set_({ style: { radius: n } }, 'radius', live)} /><//>`}
    ${isLine &&
    html`<${Row} label=${t('Ends')}>
      <div class="row-inline">
        <${Select} value=${same(els, (e) => e.content.startCap) ?? 'none'} options=${caps} onChange=${(c) => set_({ content: { startCap: c } }, 'sc')} />
        <${Select} value=${same(els, (e) => e.content.endCap) ?? 'none'} options=${caps} onChange=${(c) => set_({ content: { endCap: c } }, 'ec')} />
      </div>
    <//>`}
    <${Row} label=${t('Shadow')}>
      <${Select}
        value=${v('shadow') ?? 'none'}
        options=${[
          { value: 'none', label: t('None') },
          { value: 'soft', label: t('Soft') },
          { value: 'hard', label: t('Hard (offset)') },
          { value: 'glow', label: t('Glow') },
        ]}
        onChange=${(s) => set_({ style: { shadow: s } }, 'shadow')}
      />
    <//>
  <//>`;
}

const CAPS = [
  { value: 'none', label: N_('None') },
  { value: 'arrow', label: N_('Arrow') },
  { value: 'dot', label: N_('Dot') },
];

/** Small glyph for the shape picker. */
export function ShapeGlyph({ id }) {
  const P = {
    rect: 'M3 5h18v14H3z',
    rounded: 'M7 5h10a4 4 0 0 1 4 4v6a4 4 0 0 1-4 4H7a4 4 0 0 1-4-4V9a4 4 0 0 1 4-4z',
    ellipse: 'M12 5c5 0 9 3.1 9 7s-4 7-9 7-9-3.1-9-7 4-7 9-7z',
    triangle: 'M12 4l9 16H3z',
    'right-triangle': 'M4 4v16h16z',
    diamond: 'M12 3l9 9-9 9-9-9z',
    pentagon: 'M12 3l9 6.5-3.4 10.5H6.4L3 9.5z',
    hexagon: 'M7 4h10l5 8-5 8H7l-5-8z',
    octagon: 'M8 3h8l5 5v8l-5 5H8l-5-5V8z',
    star: 'M12 3l2.7 5.8 6.3.7-4.7 4.3 1.3 6.2L12 17l-5.6 3 1.3-6.2L3 9.5l6.3-.7z',
    burst: 'M12 2l2 4 4-2-1 4.5 4.5 1-3.5 3 3.5 3-4.5 1 1 4.5-4-2-2 4-2-4-4 2 1-4.5-4.5-1L6 12 2.5 9l4.5-1L6 3.5l4 2z',
    arrow: 'M3 9h11V5l7 7-7 7v-4H3z',
    'double-arrow': 'M2 12l5-6v4h10V6l5 6-5 6v-4H7v4z',
    chevron: 'M3 5h12l6 7-6 7H3l6-7z',
    parallelogram: 'M7 5h14l-4 14H3z',
    trapezoid: 'M7 5h10l4 14H3z',
    plus: 'M9 3h6v6h6v6h-6v6H9v-6H3V9h6z',
    heart: 'M12 20s-8-5-8-11a4.5 4.5 0 0 1 8-2.5A4.5 4.5 0 0 1 20 9c0 6-8 11-8 11z',
    bubble: 'M5 4h14a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-8l-5 4v-4H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z',
    cloud: 'M7 19a5 5 0 0 1-.5-10A6 6 0 0 1 18 8a4.5 4.5 0 0 1 0 11z',
    line: 'M3 12h15m-3-4 4 4-4 4',
  };
  return html`<svg viewBox="0 0 24 24" width="22" height="22" class=${cls('shape-glyph', id === 'line' && 'stroke')}><path d=${P[id] || P.rect} /></svg>`;
}

// ---------------------------------------------------------------------------
// Images & video
// ---------------------------------------------------------------------------

export function ImagePanel({ els }) {
  const single = els.length === 1 ? els[0] : null;
  const c = (k) => same(els, (e) => e.content[k]);
  const set_ = (patch, key, live) => patchAll(els, { content: patch }, live ? key : null);
  return html`<${Section} title=${t('Image')} id="image">
    ${single &&
    html`<div class="row-inline">
      <${Button} icon="image-up" label=${single.content.src ? t('Replace…') : t('From Computer…')} onClick=${() => A.replaceImage(single)} />
      <${Button} icon="images" label=${t('Search Online…')} onClick=${() => import('./media.js').then((m) => m.openMediaLibrary('photos', single.id))} />
      ${single.content.naturalWidth > 0 && html`<${Button} icon="ratio" label=${t('Original Ratio')} onClick=${() => A.resetImageSize(single)} />`}
    </div>`}
    <${Row} label=${t('Fit')}>
      <${Segmented}
        value=${c('fit')}
        options=${[
          { value: 'cover', label: t('image fit::Fill') },
          { value: 'contain', label: t('image fit::Fit') },
          { value: 'fill', label: t('Stretch') },
        ]}
        onChange=${(v) => set_({ fit: v }, 'fit')}
      />
    <//>
    ${c('fit') === 'cover' &&
    html`<${Row} label=${t('Focus')}>
      <div class="row-inline">
        <${NumberField} label="X" value=${c('focusX') ?? 50} min=${0} max=${100} unit="%" onChange=${(v, live) => set_({ focusX: v }, 'fx', live)} />
        <${NumberField} label="Y" value=${c('focusY') ?? 50} min=${0} max=${100} unit="%" onChange=${(v, live) => set_({ focusY: v }, 'fy', live)} />
      </div>
    <//>`}
    <${Row} label=${t('Flip')}>
      <div class="row-inline">
        <${Toggle} value=${c('flipX')} label=${t('flip::Horizontal')} onChange=${(v) => set_({ flipX: v }, 'flipx')} />
        <${Toggle} value=${c('flipY')} label=${t('flip::Vertical')} onChange=${(v) => set_({ flipY: v }, 'flipy')} />
      </div>
    <//>
    <${Row} label=${t('Grayscale')}><${Slider} value=${c('grayscale') ?? 0} min=${0} max=${100} unit="%" onChange=${(v, live) => set_({ grayscale: v }, 'gray', live)} /><//>
    <${Row} label=${t('Brightness')}><${Slider} value=${c('brightness') ?? 100} min=${0} max=${200} unit="%" onChange=${(v, live) => set_({ brightness: v }, 'bright', live)} /><//>
    <${Row} label=${t('Contrast')}><${Slider} value=${c('contrast') ?? 100} min=${0} max=${200} unit="%" onChange=${(v, live) => set_({ contrast: v }, 'contrast', live)} /><//>
    <${Row} label=${t('Saturation')}><${Slider} value=${c('saturate') ?? 100} min=${0} max=${300} unit="%" onChange=${(v, live) => set_({ saturate: v }, 'sat', live)} /><//>
    <${Row} label=${t('Blur')}><${Slider} value=${c('blur') ?? 0} min=${0} max=${40} unit="px" onChange=${(v, live) => set_({ blur: v }, 'blur', live)} /><//>
    <${Row} label=${t('Description')} stack><${TextField} value=${c('alt') ?? ''} placeholder=${t('Alternative text for accessibility')} onChange=${(v) => set_({ alt: v }, 'alt')} /><//>
  <//>`;
}

export function VideoPanel({ els }) {
  const single = els.length === 1 ? els[0] : null;
  const c = (k) => same(els, (e) => e.content[k]);
  const set_ = (patch, key) => patchAll(els, { content: patch }, null);
  const online = /youtube\.com|youtu\.be|vimeo\.com/i.test(c('src') || '');
  return html`<${Section} title=${t('Video')} id="video">
    ${single &&
    html`<div class="row-inline">
      <${Button} icon="film" label=${t('Choose File…')} onClick=${() => A.replaceVideo(single)} />
    </div>`}
    <${Row} label=${t('Address')} stack>
      <${TextField} value=${c('src') ?? ''} placeholder=${t('YouTube/Vimeo link or video URL')} onChange=${(v) => set_({ src: v.trim() }, 'src')} />
    <//>
    <${Row}><${Toggle} value=${c('autoplay')} label=${t('Play automatically')} onChange=${(v) => set_({ autoplay: v }, 'autoplay')} /><//>
    <${Row}><${Toggle} value=${c('looped')} label=${t('Loop')} onChange=${(v) => set_({ looped: v }, 'loop')} /><//>
    <${Row}><${Toggle} value=${c('muted')} label=${t('Muted')} onChange=${(v) => set_({ muted: v }, 'muted')} /><//>
    <${Row}><${Toggle} value=${c('controls')} label=${t('Show controls')} onChange=${(v) => set_({ controls: v }, 'controls')} /><//>
    ${!online &&
    html`<${Row} label=${t('Fit')}>
      <${Segmented}
        value=${c('fit')}
        options=${[
          { value: 'contain', label: t('image fit::Fit') },
          { value: 'cover', label: t('image fit::Fill') },
        ]}
        onChange=${(v) => set_({ fit: v }, 'fit')}
      />
    <//>`}
    ${online && html`<${Hint}>${t('Online videos need an internet connection while presenting.')}</${Hint}>`}
  <//>`;
}

/** Cameras seen during earlier shows (only a show turns the camera on). */
function knownCameras() {
  try {
    const list = JSON.parse(localStorage.getItem('cz-cameras') || '[]');
    return Array.isArray(list) ? list.filter((d) => d && d.id && d.label) : [];
  } catch {
    return [];
  }
}

export function CameraPanel({ els }) {
  const c = (k) => same(els, (e) => e.content[k]);
  const set_ = (patch) => patchAll(els, { content: patch }, null);
  const [cameras, setCameras] = useState(knownCameras);
  // A show running in another window may find cameras meanwhile.
  useEffect(() => {
    const update = (e) => e.key === 'cz-cameras' && setCameras(knownCameras());
    window.addEventListener('storage', update);
    return () => window.removeEventListener('storage', update);
  }, []);
  const options = [{ value: '', label: t('Default camera') }, ...cameras.map((d) => ({ value: d.id, label: d.label }))];
  const chosen = c('deviceId');
  if (chosen && !cameras.some((d) => d.id === chosen)) options.push({ value: chosen, label: c('deviceLabel') || chosen });
  return html`<${Section} title=${t('Live Camera')} id="camera">
    <${Row} label=${t('Camera')}>
      <${Select}
        value=${chosen ?? ''}
        options=${options}
        onChange=${(v) => set_({ deviceId: v, deviceLabel: cameras.find((d) => d.id === v)?.label || '' })}
      />
    <//>
    <${Row}><${Toggle} value=${c('mirror')} label=${t('Mirror image')} onChange=${(v) => set_({ mirror: v })} /><//>
    <${Row} label=${t('Shape')}>
      <${Segmented}
        value=${c('mask')}
        options=${[
          { value: 'rect', label: t('Rectangle') },
          { value: 'circle', label: t('Circle') },
        ]}
        onChange=${(v) => set_({ mask: v })}
      />
    <//>
    <${Hint}>${[
      t('Only the audience sees the camera, and only while you present its slide: the editor and the presenter display never turn it on.'),
      t('Exported presentations and printouts leave it out.'),
      !cameras.length && t('Your cameras are listed here by name after you present live video once.'),
      t('For a round picture, choose Circle with a square frame.'),
    ]
      .filter(Boolean)
      .join(' ')}</${Hint}>
  <//>`;
}

export function EmbedPanel({ els }) {
  const c = (k) => same(els, (e) => e.content[k]);
  const set_ = (patch, key) => patchAll(els, { content: patch }, null);
  return html`<${Section} title=${t('Web Page')} id="embed">
    <${Row} label=${t('Address')} stack>
      <${TextField} value=${c('url') ?? ''} placeholder="https://…" onChange=${(v) => set_({ url: v.trim() }, 'url')} />
    <//>
    <${Row}><${Toggle} value=${c('interactive')} label=${t('Interactive while presenting')} onChange=${(v) => set_({ interactive: v }, 'interactive')} /><//>
    <${Hint}>${t('The page is shown live during the presentation. Some sites refuse to be embedded.')}</${Hint}>
  <//>`;
}

// ---------------------------------------------------------------------------
// Code & math
// ---------------------------------------------------------------------------

const LANGS = [
  'plaintext', 'bash', 'c', 'cpp', 'csharp', 'css', 'dart', 'diff', 'dockerfile', 'elixir', 'go', 'graphql', 'haskell', 'html', 'java',
  'javascript', 'json', 'julia', 'kotlin', 'latex', 'lua', 'makefile', 'markdown', 'matlab', 'nginx', 'objectivec', 'perl', 'php',
  'powershell', 'python', 'r', 'ruby', 'rust', 'scala', 'scss', 'shell', 'sql', 'swift', 'toml', 'typescript', 'xml', 'yaml',
];

function useFocusPrimary(ref) {
  useEffect(() => {
    const f = () => ref.current?.querySelector('textarea, input')?.focus();
    window.addEventListener('cz-focus-primary', f);
    return () => window.removeEventListener('cz-focus-primary', f);
  }, []);
}

export function CodePanel({ els }) {
  const single = els.length === 1 ? els[0] : null;
  const ref = useRef();
  useFocusPrimary(ref);
  const c = (k) => same(els, (e) => e.content[k]);
  const set_ = (patch, key, live) => patchAll(els, { content: patch }, live ? key : null);
  return html`<${Section} title=${t('Code')} id="code">
    <div ref=${ref}>
      ${single && html`<${TextField} multiline mono rows=${10} live value=${single.content.code} onChange=${(v, live) => set_({ code: v }, 'code', live)} />`}
    </div>
    <${Row} label=${t('Language')}>
      <${Select} value=${c('language') ?? ''} options=${LANGS.map((l) => ({ value: l, label: l }))} onChange=${(v) => set_({ language: v }, 'lang')} />
    <//>
    <${Row} label=${t('Size')}><${NumberField} value=${c('fontSize')} min=${6} max=${200} unit="px" onChange=${(v, live) => set_({ fontSize: v }, 'fs', live)} /><//>
    <${Row}><${Toggle} value=${c('lineNumbers')} label=${t('Line numbers')} onChange=${(v) => set_({ lineNumbers: v }, 'ln')} /><//>
    <${Row} label=${t('Step highlights')} stack>
      <${TextField} value=${c('highlightSteps') ?? ''} placeholder=${t('e.g. 1|2-3|5,7')} onChange=${(v) => set_({ highlightSteps: v }, 'steps')} />
    <//>
    <${Hint}>${t('Each “|” is a click during the presentation that highlights those lines.')}</${Hint}>
    <${Row} label=${t('Title')} stack><${TextField} value=${c('title') ?? ''} placeholder=${t('Optional file name or caption')} onChange=${(v) => set_({ title: v }, 'title')} /><//>
    <${Row} label=${t('Corners')}><${Slider} value=${same(els, (e) => e.style.radius) ?? 0} min=${0} max=${60} unit="px" onChange=${(v, live) => patchAll(els, { style: { radius: v } }, live ? 'radius' : null)} /><//>
  <//>`;
}

export function MathPanel({ els }) {
  const single = els.length === 1 ? els[0] : null;
  const ref = useRef();
  useFocusPrimary(ref);
  const c = (k) => same(els, (e) => e.content[k]);
  const set_ = (patch, key, live) => patchAll(els, { content: patch }, live ? key : null);
  return html`<${Section} title=${t('Equation')} id="math">
    <div ref=${ref}>
      ${single && html`<${TextField} multiline mono rows=${4} live value=${single.content.latex} placeholder=${t('LaTeX, e.g. {example}', { example: '\\frac{a}{b}' })} onChange=${(v, live) => set_({ latex: v }, 'latex', live)} />`}
    </div>
    <${Hint}>${t('Write LaTeX — rendered with KaTeX. Example:')} <code>\\int_0^1 x^2\\,dx = \\tfrac13</code></${Hint}>
    <${Row} label=${t('Size')}><${NumberField} value=${c('fontSize')} min=${6} max=${400} unit="px" onChange=${(v, live) => set_({ fontSize: v }, 'fs', live)} /><//>
    <${Row} label=${t('Color')}><${ColorField} value=${c('color') ?? ''} noneLabel=${t('Theme text color')} onChange=${(v, live) => set_({ color: v }, 'color', live)} /><//>
    <${Row} label=${t('Align')}>
      <${Segmented}
        value=${c('align')}
        options=${[
          { value: 'left', icon: 'text-align-start', title: t('Left') },
          { value: 'center', icon: 'text-align-center', title: t('Center') },
          { value: 'right', icon: 'text-align-end', title: t('Right') },
        ]}
        onChange=${(v) => set_({ align: v }, 'align')}
      />
    <//>
  <//>`;
}

// ---------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------

export function TablePanel({ els }) {
  const el = els[0];
  const tbl = el.content;
  const rows = tbl.rows;
  const cols = Math.max(1, ...rows.map((r) => r.length));
  const set_ = (patch, key, live) => patchAll([el], { content: patch }, live ? key : null);
  const resize = (nr, nc) => {
    const next = Array.from({ length: Math.max(1, nr) }, (_, r) => Array.from({ length: Math.max(1, nc) }, (_, c) => rows[r]?.[c] ?? ''));
    const grow = (nr - rows.length) * (el.h / Math.max(1, rows.length));
    apply({ op: 'updateElements', slide: state.slideId, updates: [{ id: el.id, patch: { content: { rows: next }, h: Math.max(40, el.h + (nr > rows.length ? grow : 0)) } }] });
  };
  return html`<${Section} title=${t('Table')} id="table">
    <${Button} icon="text-cursor-input" label=${t('Edit Cells')} onClick=${() => beginTableEdit(canvasApi.view, el)} />
    <${Row} label=${t('Rows')}>
      <div class="row-inline">
        <${NumberField} value=${rows.length} min=${1} max=${50} onChange=${(v) => resize(v, cols)} />
        <div class="btn-group">
          <${Button} icon="minus" size="sm" title=${t('Remove last row')} onClick=${() => resize(rows.length - 1, cols)} />
          <${Button} icon="plus" size="sm" title=${t('Add row')} onClick=${() => resize(rows.length + 1, cols)} />
        </div>
      </div>
    <//>
    <${Row} label=${t('Columns')}>
      <div class="row-inline">
        <${NumberField} value=${cols} min=${1} max=${20} onChange=${(v) => resize(rows.length, v)} />
        <div class="btn-group">
          <${Button} icon="minus" size="sm" title=${t('Remove last column')} onClick=${() => resize(rows.length, cols - 1)} />
          <${Button} icon="plus" size="sm" title=${t('Add column')} onClick=${() => resize(rows.length, cols + 1)} />
        </div>
      </div>
    <//>
    <${Row}><${Toggle} value=${tbl.headerRow} label=${t('Header row')} onChange=${(v) => set_({ headerRow: v }, 'hr')} /><//>
    <${Row}><${Toggle} value=${tbl.headerCol} label=${t('Header column')} onChange=${(v) => set_({ headerCol: v }, 'hc')} /><//>
    <${Row}><${Toggle} value=${tbl.striped} label=${t('Striped rows')} onChange=${(v) => set_({ striped: v }, 'striped')} /><//>
    <${Row} label=${t('Text size')}><${NumberField} value=${tbl.fontSize} min=${4} max=${200} unit="px" onChange=${(v, live) => set_({ fontSize: v }, 'fs', live)} /><//>
    <${Row} label=${t('Align')}>
      <${Segmented}
        value=${tbl.align}
        options=${[
          { value: 'left', icon: 'text-align-start', title: t('Left') },
          { value: 'center', icon: 'text-align-center', title: t('Center') },
          { value: 'right', icon: 'text-align-end', title: t('Right') },
        ]}
        onChange=${(v) => set_({ align: v }, 'align')}
      />
    <//>
    <${Row} label=${t('Text color')}><${ColorField} value=${tbl.color} noneLabel=${t('Theme text color')} onChange=${(v, live) => set_({ color: v }, 'color', live)} /><//>
    <${Row} label=${t('Header fill')}><${ColorField} value=${tbl.headerFill} allowNone=${false} onChange=${(v, live) => set_({ headerFill: v }, 'hf', live)} /><//>
    <${Row} label=${t('Header text')}><${ColorField} value=${tbl.headerColor} allowNone=${false} onChange=${(v, live) => set_({ headerColor: v }, 'hcol', live)} /><//>
    <${Row} label=${t('Stripes')}><${ColorField} value=${tbl.stripeFill} allowNone=${false} onChange=${(v, live) => set_({ stripeFill: v }, 'sf', live)} /><//>
    <${Row} label=${t('Lines')}><${ColorField} value=${tbl.gridColor} allowNone=${false} onChange=${(v, live) => set_({ gridColor: v }, 'gc', live)} /><//>
  <//>`;
}

// ---------------------------------------------------------------------------
// Charts
// ---------------------------------------------------------------------------

// Country maps by UI language (maps.rs), fetched once.
const mapCountries = {};

function useMapCountries(on) {
  const lang = language();
  const [list, setList] = useState(mapCountries[lang] || []);
  useEffect(() => {
    if (!on) return;
    if (mapCountries[lang]) return setList(mapCountries[lang]);
    invoke('map_countries', { language: lang })
      .then((l) => {
        mapCountries[lang] = l;
        setList(l);
      })
      .catch(() => {});
  }, [on, lang]);
  return list;
}

/**
 * A map's regions (for suggestions, and whether a row names each one), the
 * rows that name none and the rows that name a region a second time.
 */
function useMapRegions(on, map, labels) {
  const lang = language();
  const [info, setInfo] = useState({ regions: [], unmatched: [], repeated: [] });
  const key = labels.join('\n');
  useEffect(() => {
    if (!on) return;
    let live = true;
    invoke('map_regions', { map: map || 'world', language: lang, labels })
      .then((rows) => live && setInfo(rows))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [on, map, lang, key]);
  return info;
}

export function ChartPanel({ els }) {
  const el = els[0];
  const ch = el.content;
  const ref = useRef();
  useFocusPrimary(ref);
  const set_ = (patch, key, live) => patchAll([el], { content: patch }, live ? key : null);
  const kind = chartKind(ch);
  const one = oneSeries(kind);
  // Candlesticks take exactly four series (opening, high, low, closing).
  const fixed = one || kind === 'candlestick';
  const isMap = kind === 'map';
  const countries = useMapCountries(isMap);
  const { regions, unmatched, repeated } = useMapRegions(isMap, ch.map, ch.labels);
  const n = ch.labels.length;
  const setCell = (s, i, v) => {
    const series = ch.series.map((x, k) => (k === s ? { ...x, values: Array.from({ length: n }, (_, j) => (j === i ? v : x.values[j] ?? 0)) } : x));
    set_({ series }, 'data', true);
  };
  const setLabel = (i, v) => set_({ labels: ch.labels.map((l, j) => (j === i ? v : l)) }, 'labels', true);
  // Error bars: one number (or none) per value, kept beside the values.
  const padded = (list = []) => Array.from({ length: n }, (_, j) => list[j] ?? null);
  const setErr = (k, i, key, v) => {
    const series = ch.series.map((x, j) => (j === k ? { ...x, [key]: padded(x[key]).map((e, r) => (r === i ? v : e)) } : x));
    set_({ series }, 'data', true);
  };
  const addRow = () =>
    set_(
      {
        labels: [...ch.labels, t('Item {n}', { n: n + 1 })],
        series: ch.series.map((s) => ({
          ...s,
          values: [...s.values.slice(0, n), 0],
          low: s.low?.length ? [...padded(s.low), null] : [],
          high: s.high?.length ? [...padded(s.high), null] : [],
        })),
      },
      'rows',
    );
  const delRow = (i) =>
    set_(
      {
        labels: ch.labels.filter((_, j) => j !== i),
        series: ch.series.map((s) => ({
          ...s,
          values: s.values.filter((_, j) => j !== i),
          low: (s.low || []).filter((_, j) => j !== i),
          high: (s.high || []).filter((_, j) => j !== i),
        })),
      },
      'rows',
    );
  const addSeries = () =>
    set_({ series: [...ch.series, { name: t('Series {n}', { n: ch.series.length + 1 }), values: ch.labels.map(() => 0), color: '', marker: '', low: [], high: [] }] }, 'series');
  const delSeries = (k) => set_({ series: ch.series.filter((_, j) => j !== k) }, 'series');
  const shownSeries = one ? ch.series.slice(0, 1) : ch.series;
  const linked = linkChart(kind);
  const links = ch.links || [];
  const setLink = (i, patch) => set_({ links: links.map((l, j) => (j === i ? { ...l, ...patch } : l)) }, 'links', true);
  const addLink = () => set_({ links: [...links, { from: '', to: '', value: 1 }] }, 'links');
  const delLink = (i) => set_({ links: links.filter((_, j) => j !== i) }, 'links');
  // Series that get a color of their own: not slices, rectangles of a
  // one-series treemap, heatmap cells, candles or a scatterplot's X.
  const colored =
    linked || one || kind === 'heatmap' || kind === 'candlestick' || (kind === 'treemap' && ch.series.length <= 1)
      ? []
      : ch.series.map((s, k) => ({ s, k })).filter(({ k }) => !(kind === 'scatter' && k === 0 && ch.series.length > 1));
  const hint = {
    scatter: t('The first column holds the X values. Each other column is a group of points.'),
    heatmap: t('Each cell is colored by its value.'),
    parliament: t('One row per party, with its number of seats.'),
    treemap: t('One column: a rectangle per row. Several columns: each column is a group of rectangles.'),
    candlestick: t('Each row needs four values, in this order: opening, high, low and closing price.'),
    sankey: t('One row per flow: where it comes from, where it goes and how much.'),
    network: t('One row per connection between two nodes. The value sets the line width.'),
    map: t('One row per region: its name or code, and its value. Regions without a row stay gray.'),
  }[kind];
  const grid = ['column', 'stacked-column', 'bar', 'stacked-bar', 'line', 'dot', 'area', 'stacked-area', 'streamgraph', 'scatter', 'candlestick'].includes(kind);
  const errorsOn = hasErrorBars(kind) && !!ch.errorBars;
  const errCols = errorsOn ? errorColumns(ch.errorBars) : [];
  const markers = hasMarkers(kind);
  const axes = hasAxes(kind);
  const refLines = ch.refLines || [];
  const setRef = (i, patch, live) => set_({ refLines: refLines.map((l, j) => (j === i ? { ...l, ...patch } : l)) }, 'ref', live);
  const addRef = () => set_({ refLines: [...refLines, { value: typicalValue(ch), label: '' }] }, 'ref');
  const delRef = (i) => set_({ refLines: refLines.filter((_, j) => j !== i) }, 'ref');
  const step = axisStep(ch);
  return html`<${Section} title=${t('Chart')} id="chart">
    <div class="chart-types">
      ${state.info.charts.map(
        (c) => html`<button type="button" class=${cls('chart-type', kind === c.id && 'active')} title=${t(c.name)} onClick=${() => kind !== c.id && set_(switchChart(ch, c.id), 'type')}>
          <${Icon} name=${CHART_ICONS[c.id]} size=${18} />
          <span>${t(c.name)}</span>
        </button>`,
      )}
    </div>
    <div class="row-inline">
      <${Button}
        size="sm"
        icon="sheet"
        label=${t('Edit as CSV…')}
        title=${t('Paste or open data in CSV form and pick the chart to make from it.')}
        onClick=${() => openModal('chartData', { target: el.id })}
      />
    </div>
    ${isMap &&
    html`<${Row} label=${t('Map')}>
      <${Select}
        value=${ch.map || 'world'}
        options=${[
          { value: 'world', label: t('World') },
          { group: t('Continents'), options: MAP_AREAS.slice(1).map(([value, name]) => ({ value, label: t(name) })) },
          { group: t('Countries'), options: countries.map(([value, label]) => ({ value, label })) },
        ]}
        onChange=${(v) => set_({ map: v }, 'map')}
      />
    <//>`}
    ${hint && html`<${Hint}>${hint}</${Hint}>`}
    ${isMap && unmatched.length > 0 && html`<${Hint}>${t('Not on this map: {names}', { names: unmatched.join(', ') })}</${Hint}>`}
    ${isMap && repeated.length > 0 && html`<${Hint}>${t('Already in an earlier row, so left out: {names}', { names: repeated.join(', ') })}</${Hint}>`}
    ${isMap &&
    html`<div class="row-inline">
      <${Button}
        size="sm"
        icon="list-plus"
        label=${t('Add All Regions')}
        title=${t('Adds a row for each region of the map that has none yet.')}
        onClick=${() => {
          // The sample is replaced; rows someone wrote are kept, and so are
          // the regions they name (by a name, a code or another language).
          const fresh = isSample(ch, 'map');
          const add = regions.filter((r) => fresh || !r.named).map((r) => r.label);
          const labels = [...(fresh ? [] : ch.labels), ...add];
          const first = ch.series[0] || { name: t('Series {n}', { n: 1 }), color: '' };
          const values = [...(fresh ? [] : first.values.slice(0, ch.labels.length)), ...add.map(() => 0)];
          set_({ labels, series: [{ ...first, values }, ...(fresh ? [] : ch.series.slice(1))] }, 'regions');
        }}
      />
    </div>`}
    ${isMap && html`<datalist id="cz-map-regions">${regions.map((r) => html`<option value=${r.name}>${r.id}</option>`)}</datalist>`}
    ${linked &&
    html`<div class="datagrid" ref=${ref}>
      <table>
        <thead>
          <tr><th>${t('From')}</th><th>${t('To')}</th><th>${t('Value')}</th><th></th></tr>
        </thead>
        <tbody>
          ${links.map(
            (l, i) => html`<tr>
              <td><${Cell} class="cell label" value=${l.from} onChange=${(v) => setLink(i, { from: v })} /></td>
              <td><${Cell} class="cell label" value=${l.to} onChange=${(v) => setLink(i, { to: v })} /></td>
              <td><${Cell} class="cell num" number value=${l.value} onChange=${(v) => setLink(i, { value: v })} /></td>
              <td><button class="cell-x" title=${t('Remove row')} onClick=${() => delLink(i)}>×</button></td>
            </tr>`,
          )}
        </tbody>
      </table>
      <button class="cell-add wide" onClick=${addLink}>${t('+ Add row')}</button>
    </div>`}
    ${hasErrorBars(kind) &&
    html`<${Row} label=${t('Error bars')}>
      <${Select} value=${ch.errorBars || ''} options=${ERROR_MODES.map(([value, label]) => ({ value, label: t(label) }))} onChange=${(v) => set_({ errorBars: v }, 'errors')} />
    <//>`}
    ${!linked &&
    html`<div class="datagrid" ref=${ref}>
      <table>
        <thead>
          <tr>
            <th></th>
            ${shownSeries.map(
              (s, k) => html`<th>
                <${Cell} class="cell head" value=${s.name} onChange=${(v) => set_({ series: ch.series.map((x, j) => (j === k ? { ...x, name: v } : x)) }, 'sname')} />
                ${!fixed && ch.series.length > 1 && html`<button class="cell-x" title=${t('Remove series')} onClick=${() => delSeries(k)}>×</button>`}
              </th>
              ${errCols.map(([, head]) => html`<th class="err-head" title=${t('Error bars of {name}', { name: s.name })}>${t(head)}</th>`)}`,
            )}
            ${!fixed && html`<th><button class="cell-add" title=${t('Add series')} onClick=${addSeries}>+</button></th>`}
          </tr>
        </thead>
        <tbody>
          ${ch.labels.map(
            (l, i) => html`<tr>
              <td><${Cell} class="cell label" list=${isMap ? 'cz-map-regions' : undefined} value=${l} onChange=${(v) => setLabel(i, v)} /></td>
              ${shownSeries.map(
                (s, k) => html`<td>
                    <${Cell} class="cell num" number value=${s.values[i] ?? 0} onChange=${(v) => setCell(k, i, v)} />
                  </td>
                  ${errCols.map(
                    ([key]) => html`<td>
                      <${Cell} class="cell num err" number nullable value=${s[key]?.[i] ?? null} onChange=${(v) => setErr(k, i, key, v)} />
                    </td>`,
                  )}`,
              )}
              <td><button class="cell-x" title=${t('Remove row')} onClick=${() => delRow(i)}>×</button></td>
            </tr>`,
          )}
        </tbody>
      </table>
      <button class="cell-add wide" onClick=${addRow}>${t('+ Add row')}</button>
    </div>`}
    ${colored.length > 0 &&
    html`<div class="series-colors">
      ${colored.map(
        ({ s, k }) => html`<${Row} label=${s.name}>
          <div class=${cls('series-style', markers && 'with-marker')}>
            <${ColorField}
              value=${s.color}
              noneLabel=${t('Theme color {n}', { n: (k % 6) + 1 })}
              onChange=${(v, live) => set_({ series: ch.series.map((x, j) => (j === k ? { ...x, color: v } : x)) }, 'scolor', live)}
            />
            ${markers &&
            html`<${Select}
              title=${t('Point shape')}
              value=${s.marker || ''}
              options=${MARKERS.map(([value, label]) => ({ value, label: t(label) }))}
              onChange=${(v) => set_({ series: ch.series.map((x, j) => (j === k ? { ...x, marker: v } : x)) }, 'marker')}
            />`}
          </div>
        <//>`,
      )}
    </div>`}
    ${(kind === 'heatmap' || isMap) &&
    html`<${Row} label=${t('Color')}>
      <${ColorField} value=${ch.scaleColor} noneLabel=${t('Theme color {n}', { n: 1 })} onChange=${(v, live) => set_({ scaleColor: v }, 'scale', live)} />
    <//>`}
    ${kind !== 'candlestick' && !linked && html`<${Row}><${Toggle} value=${ch.showLegend} label=${t('Legend')} onChange=${(v) => set_({ showLegend: v }, 'legend')} /><//>`}
    ${kind !== 'streamgraph' && html`<${Row}><${Toggle} value=${ch.showValues} label=${t('Value labels')} onChange=${(v) => set_({ showValues: v }, 'values')} /><//>`}
    ${grid && html`<${Row}><${Toggle} value=${ch.showGrid} label=${t('Grid lines')} onChange=${(v) => set_({ showGrid: v }, 'grid')} /><//>`}
    <${Row} label=${t('Text size')}><${NumberField} value=${ch.fontSize} min=${6} max=${96} unit="px" onChange=${(v, live) => set_({ fontSize: v }, 'fs', live)} /><//>
    <${Row} label=${t('Text color')}><${ColorField} value=${ch.color} noneLabel=${t('Theme text color')} onChange=${(v, live) => set_({ color: v }, 'color', live)} /><//>
    ${axes &&
    html`<div class="chart-axes">
      <${Row} label=${kind === 'bar' || kind === 'stacked-bar' ? t('X axis') : t('Y axis')} hint=${t('Leave empty to follow the data.')}>
        <div class="row-inline">
          <${NumberField} label=${t('axis minimum::Min')} nullable placeholder=${t('Auto')} value=${ch.axisMin ?? null} step=${step} precision=${6} onChange=${(v, live) => set_({ axisMin: v }, 'amin', live)} />
          <${NumberField} label=${t('axis maximum::Max')} nullable placeholder=${t('Auto')} value=${ch.axisMax ?? null} step=${step} precision=${6} onChange=${(v, live) => set_({ axisMax: v }, 'amax', live)} />
        </div>
      <//>
      <${Row} label=${t('X-axis title')}><${TextField} value=${ch.xTitle || ''} onChange=${(v) => set_({ xTitle: v }, 'xt')} /><//>
      <${Row} label=${t('Y-axis title')}><${TextField} value=${ch.yTitle || ''} onChange=${(v) => set_({ yTitle: v }, 'yt')} /><//>
      ${refLines.length > 0 && html`<label class="field-label">${t('Reference lines')}</label>`}
      ${refLines.map(
        (l, i) => html`<div class="ref-line">
          <${NumberField} value=${l.value} step=${step} precision=${6} title=${t('Value')} onChange=${(v, live) => setRef(i, { value: v }, live)} />
          <${TextField} value=${l.label} placeholder=${t('Label')} onChange=${(v) => setRef(i, { label: v })} />
          <${Button} icon="x" size="sm" variant="ghost" title=${t('Remove line')} onClick=${() => delRef(i)} />
        </div>`,
      )}
      <div class="row-inline">
        <${Button} size="sm" icon="plus" label=${t('Add Reference Line')} title=${t('A dashed line across the chart at a value, such as a target.')} onClick=${addRef} />
      </div>
    </div>`}
  <//>`;
}

/** Arrow keys and scrubbing in a chart's value fields move by about a tenth of its data's spread. */
function axisStep(c) {
  const values = c.series.flatMap((s) => s.values).filter(Number.isFinite);
  const spread = values.length ? Math.max(...values) - Math.min(...values) : 0;
  return spread > 0 ? 10 ** Math.floor(Math.log10(spread / 10)) : 1;
}

/** Where a new reference line starts: the average of the data, rounded to its step. */
function typicalValue(c) {
  const values = c.series.flatMap((s) => s.values).filter(Number.isFinite);
  if (!values.length) return 0;
  const step = axisStep(c);
  return Number((Math.round(values.reduce((a, b) => a + b, 0) / values.length / step) * step).toFixed(10));
}

// ---------------------------------------------------------------------------
// Arrange & link (all elements)
// ---------------------------------------------------------------------------

export function ArrangePanel({ els }) {
  const single = els.length === 1 ? els[0] : null;
  const geo = (k, v, live) => patchAll(els, { [k]: v }, live ? `geo-${k}` : null);
  const opacity = same(els, (e) => e.opacity);
  const locked = els.every((e) => e.locked);
  return html`<${Section} title=${t('Arrange')} id="arrange">
    ${single &&
    html`<div class="geo-grid">
      <${NumberField} label="X" value=${single.x} onChange=${(v, live) => geo('x', v, live)} />
      <${NumberField} label="Y" value=${single.y} onChange=${(v, live) => geo('y', v, live)} />
      <${NumberField} label=${t('W')} value=${single.w} min=${4} onChange=${(v, live) => geo('w', v, live)} />
      <${NumberField}
        label=${t('H')}
        value=${single.h}
        min=${4}
        onChange=${(v, live) =>
          // A height of its own: the box stops fitting its text.
          fitsText(single) ? patchAll(els, { h: v, content: { style: { autoHeight: false } } }, live ? 'geo-h' : null) : geo('h', v, live)}
      />
      <${NumberField} label="°" value=${single.rotation} min=${-360} max=${360} title=${t('Rotation')} onChange=${(v, live) => geo('rotation', v, live)} />
    </div>`}
    <${Row} label=${t('Opacity')}><${Slider} value=${Math.round((opacity ?? 1) * 100)} min=${0} max=${100} unit="%" onChange=${(v, live) => geo('opacity', v / 100, live)} /><//>
    <div class="icon-row">
      ${[
        ['arrange.front', 'bring-to-front', () => A.arrange('front')],
        ['arrange.forward', 'arrow-up', () => A.arrange('forward')],
        ['arrange.backward', 'arrow-down', () => A.arrange('backward')],
        ['arrange.back', 'send-to-back', () => A.arrange('back')],
      ].map(([cmd, icon, run]) => html`<${Button} icon=${icon} size="sm" title=${{ 'arrange.front': t('Bring to front'), 'arrange.forward': t('Bring forward'), 'arrange.backward': t('Send backward'), 'arrange.back': t('Send to back') }[cmd]} onClick=${run} />`)}
      <span class="spacer" />
      <${Button} icon=${locked ? 'lock' : 'lock-open'} size="sm" active=${locked} title=${locked ? t('Unlock') : t('Lock')} onClick=${A.toggleLock} />
    </div>
    <div class="icon-row">
      ${[
        ['left', 'align-start-vertical', t('Align left'), t('Align left (to slide)')],
        ['hcenter', 'align-center-vertical', t('Align centers'), t('Align centers (to slide)')],
        ['right', 'align-end-vertical', t('Align right'), t('Align right (to slide)')],
        ['top', 'align-start-horizontal', t('Align tops'), t('Align tops (to slide)')],
        ['vcenter', 'align-center-horizontal', t('Align middles'), t('Align middles (to slide)')],
        ['bottom', 'align-end-horizontal', t('Align bottoms'), t('Align bottoms (to slide)')],
      ].map(([m, icon, label, toSlide]) => html`<${Button} icon=${icon} size="sm" title=${single ? toSlide : label} onClick=${() => A.align(m)} />`)}
    </div>
    ${els.length > 2 &&
    html`<div class="icon-row">
      <${Button} icon="align-horizontal-distribute-center" size="sm" label=${t('Distribute horizontally')} onClick=${() => A.align('distribute-h')} />
      <${Button} icon="align-vertical-distribute-center" size="sm" label=${t('Vertically')} onClick=${() => A.align('distribute-v')} />
    </div>`}
  <//>`;
}

export function LinkPanel({ els }) {
  const kind = same(els, (e) => e.action.kind) ?? 'none';
  const url = same(els, (e) => e.action.url) ?? '';
  const slide = same(els, (e) => e.action.slide) ?? '';
  const set_ = (patch) => patchAll(els, { action: patch }, null);
  const slides = deck().slides;
  return html`<${Section} title=${t('Link')} id="link" open=${false}>
    <${Row} label=${t('On click')}>
      <${Select}
        value=${kind}
        options=${[
          { value: 'none', label: t('Nothing') },
          { value: 'url', label: t('Open web page') },
          { value: 'slide', label: t('Go to slide') },
        ]}
        onChange=${(v) => set_({ kind: v })}
      />
    <//>
    ${kind === 'url' && html`<${Row} label=${t('Address')} stack><${TextField} value=${url} placeholder="https://…" onChange=${(v) => set_({ url: v.trim() })} /><//>`}
    ${kind === 'slide' &&
    html`<${Row} label=${t('Slide')}>
      <${Select}
        value=${slide}
        options=${[{ value: '', label: t('Choose…') }, ...slides.map((s, i) => ({ value: s.id, label: `${i + 1}. ${slideTitle(s)}` }))]}
        onChange=${(v) => set_({ slide: v })}
      />
    <//>`}
  <//>`;
}

/** A short label for a slide (its title text or layout). */
export function slideTitle(s) {
  for (const e of s.elements) {
    const text = e.content.kind === 'text' ? e.content : null;
    if (text && text.html) {
      const txt = text.html.replace(/<[^>]+>/g, ' ').replace(/&nbsp;/g, ' ').replace(/\s+/g, ' ').trim();
      if (txt) return txt.length > 40 ? txt.slice(0, 40) + '…' : txt;
    }
  }
  return t('Untitled slide');
}

export { patchAll, same, themeInfo, set, currentSlide };

// ---------------------------------------------------------------------------
// Diagrams (Mermaid)
// ---------------------------------------------------------------------------

const DIAGRAM_STYLE_OPTIONS = [
  { value: 'classic', label: N_('Classic') },
  { value: 'neutral', label: N_('Neutral') },
  { value: 'forest', label: N_('Forest') },
  { value: 'neo', label: N_('Neo') },
  { value: 'redux', label: N_('Redux') },
  { value: 'redux-color', label: N_('Redux (color)') },
];

function diagramError(e) {
  const msg = String(e?.message || e?.str || e || t('Invalid diagram'));
  return msg.replace(/^Error:\s*/, '').split('\n').slice(0, 6).join('\n');
}

export function DiagramPanel({ els }) {
  const el = els[0];
  const d = el.content;
  const ref = useRef();
  useFocusPrimary(ref);
  const [error, setError] = useState(null);
  const [busy, setBusy] = useState(false);
  const update = async (patch, live) => {
    const next = { ...d, ...patch };
    setBusy(true);
    try {
      const svgs = await renderDiagram(next.source, next.style, next.look);
      setError(null);
      await patchAll([el], { content: { ...patch, ...svgs } }, live ? 'diagram' : null);
    } catch (e) {
      setError(diagramError(e));
      // Keep what was typed even while it does not parse yet.
      if ('source' in patch) await patchAll([el], { content: { source: patch.source } }, live ? 'diagram' : null);
    } finally {
      setBusy(false);
    }
  };
  const fit = () => {
    const a = svgAspect(d.svgLight || d.svgDark);
    if (a) patchAll([el], { h: Math.max(20, Math.round(el.w / a)) }, null);
  };
  return html`<${Section} title=${t('Diagram')} id="diagram">
    <${Row} label=${t('Template')}>
      <${Select}
        value=""
        options=${[{ value: '', label: t('Start from a template…') }, ...DIAGRAM_TEMPLATES.map(([name], i) => ({ value: String(i), label: t(name) }))]}
        onChange=${(v) => v !== '' && update({ source: DIAGRAM_TEMPLATES[Number(v)][1] })}
      />
    <//>
    <div ref=${ref}>
      <${TextField} multiline mono rows=${12} live value=${d.source} onChange=${(v, live) => update({ source: v }, live)} />
    </div>
    ${busy && html`<div class="ihint"><span class="spinner small" /> ${t('Rendering…')}</div>`}
    ${error && html`<pre class="diagram-error">${error}</pre>`}
    <${Hint}>
      ${t('Write Mermaid syntax: flowcharts, sequence, class, state, ER, Gantt, pie, mindmaps, timelines and more.')} <a href="#" onClick=${(e) => { e.preventDefault(); opener.openUrl('https://mermaid.js.org/intro/'); }}>${t('Learn more')}</a>
    </${Hint}>
    <${Row} label=${t('Style')}><${Select} value=${d.style} options=${DIAGRAM_STYLE_OPTIONS.map((o) => ({ ...o, label: t(o.label) }))} onChange=${(v) => update({ style: v })} /><//>
    <${Row} label=${t('Look')}>
      <${Segmented}
        value=${d.look}
        options=${[
          { value: 'classic', label: t('Classic') },
          { value: 'handDrawn', label: t('Sketch') },
          { value: 'neo', label: t('Neo') },
        ]}
        onChange=${(v) => update({ look: v })}
      />
    <//>
    <${Row} label=${t('Colors')}>
      <${Segmented}
        value=${d.mode}
        options=${[
          { value: 'auto', label: t('Auto') },
          { value: 'light', label: t('Light') },
          { value: 'dark', label: t('Dark') },
        ]}
        onChange=${(v) => patchAll([el], { content: { mode: v } }, null)}
      />
    <//>
    <div class="row-inline"><${Button} size="sm" icon="ratio" label=${t('Fit Box to Diagram')} onClick=${fit} /></div>
  <//>`;
}
