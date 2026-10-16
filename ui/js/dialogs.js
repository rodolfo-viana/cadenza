// Modal dialogs and the command palette.

import { html, useEffect, useMemo, useRef, useState } from './lib.js';
import { Icon } from './icons.js';
import { state, set, useStore, closeModal, settings, updateSettings, goToSlide, updateDeck } from './store.js';
import { Modal, Button } from './ui.js';
import { Row, Select, Segmented, Toggle, NumberField } from './controls.js';
import { COMMANDS, enabled } from './commands.js';
import { slideTitle } from './panels.js';
import { Logo } from './welcome.js';
import { keyLabel, cls } from './util.js';
import { opener, invoke } from './api.js';
import { MediaLibrary, KeyPrompt } from './media.js';
import { ThemeBuilder } from './themebuilder.js';
import { ChartData } from './datachart.js';
import { t, N_, LANGUAGES } from './i18n.js';

export function ModalHost() {
  const st = useStore();
  const m = st.modal;
  if (!m) return null;
  const C = { unsaved: Unsaved, prompt: Prompt, choice: Choice, settings: Settings, shortcuts: Shortcuts, about: About, palette: Palette, media: MediaLibrary, themeBuilder: ThemeBuilder, chartData: ChartData }[m.type];
  return C ? html`<${C} ...${m.props} />` : null;
}

function Unsaved({ name, resolve }) {
  return html`<${Modal}
    width=${440}
    onClose=${() => resolve('cancel')}
    footer=${html`
      <${Button} label=${t('Don’t Save')} variant="ghost" onClick=${() => resolve('discard')} />
      <span class="spacer" />
      <${Button} label=${t('Cancel')} onClick=${() => resolve('cancel')} />
      <${Button} label=${t('Save')} variant="primary" onClick=${() => resolve('save')} />
    `}
  >
    <div class="alert">
      <${Logo} size=${44} />
      <div>
        <h3>${t('Save changes to “{name}”?', { name })}</h3>
        <p class="muted">${t('Your changes will be lost if you don’t save them.')}</p>
      </div>
    </div>
  <//>`;
}

function Prompt({ title, label, placeholder, value = '', confirm = t('OK'), resolve }) {
  const [v, setV] = useState(value);
  const ref = useRef();
  useEffect(() => ref.current?.focus(), []);
  return html`<${Modal}
    title=${title}
    width=${460}
    onClose=${() => resolve(null)}
    footer=${html`<span class="spacer" /><${Button} label=${t('Cancel')} onClick=${() => resolve(null)} /><${Button} label=${confirm} variant="primary" disabled=${!v.trim()} onClick=${() => resolve(v)} />`}
  >
    <label class="field-label">${label}</label>
    <input ref=${ref} class="textfield" value=${v} placeholder=${placeholder} onInput=${(e) => setV(e.target.value)} onKeyDown=${(e) => e.key === 'Enter' && v.trim() && resolve(v)} />
  <//>`;
}

function Choice({ title, message, choices, resolve }) {
  return html`<${Modal}
    title=${title}
    width=${460}
    onClose=${() => resolve(null)}
    footer=${html`<span class="spacer" /><${Button} label=${t('Cancel')} onClick=${() => resolve(null)} />${choices.map(
      (c) => html`<${Button} label=${c.label} variant=${c.primary ? 'primary' : ''} onClick=${() => resolve(c.value)} />`,
    )}`}
  >
    <p>${message}</p>
  <//>`;
}

