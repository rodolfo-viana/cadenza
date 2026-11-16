// Chart from Data: paste or open data in CSV form, see every chart it can
// make, and insert the one picked (or give it to the chart being edited).

import { html, useEffect, useMemo, useRef, useState } from './lib.js';
import { state, closeModal, apply, deck, currentSlide, toast } from './store.js';
import { Modal, Button } from './ui.js';
import { Select, Toggle } from './controls.js';
import { Icon } from './icons.js';
import { invoke, dialog } from './api.js';
import { t, tn, language } from './i18n.js';
import { readTable, guessRoles, chartChoices, repeatedRows, ROLES } from './chartdata.js';
import { currentDataInput, dataSource, sameInput, tableFromData } from './datasource.js';
import { CHART_ICONS, ERROR_MODES, hasErrorBars } from './charts.js';
import { insert } from './actions.js';
import { cls, fileName } from './util.js';

const EXAMPLE = 'Quarter,2025,2026\nQ1,12,16\nQ2,19,22\nQ3,15,21\nQ4,24,31';

/** A role picked for one column, or none picked yet: the guess. */
const columnsKey = (table) => (table ? String(table.columns.length) : '');

export function ChartData({ target = null, kind = 'chart', source = false }) {
  // Capture the target once. Async file reads and queued edits must not follow
  // selection into another slide or document.
  const [context] = useState(() => ({ docId: state.doc?.docId, slideId: state.slideId }));
  const [modal] = useState(() => state.modal);
  const [el] = useState(() => target ? currentSlide()?.elements.find((e) => e.id === target && ['chart', 'table'].includes(e.content.kind)) : null);
  const isTable = (el?.content.kind || kind) === 'table';
  const [initial] = useState(() => el ? source && el.dataSource ? el.dataSource : currentDataInput(el.content) : null);
  const [text, setText] = useState(() => initial?.text || '');
  // 'auto' until the first-row switch is used.
  const [header, setHeader] = useState(initial?.header ?? 'auto');
  const [transpose, setTranspose] = useState(initial?.transpose || false);
  const [dialect, setDialect] = useState(initial?.dialect || 'csv');
  const [delimiter, setDelimiter] = useState(initial?.delimiter);
  const [name, setName] = useState(source ? initial?.name || '' : '');
  const [picked, setPicked] = useState(() => initial ? { key: String(initial.roles.length), roles: initial.roles } : { key: '', roles: [] });
  const [mapOk, setMapOk] = useState(false);
  const [previews, setPreviews] = useState({ key: '', svgs: [] });
  const area = useRef();
  useEffect(() => {
    // The caret at the start: the beginning of the data in view.
    area.current?.focus();
    area.current?.setSelectionRange(0, 0);
  }, []);

  const table = useMemo(() => (text.trim() ? readTable(text, { header, transpose, delimiter, dialect }) : null), [text, header, transpose, delimiter, dialect]);
  const key = columnsKey(table);
  const roles = useMemo(() => (!table || isTable ? [] : picked.key === key ? picked.roles : guessRoles(table)), [table, key, picked, isTable]);
  const unchanged = sameInput(initial, text, table, transpose, dialect, roles);
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
  const chartEl = isTable ? null : el;
  const look = chartEl ? chartEl.content : { fontSize: Math.round(20 * scale), showLegend: true, showValues: false, showGrid: true };

  const choices = useMemo(() => {
    if (!table || isTable) return [];
    const kinds = state.info.charts.map((c) => c.id);
    const list = chartChoices(table, roles, kinds, { current: !source ? chartEl?.content : null });
    const one = list.find((c) => c.kind === 'column');
    if (mapOk && one) {
      const first = one.content.series[0];
      const map = el?.content.chart === 'map' && el.content.map ? el.content.map : 'world';
      // Maps come last, as in the list of chart types.
      list.push({ kind: 'map', content: { ...one.content, chart: 'map', map, series: [first] } });
    }
    // A current model may contain meaningful missing rows or values, inactive
    // error arrays and settings that an importer cannot reconstruct. Unchanged
    // input keeps that exact model and must still offer its existing chart kind.
    if (el && !source && unchanged && !list.some((c) => c.kind === el.content.chart)) {
      list.unshift({ kind: el.content.chart, content: el.content });
    }
    return list.map((c) => ({ ...c, content: el && !source && unchanged && c.kind === el.content.chart
      ? el.content : dressed(c.content, chartEl, look) }));
  }, [table, roles, mapOk, unchanged]);

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
      setName(fileName(path));
      setHeader('auto');
      setTranspose(false);
      setDelimiter(undefined);
      setDialect('csv');
      setPicked({ key: '', roles: [] });
    } catch (e) {
      toast(t(String(e)), 'error');
    }
  };

  const choose = async (choice) => {
    if (state.doc?.docId !== context.docId || state.modal !== modal || !table || (target && !el)) return;
    // Inspecting current data and applying it unchanged is a true no-op.
    if (el && !source && unchanged && (isTable || choice.kind === el.content.chart)) {
      closeModal();
      return;
    }
    const content = choice.kind === 'table' ? tableFromData(table, isTable ? el?.content : {}) : choice.content;
    const origin = dataSource(text, { name, dialect, transpose, roles: choice.kind === 'table' ? [] : roles, table });
    let out;
    if (el) {
      let stale = false;
      out = await apply(() => {
        const current = deck().slides.find((s) => s.id === context.slideId)?.elements.find((e) => e.id === el.id);
        // Another edit or an Undo while the modal was open must not be replaced.
        if (!current || JSON.stringify(current.content) !== JSON.stringify(el.content)
          || JSON.stringify(current.dataSource) !== JSON.stringify(el.dataSource)) {
          stale = true;
          return null;
        }
        const patch = { content, ...((source || !el.dataSource) && { dataSource: origin }) };
        return { op: 'updateElements', slide: context.slideId, updates: [{ id: el.id, patch }] };
      }, { docId: context.docId });
      if (stale) {
        toast(t('The object changed while its data was open. Your CSV draft is kept here; reopen the editor to apply it to the current object.'), 'error');
        return;
      }
    } else {
      out = await insert(choice.kind === 'table' ? 'table' : 'chart', choice.kind === 'table' ? '' : choice.kind, { content, dataSource: origin }, context);
    }
    if (out && state.doc?.docId === context.docId && state.modal === modal) closeModal();
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
  const repeated = series.length ? repeatedRows(table, roles) : 0;
  const svgs = previews.key === previewKey ? previews.svgs : [];

  return html`<${Modal} title=${source ? t('Edit original CSV') : isTable ? t('Table from Data') : el ? t('Chart Data') : t('Chart from Data')} width=${1040} class="dc-modal" onClose=${closeModal}>
    ${source && html`<p class="dc-source-hint">${t('Applying replaces the stored original CSV and the current data together.')}</p>`}
    <div class="dc">
      <div class="dc-input">
        <div class="dc-bar">
          <span class="ihint">${t('Paste cells from a spreadsheet, or data in CSV form.')}</span>
          <${Button} size="sm" icon="folder-open" label=${t('Open File…')} onClick=${openFile} />
        </div>
        <textarea
          ref=${area}
          class="textfield mono dc-text"
          aria-label=${t('CSV data')}
          spellcheck="false"
          placeholder=${EXAMPLE}
          value=${text}
          onInput=${(e) => setText(e.target.value)}
        />
        <div class="dc-switches">
          <label>${t('Separator')} <${Select} value=${delimiter || 'auto'} options=${[
            { value: 'auto', label: t('Automatic') }, { value: ',', label: t('Comma') },
            { value: ';', label: t('Semicolon') }, { value: '\t', label: t('Tab') },
          ]} onChange=${(v) => { setDelimiter(v === 'auto' ? undefined : v); setPicked({ key: '', roles: [] }); }} /></label>
          <${Toggle} value=${table ? table.header : header === true} label=${t('First row names the columns')} onChange=${(v) => { setHeader(v); setPicked({ key: '', roles: [] }); }} />
          <${Toggle} value=${transpose} label=${t('Swap rows and columns')} onChange=${(v) => { setTranspose(v); setPicked({ key: '', roles: [] }); }} />
          <${Toggle} value=${dialect === 'pgfplots'} label=${t('Interpret LaTeX braces (pgfplots)')} onChange=${(v) => setDialect(v ? 'pgfplots' : 'csv')} />
        </div>
        ${table && !isTable &&
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
        ${summary && !isTable && html`<p class="ihint">${summary}</p>`}
        ${repeated > 0 &&
        html`<p class="ihint">
          ${tn(
            '{n} row repeats the group and label of an earlier row. Only the first value is charted.',
            '{n} rows repeat the group and label of an earlier row. Only the first value is charted.',
            repeated,
          )}
        </p>`}
        ${errors && !choices.some((c) => hasErrorBars(c.kind)) && html`<p class="ihint">${t('Error bars show on column, bar, line and dot charts.')}</p>`}
      </div>
      <div class="dc-gallery">
        ${!table && isTable && html`<div class="dc-empty"><${Icon} name="table" size=${30} /><p>${t('The table preview appears here when you paste or open data.')}</p></div>`}
        ${table?.grid?.length > 0 && (!el || isTable) && html`<div class="dc-table-summary">
          <${Button} icon="table" label=${el ? t('Apply data to table') : t('Insert as table')} onClick=${() => choose({ kind: 'table' })} />
          ${isTable && html`<div class="dc-table-preview"><table><tbody>${table.grid.slice(0, 20).map((row, i) => html`<tr>${row.map((cell) =>
            table.header && i === 0 ? html`<th>${cell}</th>` : html`<td>${cell}</td>`)}</tr>`)}</tbody></table></div>`}
          ${isTable && table.grid.length > 20 && html`<p class="ihint">${t('Preview shows the first 20 rows. All rows will be inserted.')}</p>`}
        </div>`}
        ${!choices.length && !isTable &&
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
    ...(old && content.chart === old.chart && { stacked: old.stacked }),
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
      if (v === null || v === undefined) return [];
      const [lo, hi] = [s.low?.[i] ?? null, s.high?.[i] ?? null];
      if (mode === 'range') return [v, lo ?? v, hi ?? v];
      if (mode === 'pm') return [v - Math.abs(hi ?? lo ?? 0), v + Math.abs(hi ?? lo ?? 0)];
      if (mode === 'amounts') return [v - Math.abs(lo ?? 0), v + Math.abs(hi ?? 0)];
      return [v];
    }),
  );
  if (!values.length) return true;
  const [lo, hi] = values.reduce(([a, b], v) => [Math.min(a, v), Math.max(b, v)], [Infinity, -Infinity]);
  const [a, b] = [min ?? Math.min(lo, 0), max ?? Math.max(hi, 0)];
  return lo >= a && hi <= b && hi - lo >= (b - a) * 0.25;
}
