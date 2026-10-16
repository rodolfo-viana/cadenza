// Chart from Data: paste or open data in CSV form, see every chart it can
// make, and insert the one picked (or give it to the chart being edited).

import { html, useEffect, useMemo, useRef, useState } from './lib.js';
import { state, closeModal, apply, deck, currentSlide, toast } from './store.js';
import { Modal, Button } from './ui.js';
import { Select, Toggle } from './controls.js';
import { Icon } from './icons.js';
import { invoke, dialog } from './api.js';
import { t, tn, language } from './i18n.js';
import { readTable, guessRoles, chartChoices, toCsv, ROLES } from './chartdata.js';
import { CHART_ICONS, ERROR_MODES, hasErrorBars } from './charts.js';
import { insert } from './actions.js';
import { cls } from './util.js';

const EXAMPLE = 'Quarter,2025,2026\nQ1,12,16\nQ2,19,22\nQ3,15,21\nQ4,24,31';

/** A role picked for one column, or none picked yet: the guess. */
const columnsKey = (table) => (table ? table.columns.map((c) => c.name).join('\u0001') : '');

export function ChartData({ target = null }) {
  const el = target ? currentSlide()?.elements.find((e) => e.id === target && e.content.kind === 'chart') : null;
  const [text, setText] = useState(() => (el ? toCsv(el.content) : ''));
  // 'auto' until the first-row switch is used.
  const [header, setHeader] = useState(el ? true : 'auto');
  const [transpose, setTranspose] = useState(false);
  const [picked, setPicked] = useState({ key: '', roles: [] });
  const [mapOk, setMapOk] = useState(false);
  const [previews, setPreviews] = useState({ key: '', svgs: [] });
  const area = useRef();
  useEffect(() => {
    // The caret at the start: the beginning of the data in view.
    area.current?.focus();
    area.current?.setSelectionRange(0, 0);
  }, []);

  const table = useMemo(() => (text.trim() ? readTable(text, { header, transpose }) : null), [text, header, transpose]);
  const key = columnsKey(table);
  const roles = useMemo(() => (!table ? [] : picked.key === key ? picked.roles : guessRoles(table)), [table, key, picked]);
  const setRole = (j, role) => {
    // One column of labels and one of groups at most.
    const next = roles.map((r, k) => (k === j ? role : (role === 'label' || role === 'group') && r === role ? 'ignore' : r));
    setPicked({ key, roles: next });
  };

  // Maps: whether the labels name countries is known in Rust.
  const labels = useMemo(() => {
    const j = roles.indexOf('label');
    return j >= 0 && table && !table.columns[j].numeric ? table.columns[j].cells : [];
  }, [table, roles]);
  useEffect(() => {
    let live = true;
    setMapOk(false);
    if (labels.length < 2) return;
    invoke('map_regions', { map: 'world', language: language(), labels })
      .then((r) => live && setMapOk(r.unmatched.length <= labels.length * 0.25))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [labels.join('\n')]);

  // The size and text of the chart made: the edited chart's, or a new one's.
  const d = deck();
  const scale = Math.min(d.width / 1280, d.height / 720);
  const pad = el ? Math.max(0, el.style?.padding || 0) * 2 : 0;
  const size = el ? { w: Math.max(10, el.w - pad), h: Math.max(10, el.h - pad) } : { w: Math.round(d.width * 0.6), h: Math.round(d.height * 0.55) };
  const look = el ? el.content : { fontSize: Math.round(20 * scale), showLegend: true, showValues: false, showGrid: true };

  const choices = useMemo(() => {
    if (!table) return [];
    const kinds = state.info.charts.map((c) => c.id);
    const list = chartChoices(table, roles, kinds);
    const one = list.find((c) => c.kind === 'column');
    if (mapOk && one) {
      const first = one.content.series[0];
      const map = el?.content.chart === 'map' && el.content.map ? el.content.map : 'world';
      // Maps come last, as in the list of chart types.
      list.push({ kind: 'map', content: { ...one.content, chart: 'map', map, series: [first] } });
    }
    return list.map((c) => ({ ...c, content: dressed(c.content, el, look) }));
  }, [table, roles, mapOk]);

  // Previews, drawn in Rust as the chart will be (a moment after typing stops).
  const previewKey = JSON.stringify(choices.map((c) => c.content));
  useEffect(() => {
    if (!choices.length) return setPreviews({ key: previewKey, svgs: [] });
    let live = true;
    const timer = setTimeout(() => {
      invoke('chart_previews', { charts: choices.map((c) => c.content), width: size.w, height: size.h })
        .then((svgs) => live && setPreviews({ key: previewKey, svgs }))
        .catch((e) => toast(String(e), 'error'));
    }, 200);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [previewKey]);

  const openFile = async () => {
    const path = await dialog.open({ title: t('Open Data'), multiple: false, filters: [{ name: t('Data (CSV)'), extensions: ['csv', 'tsv', 'tab', 'txt', 'dat'] }] });
    if (!path) return;
    try {
      setText(await invoke('read_data_file', { path }));
      setHeader('auto');
      setTranspose(false);
    } catch (e) {
      toast(t(String(e)), 'error');
    }
  };

  const choose = async (choice) => {
    closeModal();
    if (el) {
      await apply({ op: 'updateElements', slide: state.slideId, updates: [{ id: el.id, patch: { content: choice.content } }] });
    } else {
      await insert('chart', choice.kind, { content: choice.content });
    }
  };

  const series = choices.find((c) => c.kind === 'column')?.content.series || [];
  const errors = series.length ? choices[0].content.errorBars : '';
  const summary = !table
    ? ''
    : series.length
      ? [
          tn('{n} row', '{n} rows', table.rows),
          tn('{n} series', '{n} series', series.length),
          errors && t('error bars: {mode}', { mode: t(ERROR_MODES.find(([m]) => m === errors)?.[1] || '') }),
        ]
          .filter(Boolean)
          .join(' · ')
      : t('No column of numbers to chart. Mark one as Values.');
  const svgs = previews.key === previewKey ? previews.svgs : [];

  return html`<${Modal} title=${el ? t('Chart Data') : t('Chart from Data')} width=${1040} class="dc-modal" onClose=${closeModal}>
    <div class="dc">
      <div class="dc-input">
        <div class="dc-bar">
          <span class="ihint">${t('Paste cells from a spreadsheet, or data in CSV form.')}</span>
          <${Button} size="sm" icon="folder-open" label=${t('Open File…')} onClick=${openFile} />
        </div>
        <textarea
          ref=${area}
          class="textfield mono dc-text"
          spellcheck="false"
          placeholder=${EXAMPLE}
          value=${text}
          onInput=${(e) => setText(e.target.value)}
        />
        <div class="dc-switches">
          <${Toggle} value=${table ? table.header : header === true} label=${t('First row names the columns')} onChange=${(v) => setHeader(v)} />
          <${Toggle} value=${transpose} label=${t('Swap rows and columns')} onChange=${setTranspose} />
        </div>
        ${table &&
        html`<div class="dc-cols">
          ${table.columns.map(
            (c, j) => html`<div class=${cls('dc-col', roles[j] === 'ignore' && 'off')}>
              <span class="dc-col-name" title=${c.name}>${c.name}</span>
              <${Select}
                value=${roles[j]}
                options=${ROLES.filter(([r]) => c.numeric || ['label', 'group', 'ignore'].includes(r)).map(([value, label]) => ({ value, label: t(label) }))}
                onChange=${(v) => setRole(j, v)}
              />
            </div>`,
          )}
        </div>`}
        ${summary && html`<p class="ihint">${summary}</p>`}
        ${errors && !choices.some((c) => hasErrorBars(c.kind)) && html`<p class="ihint">${t('Error bars show on column, bar, line and dot charts.')}</p>`}
      </div>
      <div class="dc-gallery">
        ${!choices.length &&
        html`<div class="dc-empty">
          <${Icon} name="sheet" size=${30} />
          <p>${table ? t('No chart fits this data yet.') : t('The charts you can make from your data appear here.')}</p>
        </div>`}
        ${choices.length > 0 &&
        html`<div class="dc-grid">
          ${choices.map(
            (c, k) => html`<button type="button" class="dc-card" title=${el ? t('Use this chart') : t('Insert this chart')} onClick=${() => choose(c)}>
              <div class="dc-preview" style=${{ aspectRatio: `${size.w} / ${size.h}` }} dangerouslySetInnerHTML=${{ __html: svgs[k] || '' }} />
              <span class="dc-name"><${Icon} name=${CHART_ICONS[c.kind]} size=${14} />${t(state.info.charts.find((x) => x.id === c.kind)?.name || c.kind)}</span>
            </button>`,
          )}
        </div>`}
      </div>
    </div>
  <//>`;
}

/**
 * The content a choice gives a chart: its data and type, with the look of
 * the chart being edited (colors and point shapes of its series too), or a
 * new chart's.
 */
function dressed(content, el, look) {
  const old = el?.content;
  const series = content.series.map((s, k) => ({ ...s, color: old?.series[k]?.color || '', marker: old?.series[k]?.marker || '' }));
  const ends = old && !fitsEnds(content, old.axisMin ?? null, old.axisMax ?? null) ? { axisMin: null, axisMax: null } : {};
  return {
    ...(old || look),
    ...content,
    ...ends,
    series,
    map: content.chart === 'map' ? content.map : old?.map || '',
  };
}

/**
 * Whether the ends given to a chart's value axis still suit new data: it
 * lies between them and fills a good part of the room (new data in the
 * place of other data would otherwise get squeezed or cut off).
 */
function fitsEnds(content, min, max) {
  if (min === null && max === null) return true;
  const mode = content.errorBars;
  const values = content.series.flatMap((s) =>
    s.values.flatMap((v, i) => {
      const [lo, hi] = [s.low?.[i] ?? null, s.high?.[i] ?? null];
      if (mode === 'range') return [v, lo ?? v, hi ?? v];
      if (mode === 'pm') return [v - Math.abs(hi ?? lo ?? 0), v + Math.abs(hi ?? lo ?? 0)];
      if (mode === 'amounts') return [v - Math.abs(lo ?? 0), v + Math.abs(hi ?? 0)];
      return [v];
    }),
  );
  if (!values.length) return true;
  const [lo, hi] = [Math.min(...values), Math.max(...values)];
  const [a, b] = [min ?? Math.min(lo, 0), max ?? Math.max(hi, 0)];
  return lo >= a && hi <= b && hi - lo >= (b - a) * 0.25;
}