function Settings() {
  const s = settings();
  const themes = state.info.themes;
  return html`<${Modal} title=${t('Settings')} width=${520} onClose=${closeModal}>
    <div class="settings">
      <h4>${t('Appearance')}</h4>
      <${Row} label=${t('Language')}>
        <${Select} value=${s.language || 'en'} options=${LANGUAGES.map((l) => ({ value: l.code, label: l.name }))} onChange=${(v) => updateSettings({ language: v })} />
      <//>
      <${Row} label=${t('App theme')}>
        <${Segmented}
          value=${s.uiTheme}
          options=${[
            { value: 'system', label: t('System') },
            { value: 'dark', label: t('Dark') },
            { value: 'light', label: t('Light') },
          ]}
          onChange=${(v) => updateSettings({ uiTheme: v })}
        />
      <//>
      <h4>${t('New presentations')}</h4>
      <${Row} label=${t('Theme')}>
        <${Select} value=${s.defaultTheme} options=${themes.map((theme) => ({ value: theme.id, label: `${theme.name} (${theme.group})` }))} onChange=${(v) => updateSettings({ defaultTheme: v })} />
      <//>
      <${Row} label=${t('Slide size')}>
        <${Segmented}
          value=${s.defaultAspect}
          options=${['16:9', '16:10', '4:3', '1:1'].map((v) => ({ value: v, label: v }))}
          onChange=${(v) => updateSettings({ defaultAspect: v })}
        />
      <//>
      <h4>${t('Editing')}</h4>
      <${Row}><${Toggle} value=${s.snap} label=${t('Snap to smart guides (hold Ctrl to disable)')} onChange=${(v) => updateSettings({ snap: v })} /><//>
      <${Row}><${Toggle} value=${s.showGrid} label=${t('Show grid and snap to it')} onChange=${(v) => updateSettings({ showGrid: v })} /><//>
      ${s.showGrid && html`<${Row} label=${t('Grid size')}><${NumberField} value=${s.gridSize} min=${4} max=${400} unit="px" onChange=${(v) => updateSettings({ gridSize: v })} /><//>`}
      <h4>${t('Integrations')}</h4>
      <p class="ihint">${t('Optional online services for the Media Library. Keys are stored in your system keychain and never saved in presentations.')}</p>
      <${Integrations} />
      <h4>${t('Presenting')}</h4>
      <${Row}>
        <${Toggle}
          value=${s.presenterView}
          label=${t('Open the presenter display (notes, timer, next slide)')}
          onChange=${(v) => updateSettings(v ? { presenterView: true } : { presenterView: false, teleprompter: { enabled: false } })}
        />
      <//>
      <${Row}>
        <${Toggle}
          value=${!!s.teleprompter?.enabled}
          label=${t('Show the teleprompter in the presenter display (scrolls your speaker notes)')}
          onChange=${(v) => updateSettings(v ? { presenterView: true, teleprompter: { enabled: true } } : { teleprompter: { enabled: false } })}
        />
      <//>
      <p class="ihint">${t('With two screens, the audience sees the presentation on the other screen while the presenter display stays on yours.')}</p>
      <p class="ihint">${t('With one screen, press X during the show to switch between the slides and the presenter display. In a video call, share only the presentation window: the audience never sees the presenter display or the teleprompter.')}</p>
    </div>
  <//>`;
}

const isHidden = (c) => (typeof c.hidden === 'function' ? c.hidden() : c.hidden);

function Shortcuts() {
  const groups = {};
  for (const c of COMMANDS) {
    if (!c.keys?.length || isHidden(c)) continue;
    (groups[c.group] ||= []).push(c);
  }
  const extra = [
    ['Canvas', t('Double-click'), t('Edit text / replace image')],
    ['Canvas', t('Arrow keys'), t('Nudge 1 px (Shift: 10 px)')],
    ['Canvas', t('Shift + drag'), t('Constrain movement / keep proportions')],
    ['Canvas', t('Ctrl + drag'), t('Move without snapping')],
    ['Canvas', t('Ctrl + scroll'), t('Zoom')],
    ['Canvas', t('Drag from a ruler'), t('Create a guide (drag it back to remove)')],
    ['Slides', 'Tab / Shift+Tab', t('Nest / un-nest slide (vertical stack)')],
    ['Presenting', t('B or .'), t('Black screen')],
    ['Presenting', 'O', t('Slide overview')],
    ['Presenting', 'Esc', t('End the show')],
  ];
  return html`<${Modal} title=${t('Keyboard Shortcuts')} width=${720} onClose=${closeModal}>
    <div class="shortcuts">
      ${Object.entries(groups).map(
        ([g, cmds]) => html`<div class="sc-group">
          <h4>${t(g)}</h4>
          ${cmds.map((c) => html`<div class="sc-row"><span>${t(c.label)}</span><kbd>${keyLabel(c.keys[0])}</kbd></div>`)}
        </div>`,
      )}
      <div class="sc-group">
        <h4>${t('Mouse & presenting')}</h4>
        ${extra.map(([, k, d]) => html`<div class="sc-row"><span>${d}</span><kbd>${k}</kbd></div>`)}
      </div>
    </div>
  <//>`;
}

