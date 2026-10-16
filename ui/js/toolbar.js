// The top toolbar.

import { html, useState } from './lib.js';
import { Icon } from './icons.js';
import { state, set, useStore, settings, updateSettings, undo, redo, play, updateDeck, deckTitle } from './store.js';
import { ToolButton, showMenuAt } from './ui.js';
import { item, COMMANDS } from './commands.js';
import * as A from './actions.js';
import { ShapeGlyph } from './panels.js';
import { cls, fileName } from './util.js';
import { t } from './i18n.js';
import { CHART_ICONS, chartSample } from './charts.js';

/** A menu item for a command, with its (English) label translated. */
// Menu entry for a command (item() translates the command's label).
const cmd = item;

function appMenu() {
  const recent = (state.info.recent || []).slice(0, 8);
  return [
    cmd('file.new', { label: t('New Presentation…') }),
    cmd('file.newWindow'),
    cmd('file.open'),
    {
      label: t('Open Recent'),
      icon: 'history',
      disabled: !recent.length,
      children: recent.map((r) => ({ label: r.name, run: () => import('./store.js').then((m) => m.openPath(r.path)) })),
    },
    '-',
    cmd('file.save'),
    cmd('file.saveAs'),
    '-',
    cmd('file.importMd'),
    {
      label: t('Export'),
      icon: 'share',
      children: [cmd('file.exportFolder'), cmd('file.exportSingle'), cmd('file.exportPdf')],
    },
    '-',
    cmd('app.palette'),
    cmd('app.settings'),
    cmd('app.shortcuts'),
    cmd('app.about'),
    '-',
    cmd('file.close'),
  ];
}

function shapeMenu() {
  return state.info.shapes.map((s) => ({
    label: t(s.name),
    icon: null,
    run: () => A.insert('shape', s.id),
    style: null,
    keys: null,
    checked: false,
    glyph: s.id,
  }));
}

function mediaMenu() {
  return [
    { heading: t('Media Library') },
    cmd('media.photos'),
    cmd('media.gifs'),
    cmd('media.videos'),
    '-',
    { heading: t('Browse the web') },
    cmd('browse.youtube'),
    cmd('browse.giphy'),
    '-',
    { heading: t('From this computer') },
    cmd('insert.image'),
    cmd('insert.video'),
    cmd('insert.camera'),
    '-',
    cmd('insert.web'),
  ];
}

function textMenu() {
  return [
    { label: t('Title'), icon: 'heading', run: () => A.insert('text', 'title') },
    { label: t('Subtitle'), icon: 'type', run: () => A.insert('text', 'subtitle') },
    { label: t('Heading'), icon: 'heading', run: () => A.insert('text', 'heading') },
    { label: t('Body Text'), icon: 'pilcrow', run: () => A.insert('text', 'body') },
    { label: t('Caption'), icon: 'type', run: () => A.insert('text', 'caption') },
    { label: t('Quote'), icon: 'quote', run: () => A.insert('text', 'quote') },
  ];
}

function chartMenu() {
  return state.info.charts.map((c) => ({ label: t(c.name), icon: CHART_ICONS[c.id], run: () => A.insert('chart', c.id, { content: chartSample(c.id) }) }));
}

function zoomMenu() {
  return [
    { label: t('Fit to Window'), checked: state.zoom === 'fit', keys: 'Mod+0', run: () => set({ zoom: 'fit' }) },
    '-',
    ...[0.5, 0.75, 1, 1.25, 1.5, 2].map((z) => ({ label: `${z * 100}%`, checked: state.zoom === z, run: () => set({ zoom: z }) })),
    '-',
    { label: t('Rulers'), checked: settings().showRulers !== false, keys: 'Mod+R', run: () => updateSettings({ showRulers: settings().showRulers === false }) },
    { label: t('Guides'), checked: settings().showGuides !== false, keys: 'Mod+;', run: () => updateSettings({ showGuides: settings().showGuides === false }) },
    { label: t('Show Grid'), checked: settings().showGrid, run: () => updateSettings({ showGrid: !settings().showGrid }) },
    { label: t('Snap to Guides'), checked: settings().snap !== false, run: () => updateSettings({ snap: settings().snap === false }) },
    { label: t('Speaker Notes'), checked: settings().showNotes, keys: 'Mod+Alt+N', run: () => updateSettings({ showNotes: !settings().showNotes }) },
  ];
}

function playMenu() {
  return [
    cmd('play.start'),
    cmd('play.current'),
    '-',
    {
      label: t('Use Presenter Display'),
      checked: settings().presenterView,
      // The teleprompter is part of the presenter display.
      run: () => updateSettings(settings().presenterView ? { presenterView: false, teleprompter: { enabled: false } } : { presenterView: true }),
    },
    {
      label: t('Teleprompter'),
      checked: !!settings().teleprompter?.enabled,
      run: () => updateSettings(settings().teleprompter?.enabled ? { teleprompter: { enabled: false } } : { presenterView: true, teleprompter: { enabled: true } }),
    },
  ];
}

