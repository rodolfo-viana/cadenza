// Builds and slide transitions. Document updates still go through the store's Ops.
import { html } from './lib.js';
import { Icon } from './icons.js';
import { state, set, apply, currentSlide, updateSlides } from './store.js';
import { Section, Row, Hint, NumberField, Select, Segmented, Toggle } from './controls.js';
import { Button } from './ui.js';
import { drawPath } from './canvas.js';
import { cls } from './util.js';
import { t, N_ } from './i18n.js';
import { buildUnit } from './charts.js';
import { elementName } from './element-label.js';
import { TransitionPreview } from './transition-preview-panel.js';
import * as A from './actions.js';

const translateOptions = (options) => options.map((o) => o.group
  ? { ...o, group: t(o.group), options: translateOptions(o.options) }
  : { ...o, label: t(o.label) });

const BUILD_IN = [
  { group: N_('Build In'), options: [
    { value: '', label: N_('None') },
    { value: 'fade-in', label: N_('Appear (fade)') },
    { value: 'fade-up', label: N_('Fade up') },
    { value: 'fade-down', label: N_('Fade down') },
    { value: 'fade-left', label: N_('Fade from right') },
    { value: 'fade-right', label: N_('Fade from left') },
    { value: 'cz-wipe-up', label: N_('Wipe from bottom') },
    { value: 'cz-wipe-down', label: N_('Wipe from top') },
    { value: 'cz-wipe-right', label: N_('Wipe from left') },
    { value: 'cz-wipe-left', label: N_('Wipe from right') },
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

// How a chart building step by step is described in the Build Order list.
const BY_UNIT = {
  slice: () => t('by slice'),
  party: () => t('by party'),
  item: () => t('by item'),
  column: () => t('by column'),
  series: () => t('by series'),
};

export function AnimatePanel({ els }) {
  const slide = currentSlide();
  if (!slide) return null;
  const events = A.buildEvents(slide);
  const nextOrder = () => A.nextBuildStep(slide);
  const setBuild = (patch) =>
    apply({ op: 'updateElements', slide: slide.id, updates: els.map((e, i) => ({ id: e.id, patch: { build: typeof patch === 'function' ? patch(e, i) : patch } })) });
  const one = els.length === 1 ? els[0] : null;
  const move = (i, d) => {
    const j = i + d;
    if (j < 0 || j >= events.length) return;
    const order = [...events];
    [order[i], order[j]] = [order[j], order[i]];
    A.setBuildOrder(slide, order);
  };
  const isBuildIn = (v) => BUILD_IN[0].options.some((o) => o.value === v && v);
  const unit = one?.content.kind === 'chart' ? buildUnit(one.content) : '';
  const series = one ? A.seriesSteps(one) : 0;
  const steps = { first: one?.build.order, last: (one?.build.order || 0) + series };
  const byUnit = {
    slice: [t('By slice'), t('The chart appears at step {first}, then one slice per step until step {last}.', steps)],
    party: [t('By party'), t('The chart appears at step {first}, then one party per step until step {last}.', steps)],
    item: [t('By item'), t('The chart appears at step {first}, then one item per step until step {last}.', steps)],
    column: [t('By column'), t('The chart appears at step {first}, then one column per step until step {last}.', steps)],
    series: [t('By series'), t('The chart appears at step {first}, then one series per step until step {last}.', steps)],
  }[unit];
  // Building a chart by series takes more steps: renumber if they now
  // overlap other builds.
  const setDelivery = async (delivery) => {
    await setBuild({ delivery });
    const s = currentSlide();
    const chart = s?.elements.find((e) => e.id === one.id);
    if (!chart) return;
    const from = chart.build.order;
    const to = from + A.seriesSteps(chart);
    if (A.buildEvents(s).some((ev) => ev.el.id !== chart.id && ev.step > from && ev.step <= to)) A.setBuildOrder(s, A.buildEvents(s));
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
      ${byUnit && isBuildIn(one.build.effect) &&
      html`<${Row} label=${t('Delivery')}>
          <${Segmented}
            value=${one.build.delivery || ''}
            options=${[
              { value: '', label: t('All at once') },
              { value: 'series', label: byUnit[0] },
            ]}
            onChange=${setDelivery}
          />
        <//>
        ${series > 0 && html`<${Hint}>${byUnit[1]}</${Hint}>`}`}
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
    ${one &&
    html`<${Section} title=${t('Motion Path')} id="motion-path">
      ${A.hasPath(one)
        ? html`<${Row} label=${t('Step')}><${NumberField} value=${one.build.pathOrder} min=${1} max=${99} onChange=${(v) => setBuild({ pathOrder: v })} /><//>
            <${Row} label=${t('Duration')}><${NumberField} value=${one.build.pathDuration || 1.5} min=${0.1} max=${60} step=${0.1} unit="s" onChange=${(v) => setBuild({ pathDuration: v })} /><//>
            <${Row}><${Toggle} value=${one.build.pathSmooth} label=${t('Smooth curve')} onChange=${(v) => setBuild({ pathSmooth: v })} /><//>
            <div class="row-inline">
              <${Button} size="sm" icon="spline" label=${t('Redraw')} onClick=${() => drawPath(one)} />
              <${Button} size="sm" variant="ghost" icon="trash-2" label=${t('Remove')} onClick=${() => setBuild({ path: [], pathOrder: 0 })} />
            </div>`
        : html`<div class="row-inline"><${Button} size="sm" icon="spline" label=${t('Draw Path')} onClick=${() => drawPath(one)} /></div>`}
      <${Hint}>${t('Click on the slide to add points; press Enter or double-click to finish, Esc to cancel. Drag the points to adjust the path.')}</${Hint}>
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
      <${TransitionPreview} slide=${slide} />
    <//>`}
    <${Section} title=${t('Build Order ({n})', { n: events.length })} id="build-order">
      ${events.length === 0 && html`<${Hint}>${t('No builds on this slide. Select an object and choose a “Build in” effect.')}</${Hint}>`}
      <div class="build-list">
        ${events.map((ev, i) => {
          const e = ev.el;
          const extra = ev.kind === 'in' ? A.seriesSteps(e) : 0;
          const what =
            ev.kind === 'path'
              ? t('Motion path')
              : `${effectLabel(e.build.effect)}${extra ? ` · ${BY_UNIT[buildUnit(e.content)]()}` : ''}${e.build.outEffect ? ` · ${t('out: {effect}', { effect: effectLabel(e.build.outEffect) })}` : ''}`;
          return html`<div key=${e.id + ev.kind} class=${cls('build-item', state.selection.includes(e.id) && 'active')} onClick=${() => set({ selection: [e.id] })}>
            <span class="build-step">${ev.step ? (extra ? `${ev.step}–${ev.step + extra}` : ev.step) : '–'}</span>
            <span class="build-name">${elementName(e)}<small>${what}</small></span>
            <span class="build-move">
              <button title=${t('Earlier')} onClick=${(evt) => { evt.stopPropagation(); move(i, -1); }}><${Icon} name="chevron-up" size=${14} /></button>
              <button title=${t('Later')} onClick=${(evt) => { evt.stopPropagation(); move(i, 1); }}><${Icon} name="chevron-down" size=${14} /></button>
            </span>
          </div>`;
        })}
      </div>
    <//>
  `;
}
