// Inspector panels for elements.

import { html, useEffect, useRef, useState } from './lib.js';
import { Icon } from './icons.js';
import { state, set, apply, updateElements, deck, currentSlide, richOf, richPatch, themeInfo } from './store.js';
import { Section, Row, Hint, NumberField, Slider, Select, Segmented, Toggle, TextField, ColorField, FontField } from './controls.js';
import * as A from './actions.js';
import { exec, setLink, isEditing, queryState, beginTextEdit, endTextEdit, beginTableEdit, insertInlineMath } from './textedit.js';
import { canvasApi } from './canvas.js';
import { cls } from './util.js';
import { Button } from './ui.js';
import { renderDiagram, svgAspect, DIAGRAM_TEMPLATES } from './diagrams.js';
import { opener } from './api.js';
import { t, N_ } from './i18n.js';

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
  const style = (el) => richOf(el).style;
  const setStyle = (patch, key, live) => patchAll(els, (el) => richPatch(el, { style: typeof patch === 'function' ? patch(el) : patch }), key && (live ? key : null));
  const role = same(els, (e) => style(e).role);
  const size = same(els, (e) => style(e).fontSize);
  const family = same(els, (e) => style(e).fontFamily);
  const color = same(els, (e) => style(e).color);
  const align = same(els, (e) => style(e).align);
  const valign = same(els, (e) => style(e).valign);
  const weight = same(els, (e) => style(e).fontWeight);
  const italic = same(els, (e) => style(e).italic);
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
  const inline = async (cmd, value) => {
    if (isEditing()) return exec(cmd, value);
    if (!single) return;
    beginTextEdit(canvasApi.view, single, null, { selectAll: true });
    exec(cmd, value);
    await endTextEdit();
  };

  const boldActive = editing ? queryState('bold') : weight >= 600;
  const italicActive = editing ? queryState('italic') : italic;

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
    <${Row} label=${t('Font')}><${FontField} value=${family ?? ''} onChange=${(v) => setStyle({ fontFamily: v }, 'font')} /><//>
    <${Row} label=${t('Size')}>
      <div class="row-inline">
        <${NumberField} value=${size} min=${4} max=${800} unit="px" onChange=${(v, live) => setStyle({ fontSize: v }, 'fontSize', live)} />
        <div class="btn-group">
          <${Button} icon="minus" size="sm" title=${t('Smaller')} onClick=${() => setStyle((el) => ({ fontSize: Math.max(4, Math.round(style(el).fontSize / 1.12)) }), 'fontSize', true)} />
          <${Button} icon="plus" size="sm" title=${t('Larger')} onClick=${() => setStyle((el) => ({ fontSize: Math.round(style(el).fontSize * 1.12) }), 'fontSize', true)} />
        </div>
      </div>
    <//>
    <${Row} label=${t('Color')}>
      <${ColorField}
        value=${color ?? ''}
        noneLabel=${t('Theme color')}
        onChange=${(v, live) => {
          const sel = window.getSelection();
          if (isEditing() && sel && !sel.isCollapsed && v) exec('foreColor', v);
          else setStyle({ color: v }, 'color', live);
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
          if (v === 'b') isEditing() ? exec('bold') : setStyle({ fontWeight: weight >= 600 ? 0 : 700 }, 'bold');
          if (v === 'i') isEditing() ? exec('italic') : setStyle({ italic: !italic }, 'italic');
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
    <${Row} label=${t('vertical alignment::Vertical')}>
      <${Segmented}
        value=${valign}
        options=${[
          { value: 'top', icon: 'align-start-horizontal', title: t('Top') },
          { value: 'middle', icon: 'align-center-horizontal', title: t('Middle') },
          { value: 'bottom', icon: 'align-end-horizontal', title: t('Bottom') },
        ]}
        onChange=${(v) => setStyle({ valign: v }, 'valign')}
      />
    <//>
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
    ${isText && html`<${Row}><${Toggle} value=${auto} label=${t('Grow box to fit text')} onChange=${(v) => setStyle({ autoHeight: v }, 'auto')} /><//>`}
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
      <input class="textfield" placeholder=${t('Link address…')} value=${url} onInput=${(e) => setUrl(e.target.value)} onKeyDown=${(e) => e.key === 'Enter' && setLink(url)} />
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

export function ChartPanel({ els }) {
  const el = els[0];
  const ch = el.content;
  const ref = useRef();
  useFocusPrimary(ref);
  const set_ = (patch, key, live) => patchAll([el], { content: patch }, live ? key : null);
  const pie = ch.chart === 'pie' || ch.chart === 'donut';
  const n = ch.labels.length;
  const setCell = (s, i, v) => {
    const series = ch.series.map((x, k) => (k === s ? { ...x, values: Array.from({ length: n }, (_, j) => (j === i ? v : x.values[j] ?? 0)) } : x));
    set_({ series }, 'data', true);
  };
  const setLabel = (i, v) => set_({ labels: ch.labels.map((l, j) => (j === i ? v : l)) }, 'labels', true);
  const addRow = () =>
    set_({ labels: [...ch.labels, t('Item {n}', { n: n + 1 })], series: ch.series.map((s) => ({ ...s, values: [...s.values.slice(0, n), 0] })) }, 'rows');
  const delRow = (i) => set_({ labels: ch.labels.filter((_, j) => j !== i), series: ch.series.map((s) => ({ ...s, values: s.values.filter((_, j) => j !== i) })) }, 'rows');
  const addSeries = () => set_({ series: [...ch.series, { name: t('Series {n}', { n: ch.series.length + 1 }), values: ch.labels.map(() => 0), color: '' }] }, 'series');
  const delSeries = (k) => set_({ series: ch.series.filter((_, j) => j !== k) }, 'series');
  const shownSeries = pie ? ch.series.slice(0, 1) : ch.series;
  return html`<${Section} title=${t('Chart')} id="chart">
    <div class="chart-types">
      ${state.info.charts.map(
        (c) => html`<button type="button" class=${cls('chart-type', ch.chart === c.id && 'active')} title=${t(c.name)} onClick=${() => set_({ chart: c.id }, 'type')}>
          <${Icon} name=${{ column: 'chart-column', bar: 'chart-bar', line: 'chart-line', area: 'chart-line', pie: 'chart-pie', donut: 'chart-pie' }[c.id]} size=${18} />
          <span>${t(c.name)}</span>
        </button>`,
      )}
    </div>
    <div class="datagrid" ref=${ref}>
      <table>
        <thead>
          <tr>
            <th></th>
            ${shownSeries.map(
              (s, k) => html`<th>
                <input class="cell head" value=${s.name} onChange=${(e) => set_({ series: ch.series.map((x, j) => (j === k ? { ...x, name: e.target.value } : x)) }, 'sname')} />
                ${!pie && ch.series.length > 1 && html`<button class="cell-x" title=${t('Remove series')} onClick=${() => delSeries(k)}>×</button>`}
              </th>`,
            )}
            ${!pie && html`<th><button class="cell-add" title=${t('Add series')} onClick=${addSeries}>+</button></th>`}
          </tr>
        </thead>
        <tbody>
          ${ch.labels.map(
            (l, i) => html`<tr>
              <td><input class="cell label" value=${l} onChange=${(e) => setLabel(i, e.target.value)} /></td>
              ${shownSeries.map(
                (s, k) => html`<td>
                  <input class="cell num" type="number" value=${s.values[i] ?? 0} onChange=${(e) => setCell(k, i, Number(e.target.value) || 0)} />
                </td>`,
              )}
              <td><button class="cell-x" title=${t('Remove row')} onClick=${() => delRow(i)}>×</button></td>
            </tr>`,
          )}
        </tbody>
      </table>
      <button class="cell-add wide" onClick=${addRow}>${t('+ Add row')}</button>
    </div>
    ${!pie &&
    html`<div class="series-colors">
      ${ch.series.map(
        (s, k) => html`<${Row} label=${s.name}>
          <${ColorField}
            value=${s.color}
            noneLabel=${t('Theme color {n}', { n: (k % 6) + 1 })}
            onChange=${(v, live) => set_({ series: ch.series.map((x, j) => (j === k ? { ...x, color: v } : x)) }, 'scolor', live)}
          />
        <//>`,
      )}
    </div>`}
    <${Row}><${Toggle} value=${ch.showLegend} label=${t('Legend')} onChange=${(v) => set_({ showLegend: v }, 'legend')} /><//>
    <${Row}><${Toggle} value=${ch.showValues} label=${t('Value labels')} onChange=${(v) => set_({ showValues: v }, 'values')} /><//>
    ${!pie && html`<${Row}><${Toggle} value=${ch.showGrid} label=${t('Grid lines')} onChange=${(v) => set_({ showGrid: v }, 'grid')} /><//>`}
    ${['column', 'bar', 'area'].includes(ch.chart) && html`<${Row}><${Toggle} value=${ch.stacked} label=${t('Stacked')} onChange=${(v) => set_({ stacked: v }, 'stacked')} /><//>`}
    <${Row} label=${t('Text size')}><${NumberField} value=${ch.fontSize} min=${6} max=${96} unit="px" onChange=${(v, live) => set_({ fontSize: v }, 'fs', live)} /><//>
    <${Row} label=${t('Text color')}><${ColorField} value=${ch.color} noneLabel=${t('Theme text color')} onChange=${(v, live) => set_({ color: v }, 'color', live)} /><//>
  <//>`;
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
      <${NumberField} label=${t('H')} value=${single.h} min=${4} onChange=${(v, live) => geo('h', v, live)} />
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
