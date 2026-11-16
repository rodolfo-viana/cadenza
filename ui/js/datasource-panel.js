import { html } from './lib.js';
import { openModal } from './store.js';
import { Section } from './controls.js';
import { Button } from './ui.js';
import { t } from './i18n.js';

/** Shown upon ordinary chart/table selection; no modal is needed to inspect CSV. */
export function DataSourcePanel({ el }) {
  const source = el.dataSource;
  return html`<${Section} title=${t('Data source')}>
    ${source ? html`<div class="data-source">
      <label class="data-source-label" for=${`data-source-${el.id}`}>${t('Original CSV')}</label>
      ${source.name && html`<span class="data-source-name" title=${source.name}>${source.name}</span>`}
      <textarea id=${`data-source-${el.id}`} class="textfield mono data-source-text" value=${source.text} readOnly spellcheck="false" />
      <p class="ihint">${source.edited
        ? t('Current data has been edited. The original CSV is preserved.')
        : t('The original CSV is stored in this presentation.')}</p>
      <${Button} size="sm" label=${t('Edit original CSV…')} onClick=${() => openModal('chartData', { target: el.id, source: true })} />
    </div>` : html`<p class="ihint">${t('The original CSV is unavailable for this object.')}</p>`}
    <${Button} size="sm" label=${t('Edit current data as CSV…')} onClick=${() => openModal('chartData', { target: el.id })} />
  <//>`;
}