/** Renders a translated sentence, replacing its `{key}` placeholders with elements. */
function withLinks(text, nodes) {
  return text.split(/\{(\w+)\}/).map((part, i) => (i % 2 ? (nodes[part] ?? `{${part}}`) : part));
}

function About() {
  const link = (url, label) => html`<a href="#" onClick=${(e) => { e.preventDefault(); opener.openUrl(url); }}>${label}</a>`;
  return html`<${Modal} width=${460} onClose=${closeModal}>
    <div class="about">
      <${Logo} size=${72} />
      <h2>Cadenza</h2>
      <p class="muted">${t('Version {version} · {platform}', { version: state.info.version, platform: state.info.platform })}</p>
      <p>${withLinks(t('A presentation studio built with Rust and the {engine} engine.'), { engine: link('https://revealjs.com', 'reveal.js') })}</p>
      <p>${t('Made by {author}', { author: 'Rodolfo Viana' })}</p>
      <p>${link('https://github.com/rodolfo-viana/cadenza', 'github.com/rodolfo-viana/cadenza')}</p>
      <p class="small muted">
        ${withLinks(t('Made with {tauri}, {reveal} (MIT), {katex} (MIT), {highlight} (BSD), {preact} (MIT) and {lucide} icons (ISC).'), {
          tauri: link('https://tauri.app', 'Tauri'),
          reveal: link('https://revealjs.com', 'reveal.js'),
          katex: link('https://katex.org', 'KaTeX'),
          highlight: link('https://highlightjs.org', 'highlight.js'),
          preact: link('https://preactjs.com', 'Preact'),
          lucide: link('https://lucide.dev', 'Lucide'),
        })}
        ${' '}${withLinks(t('Maps from {naturalEarth} (public domain).'), { naturalEarth: link('https://www.naturalearthdata.com', 'Natural Earth') })}
      </p>
      <${Button} label=${t('Close')} onClick=${closeModal} />
    </div>
  <//>`;
}

// ---------------------------------------------------------------------------
// Command palette
// ---------------------------------------------------------------------------

function score(text, q) {
  const txt = text.toLowerCase();
  if (!q) return 1;
  const i = txt.indexOf(q);
  if (i >= 0) return 100 - i;
  let p = 0;
  let s = 0;
  for (const ch of q) {
    const j = txt.indexOf(ch, p);
    if (j < 0) return 0;
    s += j === p ? 3 : 1;
    p = j + 1;
  }
  return s;
}

