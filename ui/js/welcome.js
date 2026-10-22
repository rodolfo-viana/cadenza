// Start screen: pick a theme and size, open recent files, recover crashes.

import { html, useEffect, useState } from './lib.js';
import { Icon } from './icons.js';
import { state, useStore, newDocument, openDialog, openPath, recover, discardRecovery, refreshRecoverable, settings, openModal, refreshRecent, shownTitle } from './store.js';
import { invoke } from './api.js';
import { Segmented } from './controls.js';
import { cardFromDef } from './themebuilder.js';
import { cls, timeAgo } from './util.js';
import { t, tn } from './i18n.js';

export function Logo({ size = 40 }) {
  return html`<svg class="logo" width=${size} height=${size} viewBox="0 0 64 64" aria-hidden="true">
    <defs>
      <linearGradient id="czg" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="#7c6cff" />
        <stop offset="1" stop-color="#e2489f" />
      </linearGradient>
    </defs>
    <rect x="2" y="2" width="60" height="60" rx="15" fill="url(#czg)" />
    <path d="M42.5 21.5a15 15 0 1 0 0 21" fill="none" stroke="#fff" stroke-width="6.5" stroke-linecap="round" />
    <circle cx="44.5" cy="19.5" r="4.6" fill="#fff" />
  </svg>`;
}

const ASPECTS = [
  { value: '16:9', label: '16:9' },
  { value: '16:10', label: '16:10' },
  { value: '4:3', label: '4:3' },
  { value: '1:1', label: '1:1' },
];

function BigThemeCard({ theme, active, onClick, onDblClick, aspect }) {
  const [w, h] = aspect.split(':').map(Number);
  return html`<button type="button" class=${cls('wtheme', active && 'active')} onClick=${onClick} onDblClick=${onDblClick}>
    <div class="wtheme-preview" style=${{ background: theme.previewBg, aspectRatio: `${w} / ${h}` }}>
      <div class="wt-title" style=${{ color: theme.headingColor, fontFamily: theme.headingFont, textTransform: theme.uppercase ? 'uppercase' : undefined }}>${t('Presentation Title')}</div>
      <div class="wt-sub" style=${{ color: theme.textColor, fontFamily: theme.bodyFont }}>${t('A subtitle goes here')}</div>
      <div class="wt-bar" style=${{ background: theme.accent }} />
    </div>
    <div class="wtheme-name">${theme.name}${theme.dark ? html`<${Icon} name="moon" size=${12} />` : html`<${Icon} name="sun" size=${12} />`}</div>
  </button>`;
}

export function Welcome() {
  const st = useStore();
  const info = st.info;
  const [theme, setTheme] = useState(settings().defaultTheme || info.themes[0].id);
  const [aspect, setAspect] = useState(settings().defaultAspect || '16:9');
  const recent = info.recent || [];
  const create = (id = theme) => newDocument(id, aspect);
  // Another window may have recovered or discarded unsaved work meanwhile.
  // Not on focus: the click that focuses the window would land on whatever
  // moved under the pointer. A card that went stale is refused when used
  // (the window that has it comes forward), and the list refreshes then.
  useEffect(() => {
    refreshRecoverable();
  }, []);
  const groups = [
    ['Cadenza', t('Cadenza themes')],
    ['reveal.js', t('Classic reveal.js themes')],
  ];
  return html`<div class="welcome">
    <aside class="welcome-side">
      <div class="brand"><${Logo} size=${44} /><div><h1>Cadenza</h1><span>${t('Presentations, powered by reveal.js')}</span></div></div>
      <div class="welcome-actions">
        <button type="button" class="btn btn-primary btn-lg" onClick=${() => create()}><${Icon} name="file-plus" size=${18} />${t('New Presentation')}</button>
        <button type="button" class="btn btn-lg" onClick=${openDialog}><${Icon} name="folder-open" size=${18} />${t('Open…')}</button>
      </div>
      ${info.recoverable?.length > 0 &&
      html`<div class="recover">
        <div class="recover-head"><${Icon} name="history" size=${16} />${t('Unsaved work found')}</div>
        ${info.recoverable.map(
          (r) => html`<div class="recover-item">
            <div><b>${shownTitle(r.title) || t('Untitled')}</b><small>${tn('{n} slide', '{n} slides', r.slides)} · ${timeAgo(r.savedAt)}</small></div>
            <div class="row-inline">
              <button class="btn btn-sm btn-primary" onClick=${() => recover(r.sessionId)}>${t('Recover')}</button>
              <button class="btn btn-sm btn-ghost" onClick=${() => discardRecovery(r.sessionId)}>${t('Discard')}</button>
            </div>
          </div>`,
        )}
      </div>`}
      <div class="recent">
        <div class="recent-head">${t('Recent')}</div>
        ${recent.length === 0 && html`<p class="muted small">${t('Presentations you open or save appear here.')}</p>`}
        ${recent.map(
          (r) => html`<div class="recent-item" title=${r.path} onClick=${() => openPath(r.path)}>
            <${Icon} name="presentation" size=${18} />
            <div class="recent-text"><b>${r.name}</b><small>${r.folder}</small></div>
            <small class="muted">${timeAgo(r.modified)}</small>
            <button
              class="icon-btn"
              title=${t('Remove from list')}
              onClick=${async (e) => {
                e.stopPropagation();
                await invoke('recent_remove', { path: r.path });
                refreshRecent();
              }}
            >
              <${Icon} name="x" size=${13} />
            </button>
          </div>`,
        )}
      </div>
      <div class="welcome-foot">
        <button class="btn btn-ghost btn-sm" onClick=${() => openModal('settings')}><${Icon} name="settings" size=${15} />${t('Settings')}</button>
        <button class="btn btn-ghost btn-sm" onClick=${() => openModal('about')}><${Icon} name="info" size=${15} />${t('About')}</button>
      </div>
    </aside>
    <main class="welcome-main">
      <div class="welcome-main-head">
        <div>
          <h2>${t('Choose a theme')}</h2>
          <p class="muted">${t('You can change the theme at any time from the Document inspector.')}</p>
        </div>
        <div class="row-inline">
          <span class="muted small">${t('Slide size')}</span>
          <${Segmented} value=${aspect} options=${ASPECTS} onChange=${setAspect} />
        </div>
      </div>
      <div class="welcome-themes">
        ${(info.userThemes || []).length > 0 &&
        html`<div class="wgroup-label">${t('My themes')}</div>
          <div class="wtheme-grid">
            ${info.userThemes.map((d) => {
              const card = cardFromDef(d);
              return html`<${BigThemeCard} theme=${card} aspect=${aspect} active=${card.id === theme} onClick=${() => setTheme(card.id)} onDblClick=${() => create(card.id)} />`;
            })}
          </div>`}
        ${groups.map(
          ([g, label]) => html`<div class="wgroup-label">${label}</div>
            <div class="wtheme-grid">
              ${info.themes
                .filter((item) => item.group === g)
                .map((item) => html`<${BigThemeCard} theme=${item} aspect=${aspect} active=${item.id === theme} onClick=${() => setTheme(item.id)} onDblClick=${() => create(item.id)} />`)}
            </div>`,
        )}
      </div>
      <div class="welcome-cta">
        <span class="muted">${(info.themes.find((item) => item.id === theme) || (info.userThemes || []).find((item) => item.id === theme))?.name} · ${aspect}</span>
        <button type="button" class="btn btn-primary btn-lg" onClick=${() => create()}>${t('Create Presentation')}<${Icon} name="arrow-right" size=${17} /></button>
      </div>
    </main>
  </div>`;
}