export function Toolbar() {
  const st = useStore();
  const doc = st.doc;
  const [editingTitle, setEditingTitle] = useState(false);
  const zoomLabel = st.zoom === 'fit' ? t('zoom::Fit') : `${Math.round(st.zoom * 100)}%`;
  const menu = (fn, align) => (e) => showMenuAt(e.currentTarget, fn, align);
  const tab = (name) => {
    if (!settings().showInspector) updateSettings({ showInspector: true });
    else if (st.tab === name) {
      updateSettings({ showInspector: false });
      return;
    }
    set({ tab: name });
  };
  return html`<header class="toolbar">
    <div class="tb-left">
      <button type="button" class="tb-menu" title=${t('Menu')} onClick=${menu(appMenu)}><${Icon} name="menu" size=${19} /></button>
      <div class="doc-title">
        ${editingTitle
          ? html`<input
              class="title-input"
              value=${deckTitle()}
              ref=${(el) => el && !el.dataset.f && ((el.dataset.f = 1), el.focus(), el.select())}
              onBlur=${(e) => {
                setEditingTitle(false);
                if (e.target.value.trim() && e.target.value !== deckTitle()) updateDeck({ title: e.target.value.trim() });
              }}
              onKeyDown=${(e) => {
                if (e.key === 'Enter') e.target.blur();
                if (e.key === 'Escape') {
                  e.target.value = deckTitle();
                  e.target.blur();
                }
              }}
            />`
          : html`<button type="button" class="title-btn" title=${doc.path || t('Not saved yet — click to rename')} onClick=${() => setEditingTitle(true)}>
              <span class="title-text">${deckTitle() || t('Untitled')}</span>
              <span class=${cls('save-state', doc.dirty && 'dirty')}>${doc.dirty ? t('Edited') : doc.path ? fileName(doc.path) : t('Not saved')}</span>
            </button>`}
      </div>
    </div>
    <div class="tb-center">
      <${ToolButton}
        icon="undo-2"
        label=${t('Undo')}
        title=${doc.undoLabel ? t('Undo {action}', { action: t(doc.undoLabel) }) : t('Undo')}
        disabled=${!doc.canUndo}
        onClick=${undo}
      />
      <${ToolButton}
        icon="redo-2"
        label=${t('Redo')}
        title=${doc.redoLabel ? t('Redo {action}', { action: t(doc.redoLabel) }) : t('Redo')}
        disabled=${!doc.canRedo}
        onClick=${redo}
      />
      <span class="tb-sep" />
      <${ToolButton} icon="type" label=${t('Text')} caret onClick=${menu(textMenu)} />
      <${ToolButton} icon="shapes" label=${t('Shape')} caret onClick=${(e) => showShapePicker(e.currentTarget)} />
      <${ToolButton} icon="image" label=${t('Media')} caret onClick=${menu(mediaMenu)} />
      <${ToolButton} icon="table" label=${t('Table')} onClick=${() => A.insert('table')} />
      <${ToolButton} icon="chart-column" label=${t('Chart')} caret onClick=${menu(chartMenu)} />
      <${ToolButton} icon="code" label=${t('Code')} onClick=${() => A.insert('code')} />
      <${ToolButton} icon="sigma" label=${t('Equation')} onClick=${() => A.insert('math')} />
      <${ToolButton} icon="workflow" label=${t('Diagram')} onClick=${() => A.insert('diagram')} />
      <span class="tb-sep" />
      <${ToolButton} icon="zoom-in" label=${zoomLabel} caret onClick=${menu(zoomMenu)} />
      <button type="button" class="tool play" title=${t('Play (F5)')} onClick=${() => play(false)} onContextMenu=${(e) => { e.preventDefault(); showMenuAt(e.currentTarget, playMenu); }}>
        <span class="tool-icon"><${Icon} name="play" size=${20} /></span>
        <span class="tool-label">${t('Play')}</span>
      </button>
      <button type="button" class="play-caret" title=${t('Play options')} onClick=${menu(playMenu)}><${Icon} name="chevron-down" size=${13} /></button>
    </div>
    <div class="tb-right">
      <${ToolButton} icon="paintbrush" label=${t('Format')} active=${settings().showInspector && st.tab === 'format'} onClick=${() => tab('format')} />
      <${ToolButton} icon="sparkles" label=${t('Animate')} active=${settings().showInspector && st.tab === 'animate'} onClick=${() => tab('animate')} />
      <${ToolButton} icon="file-text" label=${t('Document')} active=${settings().showInspector && st.tab === 'document'} onClick=${() => tab('document')} />
    </div>
  </header>`;
}

/** A grid popover of shapes (rendered through the menu host). */
function showShapePicker(anchor) {
  const r = anchor.getBoundingClientRect();
  set({ shapePicker: { x: r.left, y: r.bottom + 4 } });
}

export function ShapePicker() {
  const st = useStore();
  if (!st.shapePicker) return null;
  const close = () => set({ shapePicker: null });
  return html`<div class="picker-backdrop" onMouseDown=${close}>
    <div class="shape-picker" style=${{ left: st.shapePicker.x + 'px', top: st.shapePicker.y + 'px' }} onMouseDown=${(e) => e.stopPropagation()}>
      ${st.info.shapes.map(
        (s) => html`<button
          type="button"
          class="shape-opt"
          title=${t(s.name)}
          onClick=${() => {
            close();
            A.insert('shape', s.id);
          }}
        >
          <${ShapeGlyph} id=${s.id} />
        </button>`,
      )}
    </div>
  </div>`;
}

export { COMMANDS };