function Palette() {
  const [q, setQ] = useState('');
  const [active, setActive] = useState(0);
  const ref = useRef();
  const listRef = useRef();
  useEffect(() => ref.current?.focus(), []);
  const items = useMemo(() => {
    // `en` keeps the English wording searchable when the interface is translated.
    const out = COMMANDS.filter((c) => !isHidden(c) && enabled(c)).map((c) => ({
      label: t(c.label),
      group: t(c.group),
      en: `${c.group} ${c.label}`,
      icon: c.icon,
      keys: c.keys?.[0],
      run: c.run,
    }));
    if (state.doc) {
      state.doc.deck.slides.forEach((s, i) =>
        out.push({ label: t('Go to slide {n}: {title}', { n: i + 1, title: slideTitle(s) }), group: t('Slides'), icon: 'presentation', run: () => goToSlide(s.id) }),
      );
      for (const theme of state.info.themes) {
        out.push({ label: t('Theme: {name}', { name: theme.name }), group: t('Theme'), icon: 'palette', run: () => updateDeck({ theme: theme.id }) });
      }
    }
    const s = q.trim().toLowerCase();
    return out
      .map((it) => ({ it, s: Math.max(score(`${it.group} ${it.label}`, s), it.en ? score(it.en, s) : 0) }))
      .filter((x) => x.s > 0)
      .sort((a, b) => b.s - a.s)
      .slice(0, 60)
      .map((x) => x.it);
  }, [q]);
  useEffect(() => setActive(0), [q]);
  useEffect(() => listRef.current?.children[active]?.scrollIntoView({ block: 'nearest' }), [active]);
  const runItem = (it) => {
    closeModal();
    setTimeout(() => it.run(), 0);
  };
  return html`<div class="modal-backdrop palette-backdrop" onMouseDown=${(e) => e.target === e.currentTarget && closeModal()}>
    <div class="palette">
      <div class="palette-input">
        <${Icon} name="search" size=${17} />
        <input
          ref=${ref}
          placeholder=${t('Type a command, slide or theme…')}
          value=${q}
          onInput=${(e) => setQ(e.target.value)}
          onKeyDown=${(e) => {
            if (e.key === 'ArrowDown') {
              e.preventDefault();
              setActive(Math.min(items.length - 1, active + 1));
            } else if (e.key === 'ArrowUp') {
              e.preventDefault();
              setActive(Math.max(0, active - 1));
            } else if (e.key === 'Enter' && items[active]) {
              e.preventDefault();
              runItem(items[active]);
            } else if (e.key === 'Escape') {
              e.preventDefault();
              e.stopPropagation();
              closeModal();
            }
          }}
        />
      </div>
      <div class="palette-list" ref=${listRef}>
        ${items.map(
          (it, i) => html`<div class=${cls('palette-item', i === active && 'active')} onMouseEnter=${() => setActive(i)} onClick=${() => runItem(it)}>
            <${Icon} name=${it.icon || 'chevron-right'} size=${16} />
            <span class="pi-label">${it.label}</span>
            <span class="pi-group">${it.group}</span>
            ${it.keys && html`<kbd>${keyLabel(it.keys)}</kbd>`}
          </div>`,
        )}
        ${!items.length && html`<div class="palette-empty">${t('No matches')}</div>`}
      </div>
    </div>
  </div>`;
}

export { set };

const SERVICES = [
  ['pexels', 'Pexels', N_('Free high-quality photos')],
  ['pixabay', 'Pixabay', N_('Free photos and illustrations')],
  ['giphy', 'GIPHY', N_('Animated GIFs')],
  ['youtube', 'YouTube', N_('Video search (pasting links works without a key)')],
];

function Integrations() {
  const [status, setStatus] = useState(null);
  const [editing, setEditing] = useState(null);
  const load = () => invoke('secret_status').then(setStatus).catch(() => setStatus({}));
  useEffect(() => {
    load();
  }, []);
  if (!status) return null;
  return html`<div class="integrations">
    ${SERVICES.map(
      ([id, name, desc]) => html`<div class="integration">
        <div class="int-head">
          <div><b>${name}</b><span class="muted small">${t(desc)}</span></div>
          ${status[id]
            ? html`<span class="int-ok"><${Icon} name="check" size=${13} /> ${t('Connected')}</span>
                <${Button} size="sm" variant="ghost" label=${t('Remove')} onClick=${async () => { await invoke('secret_delete', { name: id }); load(); }} />`
            : html`<${Button} size="sm" label=${editing === id ? t('Cancel') : t('Connect…')} onClick=${() => setEditing(editing === id ? null : id)} />`}
        </div>
        ${editing === id && !status[id] && html`<${KeyPrompt} name=${id} compact onSaved=${() => { setEditing(null); load(); }} />`}
      </div>`,
    )}
  </div>`;
}
