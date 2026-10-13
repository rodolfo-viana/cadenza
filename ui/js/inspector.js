// The inspector: Format, Animate and Document tabs.

import { html } from './lib.js';
import { Icon } from './icons.js';
import { state, set, apply, useStore, currentSlide, selectedElements, updateSlides, updateDeck, deck, themeInfo } from './store.js';
import { Section, Row, Hint, NumberField, Slider, Select, Segmented, Toggle, TextField, ColorField, FontField } from './controls.js';
import { TextPanel, BoxPanel, ShapePanel, ImagePanel, VideoPanel, EmbedPanel, CodePanel, MathPanel, TablePanel, ChartPanel, DiagramPanel, ArrangePanel, LinkPanel, slideTitle } from './panels.js';
import * as A from './actions.js';
import { Button } from './ui.js';
import { cardFromDef, openThemeBuilder, applyCustomTheme, importTheme, themeMenu } from './themebuilder.js';
import { AssistantPanel, aiEnabled } from './ai.js';
import { cls } from './util.js';
import { t, tn, N_ } from './i18n.js';

export function Inspector() {
  const st = useStore();
  const els = selectedElements();
  const tabs = [
    { id: 'format', label: t('Format'), icon: 'paintbrush' },
    { id: 'animate', label: t('Animate'), icon: 'sparkles' },
    { id: 'document', label: t('Document'), icon: 'file-text' },
  ];
  if (aiEnabled()) tabs.push({ id: 'assistant', label: t('Assistant'), icon: 'wand-sparkles', iconOnly: true });
  const tab = st.tab === 'assistant' && !aiEnabled() ? 'format' : st.tab;
  return html`<aside class="inspector">
    <div class="tabs">
      ${tabs.map(
        (tabDef) => html`<button type="button" class=${cls('tab', tab === tabDef.id && 'active', tabDef.iconOnly && 'icon-only')} title=${tabDef.label} onClick=${() => set({ tab: tabDef.id })}>
          <${Icon} name=${tabDef.icon} size=${15} />${!tabDef.iconOnly && html`<span class="tab-label">${tabDef.label}</span>`}
        </button>`,
      )}
    </div>
    ${tab === 'assistant'
      ? html`<div class="inspector-body assistant" key="assistant"><${AssistantPanel} /></div>`
      : html`<div class="inspector-body" key=${tab + ':' + st.slideId}>
          ${tab === 'format' && (els.length ? html`<${ElementFormat} els=${els} />` : html`<${SlideFormat} />`)}
          ${tab === 'animate' && html`<${AnimatePanel} els=${els} />`}
          ${tab === 'document' && html`<${DocumentPanel} />`}
        </div>`}
  </aside>`;
}

const KIND_LABEL = {
  text: N_('Text'),
  shape: N_('Shape'),
  image: N_('Image'),
  video: N_('Video'),
  code: N_('Code'),
  math: N_('Equation'),
  embed: N_('Web Page'),
  table: N_('Table'),
  chart: N_('Chart'),
  diagram: N_('Diagram'),
};

/** An element's display name: its own name, or else the label of its kind (translated). */
const elementName = (el) => (el.name ? t(el.name) : KIND_LABEL[el.content.kind] && t(KIND_LABEL[el.content.kind]));

/** Translates the labels (and group names) of a Select/Segmented option list defined with N_. */
const translateOptions = (options) =>
  options.map((o) => (o.group ? { ...o, group: t(o.group), options: translateOptions(o.options) } : { ...o, label: t(o.label) }));

