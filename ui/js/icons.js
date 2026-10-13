import { html } from './lib.js';
import { ICONS } from './icon-data.js';

/** A Lucide stroke icon. */
export function Icon({ name, size = 18, stroke = 1.75, class: extra = '' }) {
  return html`<svg
    class=${'icon i-' + name + ' ' + extra}
    width=${size}
    height=${size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width=${stroke}
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    dangerouslySetInnerHTML=${{ __html: ICONS[name] || '' }}
  />`;
}