function ElementFormat({ els }) {
  const kinds = [...new Set(els.map((e) => e.content.kind))];
  const k = kinds.length === 1 ? kinds[0] : null;
  const head = els.length > 1 ? tn('{n} object selected', '{n} objects selected', els.length) : elementName(els[0]);
  const single = els.length === 1;
  return html`
    <div class="insp-title">
      <span>${head}</span>
      ${els.some((e) => e.locked) && html`<span class="pill"><${Icon} name="lock" size=${11} /> ${t('Locked')}</span>`}
    </div>
    ${k === 'text' && html`<${TextPanel} els=${els} /><${BoxPanel} els=${els} />`}
    ${k === 'shape' &&
    html`<${ShapePanel} els=${els} />${els.every((e) => e.content.shape !== 'line') && html`<${TextPanel} els=${els} title=${t('Text in shape')} />`}`}
    ${k === 'image' && html`<${ImagePanel} els=${els} /><${BoxPanel} els=${els} title=${t('Frame')} fill=${false} padding=${false} />`}
    ${k === 'video' && html`<${VideoPanel} els=${els} /><${BoxPanel} els=${els} title=${t('Frame')} fill=${false} padding=${false} />`}
    ${k === 'embed' && html`<${EmbedPanel} els=${els} /><${BoxPanel} els=${els} title=${t('Frame')} fill=${false} padding=${false} />`}
    ${k === 'code' && html`<${CodePanel} els=${els} />`}
    ${k === 'math' && html`<${MathPanel} els=${els} /><${BoxPanel} els=${els} />`}
    ${k === 'table' && single && html`<${TablePanel} els=${els} /><${BoxPanel} els=${els} title=${t('Frame')} padding=${false} />`}
    ${k === 'chart' && single && html`<${ChartPanel} els=${els} /><${BoxPanel} els=${els} />`}
    ${k === 'diagram' && single && html`<${DiagramPanel} els=${els} /><${BoxPanel} els=${els} />`}
    ${!k && html`<${BoxPanel} els=${els} />`}
    <${ArrangePanel} els=${els} />
    <${LinkPanel} els=${els} />
  `;
}

// ---------------------------------------------------------------------------
// Slide (nothing selected)
// ---------------------------------------------------------------------------

const BG_KINDS = [
  { value: 'theme', label: N_('Theme') },
  { value: 'color', label: N_('Color') },
  { value: 'gradient', label: N_('Gradient') },
  { value: 'image', label: N_('Image') },
  { value: 'video', label: N_('Video') },
];

function SlideFormat() {
  const slide = currentSlide();
  if (!slide) return null;
  const ids = state.selectedSlides.includes(slide.id) ? state.selectedSlides : [slide.id];
  const bg = slide.background;
  const setBg = (patch, key, live) => updateSlides(ids, { background: patch }, live ? `bg-${key}:${ids}` : null);
  const many = ids.length > 1;
  return html`
    <div class="insp-title"><span>${many ? tn('{n} slide', '{n} slides', ids.length) : t('Slide {n}', { n: state.doc.deck.slides.indexOf(slide) + 1 })}</span><span class="muted">${slideTitle(slide)}</span></div>
    <${Section} title=${t('Background')} id="bg">
      <${Segmented} full value=${bg.kind} options=${translateOptions(BG_KINDS)} onChange=${(v) => setBg({ kind: v }, 'kind')} />
      ${bg.kind === 'theme' && html`<${Hint}>${t('Uses the background of the “{theme}” theme.', { theme: themeInfo(deck().theme)?.name })}</${Hint}>`}
      ${bg.kind === 'color' && html`<${Row} label=${t('Color')}><${ColorField} value=${bg.color} allowNone=${false} onChange=${(v, live) => setBg({ color: v }, 'color', live)} /><//>`}
      ${bg.kind === 'gradient' &&
      html`
        <div class="gradient-preview" style=${{ background: bg.gradient.kind === 'radial' ? `radial-gradient(circle, ${bg.gradient.from}, ${bg.gradient.to})` : `linear-gradient(${bg.gradient.angle}deg, ${bg.gradient.from}, ${bg.gradient.to})` }} />
        <${Row} label=${t('From')}><${ColorField} value=${bg.gradient.from} allowNone=${false} onChange=${(v, live) => setBg({ gradient: { from: v } }, 'gfrom', live)} /><//>
        <${Row} label=${t('To')}><${ColorField} value=${bg.gradient.to} allowNone=${false} onChange=${(v, live) => setBg({ gradient: { to: v } }, 'gto', live)} /><//>
        <${Row} label=${t('Type')}>
          <${Segmented}
            value=${bg.gradient.kind}
            options=${[
              { value: 'linear', label: t('Linear') },
              { value: 'radial', label: t('Radial') },
            ]}
            onChange=${(v) => setBg({ gradient: { kind: v } }, 'gkind')}
          />
        <//>
        ${bg.gradient.kind !== 'radial' && html`<${Row} label=${t('Angle')}><${Slider} value=${bg.gradient.angle} min=${0} max=${360} unit="°" onChange=${(v, live) => setBg({ gradient: { angle: v } }, 'gangle', live)} /><//>`}
      `}
      ${bg.kind === 'image' &&
      html`
        <div class="row-inline">
          <${Button} icon="image-up" label=${bg.image ? t('Replace Image…') : t('Choose Image…')} onClick=${() => A.chooseBackgroundImage(ids)} />
        </div>
        <${Row} label=${t('Size')}>
          <${Segmented}
            value=${bg.size}
            options=${[
              { value: 'cover', label: t('image fit::Fill') },
              { value: 'contain', label: t('image fit::Fit') },
              { value: 'auto', label: t('Original') },
            ]}
            onChange=${(v) => setBg({ size: v }, 'size')}
          />
        <//>
        <${Row} label=${t('Position')}>
          <${Select}
            value=${bg.position}
            options=${[
              { value: 'center', label: t('Center') },
              { value: 'top', label: t('Top') },
              { value: 'bottom', label: t('Bottom') },
              { value: 'left', label: t('Left') },
              { value: 'right', label: t('Right') },
              { value: 'top left', label: t('Top left') },
              { value: 'top right', label: t('Top right') },
              { value: 'bottom left', label: t('Bottom left') },
              { value: 'bottom right', label: t('Bottom right') },
            ]}
            onChange=${(v) => setBg({ position: v }, 'pos')}
          />
        <//>
        <${Row}><${Toggle} value=${bg.repeat} label=${t('Tile')} onChange=${(v) => setBg({ repeat: v }, 'repeat')} /><//>
        <${Row} label=${t('Opacity')}><${Slider} value=${Math.round(bg.opacity * 100)} min=${0} max=${100} unit="%" onChange=${(v, live) => setBg({ opacity: v / 100 }, 'op', live)} /><//>
      `}
      ${bg.kind === 'video' &&
      html`
        <div class="row-inline"><${Button} icon="film" label=${bg.video ? t('Replace Video…') : t('Choose Video…')} onClick=${() => A.chooseBackgroundVideo(ids)} /></div>
        <${Row}><${Toggle} value=${bg.videoLoop} label=${t('Loop')} onChange=${(v) => setBg({ videoLoop: v }, 'vl')} /><//>
        <${Row}><${Toggle} value=${bg.videoMuted} label=${t('Muted')} onChange=${(v) => setBg({ videoMuted: v }, 'vm')} /><//>
        <${Row} label=${t('Opacity')}><${Slider} value=${Math.round(bg.opacity * 100)} min=${0} max=${100} unit="%" onChange=${(v, live) => setBg({ opacity: v / 100 }, 'op', live)} /><//>
      `}
      <div class="row-inline">
        <${Button}
          size="sm"
          label=${t('Apply to All Slides')}
          onClick=${() =>
            updateSlides(
              deck().slides.map((s) => s.id),
              { background: bg },
            )}
        />
      </div>
    <//>
    <${Section} title=${t('Slide')} id="slide">
      <${Row}><${Toggle} value=${slide.hidden} label=${t('Skip during presentation')} onChange=${(v) => updateSlides(ids, { hidden: v })} /><//>
      <${Row}>
        <${Toggle}
          value=${slide.nested}
          label=${t('Vertical sub-slide')}
          hint=${t('Stacks this slide below the previous one (reveal.js vertical slides).')}
          onChange=${(v) => A.setNested(v, ids)}
        />
      <//>
      <${Hint}>${t('Sub-slides form a vertical stack: navigate down with ↓ during the show. Press Tab in the slide list to nest.')}</${Hint}>
    <//>
    <${Section} title=${t('Insert')} id="insert-quick">
      <div class="quick-insert">
        ${[
          ['type', t('Text'), () => A.insert('text', 'body')],
          ['square', t('Shape'), () => A.insert('shape', 'rect')],
          ['image', t('Image'), A.insertImage],
          ['table', t('Table'), () => A.insert('table')],
          ['chart-column', t('Chart'), () => A.insert('chart', 'column')],
          ['code', t('Code'), () => A.insert('code')],
          ['sigma', t('Equation'), () => A.insert('math')],
          ['workflow', t('Diagram'), () => A.insert('diagram')],
          ['square-play', t('Video'), A.insertOnlineVideo],
        ].map(
          ([icon, label, run]) => html`<button type="button" class="qi" onClick=${run}>
            <${Icon} name=${icon} size=${20} /><span>${label}</span>
          </button>`,
        )}
      </div>
    <//>
  `;
}

// ---------------------------------------------------------------------------
// Animate
// ---------------------------------------------------------------------------

const BUILD_IN = [
  { group: N_('Build In'), options: [
    { value: '', label: N_('None') },
    { value: 'fade-in', label: N_('Appear (fade)') },
    { value: 'fade-up', label: N_('Fade up') },
    { value: 'fade-down', label: N_('Fade down') },
    { value: 'fade-left', label: N_('Fade from right') },
    { value: 'fade-right', label: N_('Fade from left') },
    { value: 'zoom-in', label: N_('Zoom in') },
    { value: 'fade-in-then-out', label: N_('Appear, then disappear') },
    { value: 'fade-in-then-semi-out', label: N_('Appear, then dim') },
  ] },
  { group: N_('Emphasis'), options: [
    { value: 'grow', label: N_('Grow') },
    { value: 'shrink', label: N_('Shrink') },
    { value: 'semi-fade-out', label: N_('Dim') },
    { value: 'strike', label: N_('Strike through') },
    { value: 'highlight-red', label: N_('Highlight red') },
    { value: 'highlight-green', label: N_('Highlight green') },
    { value: 'highlight-blue', label: N_('Highlight blue') },
  ] },
];

const BUILD_OUT = [
  { value: '', label: N_('None') },
  { value: 'fade-out', label: N_('Fade out') },
  { value: 'semi-fade-out', label: N_('Dim') },
];

const TRANSITIONS = [
  { value: '', label: N_('Presentation default') },
  { value: 'none', label: N_('None') },
  { value: 'fade', label: N_('Dissolve (fade)') },
  { value: 'slide', label: N_('Push (slide)') },
  { value: 'convex', label: N_('Cube (convex)') },
  { value: 'concave', label: N_('Reveal (concave)') },
  { value: 'zoom', label: N_('Zoom') },
];

const effectLabel = (v) => {
  const label = [...BUILD_IN.flatMap((g) => g.options), ...BUILD_OUT].find((o) => o.value === v)?.label;
  return label ? t(label) : v;
};

function AnimatePanel({ els }) {
  const slide = currentSlide();
  if (!slide) return null;
  const builds = slide.elements.filter((e) => e.build.effect).sort((a, b) => (a.build.order || 999) - (b.build.order || 999));
  const nextOrder = () => Math.max(0, ...slide.elements.map((e) => Math.max(e.build.order, e.build.outOrder))) + 1;
  const setBuild = (patch) =>
    apply({ op: 'updateElements', slide: slide.id, updates: els.map((e, i) => ({ id: e.id, patch: { build: typeof patch === 'function' ? patch(e, i) : patch } })) });
  const one = els.length === 1 ? els[0] : null;
  const move = (i, d) => {
    const order = builds.map((e) => e.id);
    const j = i + d;
    if (j < 0 || j >= order.length) return;
    [order[i], order[j]] = [order[j], order[i]];
    apply({ op: 'setBuildOrder', slide: slide.id, ids: order });
  };
  return html`
    ${els.length > 0 &&
    html`<${Section} title=${t('Build')} id="build">
      <${Row} label=${t('Build in')}>
        <${Select}
          value=${one ? one.build.effect : ''}
          options=${translateOptions(BUILD_IN)}
          onChange=${(v) => {
            const base = nextOrder();
            setBuild((e, i) => ({ effect: v, order: v ? (e.build.order || base + i) : 0 }));
          }}
        />
      <//>
      ${one?.build.effect && html`<${Row} label=${t('Step')}><${NumberField} value=${one.build.order} min=${1} max=${99} onChange=${(v) => setBuild({ order: v })} /><//>`}
      <${Row} label=${t('Build out')}>
        <${Select}
          value=${one ? one.build.outEffect : ''}
          options=${translateOptions(BUILD_OUT)}
          onChange=${(v) => setBuild((e) => ({ outEffect: v, outOrder: v ? e.build.outOrder || Math.max(e.build.order + 1, nextOrder()) : 0 }))}
        />
      <//>
      ${one?.build.outEffect && html`<${Row} label=${t('Out step')}><${NumberField} value=${one.build.outOrder} min=${1} max=${99} onChange=${(v) => setBuild({ outOrder: v })} /><//>`}
      <${Hint}>${t('Builds play one click at a time during the presentation. Objects with the same step appear together.')}</${Hint}>
    <//>`}
    ${els.length === 0 &&
    html`<${Section} title=${t('Transition')} id="transition">
      <${Row} label=${t('Effect')}>
        <${Select} value=${slide.transition} options=${translateOptions(TRANSITIONS)} onChange=${(v) => updateSlides([slide.id], { transition: v })} />
      <//>
      <${Row} label=${t('Speed')}>
        <${Segmented}
          value=${slide.transitionSpeed || 'default'}
          options=${[
            { value: 'fast', label: t('Fast') },
            { value: 'default', label: t('Normal') },
            { value: 'slow', label: t('Slow') },
          ]}
          onChange=${(v) => updateSlides([slide.id], { transitionSpeed: v === 'default' ? '' : v })}
        />
      <//>
      <${Row}>
        <${Toggle} value=${slide.autoAnimate} label=${t('Magic Move')} hint=${t('Animate objects shared with the previous slide')} onChange=${(v) => updateSlides([slide.id], { autoAnimate: v })} />
      <//>
      <${Hint}>
        ${t('Magic Move (reveal.js auto-animate) smoothly moves and resizes objects that also appear on the previous slide. Tip: duplicate a slide, rearrange it, then turn this on for the copy.')}
      </${Hint}>
      <div class="row-inline">
        <${Button} size="sm" icon="wand-sparkles" label=${t('Duplicate for Magic Move')} onClick=${async () => {
          await A.duplicateSlides([slide.id]);
          const s = currentSlide();
          if (s && s.id !== slide.id) updateSlides([s.id], { autoAnimate: true });
        }} />
      </div>
      <${Row} label=${t('Auto-advance')}>
        <${NumberField} value=${slide.autoSlide} min=${0} max=${3600} step=${0.5} unit="s" title=${t('0 = use the presentation setting')} onChange=${(v) => updateSlides([slide.id], { autoSlide: v })} />
      <//>
    <//>`}
    <${Section} title=${t('Build Order ({n})', { n: builds.length })} id="build-order">
      ${builds.length === 0 && html`<${Hint}>${t('No builds on this slide. Select an object and choose a “Build in” effect.')}</${Hint}>`}
      <div class="build-list">
        ${builds.map(
          (e, i) => html`<div class=${cls('build-item', state.selection.includes(e.id) && 'active')} onClick=${() => set({ selection: [e.id] })}>
            <span class="build-step">${e.build.order || '–'}</span>
            <span class="build-name">
              ${elementName(e)}<small>${effectLabel(e.build.effect)}${e.build.outEffect ? ` · ${t('out: {effect}', { effect: effectLabel(e.build.outEffect) })}` : ''}</small>
            </span>
            <span class="build-move">
              <button title=${t('Earlier')} onClick=${(ev) => { ev.stopPropagation(); move(i, -1); }}><${Icon} name="chevron-up" size=${14} /></button>
              <button title=${t('Later')} onClick=${(ev) => { ev.stopPropagation(); move(i, 1); }}><${Icon} name="chevron-down" size=${14} /></button>
            </span>
          </div>`,
        )}
      </div>
    <//>
  `;
}

// ---------------------------------------------------------------------------
// Document
// ---------------------------------------------------------------------------

const SIZES = [
  { value: '1280x720', label: '16:9', title: N_('Widescreen 16:9 (1280×720)') },
  { value: '1280x800', label: '16:10', title: '16:10 (1280×800)' },
  { value: '1024x768', label: '4:3', title: N_('Standard 4:3 (1024×768)') },
  { value: '1080x1080', label: '1:1', title: N_('Square (1080×1080)') },
];

export function ThemeCard({ theme, active, onClick, onContextMenu, compact }) {
  return html`<button type="button" class=${cls('theme-card', active && 'active', compact && 'compact')} onClick=${onClick} onContextMenu=${onContextMenu} title=${theme.name}>
    <div class="theme-preview" style=${{ background: theme.previewBg }}>
      <div class="tp-title" style=${{ color: theme.headingColor, fontFamily: theme.headingFont, textTransform: theme.uppercase ? 'uppercase' : undefined }}>${t('Title')}</div>
      <div class="tp-text" style=${{ color: theme.textColor, fontFamily: theme.bodyFont }}>${t('Subtitle text')}</div>
      <div class="tp-dots">${theme.palette.slice(0, 4).map((c) => html`<span style=${{ background: c }} />`)}</div>
    </div>
    <span class="theme-name">${theme.name}</span>
  </button>`;
}

function DocumentPanel() {
  const d = deck();
  const s = d.settings;
  const setS = (patch) => updateDeck({ settings: patch });
  const size = `${d.width}x${d.height}`;
  const groups = ['Cadenza', 'reveal.js'];
  // One sentence with the selector in the middle: split the translation around the placeholder.
  const cssTargetHint = t('Target {selector} to style slides only.').split('{selector}');
  return html`
    <${Section} title=${t('Presentation')} id="doc-info">
      <${Row} label=${t('Title')} stack><${TextField} value=${d.title} onChange=${(v) => updateDeck({ title: v })} /><//>
      <${Row} label=${t('Author')} stack><${TextField} value=${d.author} placeholder=${t('Your name')} onChange=${(v) => updateDeck({ author: v })} /><//>
    <//>
    <${Section} title=${t('Theme')} id="doc-theme">
      <div class="row-inline">
        <${Button} size="sm" icon="swatch-book" label=${t('Customize…')} onClick=${() => openThemeBuilder()} />
        <${Button} size="sm" icon="upload" label=${t('Import…')} onClick=${async () => { const def = await importTheme(); if (def) applyCustomTheme(def); }} />
      </div>
      ${(() => {
        const mine = state.info.userThemes || [];
        const own = d.customTheme && !mine.some((theme) => theme.id === d.customTheme.id) ? [d.customTheme] : [];
        const all = [...own, ...mine];
        return all.length
          ? html`<div class="theme-group-label">${t('My themes')}</div>
              <div class="theme-grid">
                ${all.map(
                  (theme) =>
                    html`<${ThemeCard} theme=${cardFromDef(theme)} compact active=${theme.id === d.theme} onClick=${() => applyCustomTheme(theme)} onContextMenu=${(e) => themeMenu(e, theme)} />`,
                )}
              </div>`
          : null;
      })()}
      ${groups.map(
        (g) => html`<div class="theme-group-label">${g === 'Cadenza' ? t('Cadenza themes') : t('Classic reveal.js themes')}</div>
          <div class="theme-grid">
            ${state.info.themes
              .filter((theme) => theme.group === g)
              .map((theme) => html`<${ThemeCard} theme=${theme} compact active=${theme.id === d.theme} onClick=${() => updateDeck({ theme: theme.id })} />`)}
          </div>`,
      )}
    <//>
    <${Section} title=${t('Typography')} id="doc-fonts">
      <${Row} label=${t('Headings')}><${FontField} value=${d.headingFont} placeholder=${t('Theme heading font')} onChange=${(v) => updateDeck({ headingFont: v })} /><//>
      <${Row} label=${t('Body text')}><${FontField} value=${d.bodyFont} placeholder=${t('Theme body font')} onChange=${(v) => updateDeck({ bodyFont: v })} /><//>
      ${d.webFonts.length > 0 &&
      html`<div class="embedded-fonts">
        <div class="theme-group-label">${t('Embedded fonts')}</div>
        ${d.webFonts.map(
          (f) => html`<div class="embedded-font">
            <span style=${{ fontFamily: `'${f.family}'` }}>${f.family}</span>
            <button class="icon-btn" title=${t('Remove from presentation')} onClick=${() => updateDeck({ webFonts: d.webFonts.filter((x) => x.family !== f.family) })}><${Icon} name="x" size=${13} /></button>
          </div>`,
        )}
      </div>`}
      <${Hint}>${t('Pick any of 1,800 Google Fonts: they are embedded in the file, so they work offline and in exports.')}</${Hint}>
    <//>
    <${Section} title=${t('Slide Size')} id="doc-size">
      <${Segmented}
        full
        value=${SIZES.some((x) => x.value === size) ? size : ''}
        options=${SIZES.map((x) => ({ ...x, title: t(x.title) }))}
        onChange=${(v) => {
          const [w, h] = v.split('x').map(Number);
          apply({ op: 'resizeDeck', width: w, height: h }, { navigate: false });
        }}
      />
      <div class="row-inline">
        <${NumberField} label=${t('W')} value=${d.width} min=${320} max=${7680} onChange=${(w) => apply({ op: 'resizeDeck', width: w, height: d.height }, { navigate: false })} />
        <${NumberField} label=${t('H')} value=${d.height} min=${240} max=${4320} onChange=${(h) => apply({ op: 'resizeDeck', width: d.width, height: h }, { navigate: false })} />
      </div>
      <${Hint}>${t('Changing the size scales your content to fit. reveal.js scales slides to any screen.')}</${Hint}>
    <//>
    <${Section} title=${t('Playback')} id="doc-play">
      <${Row}><${Toggle} value=${s.controls} label=${t('Navigation arrows')} onChange=${(v) => setS({ controls: v })} /><//>
      <${Row}><${Toggle} value=${s.progress} label=${t('Progress bar')} onChange=${(v) => setS({ progress: v })} /><//>
      <${Row}><${Toggle} value=${s.slideNumber} label=${t('Slide numbers')} onChange=${(v) => setS({ slideNumber: v })} /><//>
      <${Row}><${Toggle} value=${s.loop} label=${t('Loop presentation')} onChange=${(v) => setS({ loop: v })} /><//>
      <${Row} label=${t('Auto-advance')}><${NumberField} value=${s.autoSlide} min=${0} max=${3600} step=${0.5} unit="s" title=${t('0 = off')} onChange=${(v) => setS({ autoSlide: v })} /><//>
      <${Row} label=${t('Navigation')}>
        <${Select}
          value=${s.navigationMode}
          options=${[
            { value: 'default', label: t('Default (←→ and ↑↓)') },
            { value: 'linear', label: t('Linear (all slides in a row)') },
            { value: 'grid', label: t('Grid') },
          ]}
          onChange=${(v) => setS({ navigationMode: v })}
        />
      <//>
      <${Row}><${Toggle} value=${s.view === 'scroll'} label=${t('Scroll view (web page style)')} onChange=${(v) => setS({ view: v ? 'scroll' : '' })} /><//>
    <//>
    <${Section} title=${t('Transitions')} id="doc-transitions">
      <${Row} label=${t('Slides')}>
        <${Select} value=${s.transition} options=${translateOptions(TRANSITIONS.slice(1))} onChange=${(v) => setS({ transition: v })} />
      <//>
      <${Row} label=${t('Speed')}>
        <${Segmented}
          value=${s.transitionSpeed}
          options=${[
            { value: 'fast', label: t('Fast') },
            { value: 'default', label: t('Normal') },
            { value: 'slow', label: t('Slow') },
          ]}
          onChange=${(v) => setS({ transitionSpeed: v })}
        />
      <//>
      <${Row} label=${t('Backgrounds')}>
        <${Select} value=${s.backgroundTransition} options=${translateOptions(TRANSITIONS.slice(1))} onChange=${(v) => setS({ backgroundTransition: v })} />
      <//>
      <${Row} label=${t('Magic Move')}><${NumberField} value=${s.autoAnimateDuration} min=${0.1} max=${10} step=${0.1} unit="s" onChange=${(v) => setS({ autoAnimateDuration: v })} /><//>
      <${Row} label=${t('Easing')}>
        <${Select}
          value=${s.autoAnimateEasing}
          options=${['ease', 'ease-in', 'ease-out', 'ease-in-out', 'linear', 'cubic-bezier(0.34, 1.56, 0.64, 1)'].map((v) => ({ value: v, label: v.startsWith('cubic') ? t('spring') : v }))}
          onChange=${(v) => setS({ autoAnimateEasing: v })}
        />
      <//>
    <//>
    <${Section} title=${t('Code Blocks')} id="doc-code">
      <${Row} label=${t('Highlighting')}>
        <${Segmented}
          value=${s.codeTheme}
          options=${[
            { value: 'monokai', label: 'Monokai' },
            { value: 'zenburn', label: 'Zenburn' },
          ]}
          onChange=${(v) => setS({ codeTheme: v })}
        />
      <//>
    <//>
    <${Section} title=${t('Custom CSS')} id="doc-css" open=${false}>
      <${TextField} multiline mono rows=${8} live value=${d.customCss} placeholder=${'.reveal .cz-role-title {\n  letter-spacing: -0.04em;\n}'} onChange=${(v) => updateDeck({ customCss: v }, 'css')} />
      <${Hint}>${t('Applied after the theme.')} ${cssTargetHint[0]}<code>.reveal</code>${cssTargetHint[1]}</${Hint}>
    <//>
  `;
}

export { KIND_LABEL };
