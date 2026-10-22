// The top toolbar.

import { html, useEffect, useRef, useState } from './lib.js';
import { Icon } from './icons.js';
import { state, set, useStore, settings, updateSettings, undo, redo, play, updateDeck, deckTitle, openModal, registerFlush } from './store.js';
import { ToolButton, showMenuAt } from './ui.js';
import { item, COMMANDS } from './commands.js';
import * as A from './actions.js';
import { ShapeGlyph } from './panels.js';
import { cls, fileName, recentLabels } from './util.js';
import { t, isRtl } from './i18n.js';
import { CHART_ICONS, chartSample } from './charts.js';

/** A menu item for a command, with its (English) label translated. */
// Menu entry for a command (item() translates the command's label).
const cmd = item;

function appMenu() {
  const recent = (state.info.recent || []).slice(0, 8);
  const labels = recentLabels(recent);
  return [
    cmd('file.new', { label: t('New Presentation…') }),
    cmd('file.newWindow'),
    cmd('file.open'),
    {
      label: t('Open Recent'),
      icon: 'history',
      disabled: !recent.length,
      children: recent.map((r, i) => ({ label: labels[i], run: () => import('./store.js').then((m) => m.openPath(r.path)) })),
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
    ...(state.info.updates ? [cmd('app.updates')] : []),
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
    { label: t('Title'), icon: 'heading', run: () => A.insertText('title') },
    { label: t('Subtitle'), icon: 'type', run: () => A.insertText('subtitle') },
    { label: t('Heading'), icon: 'heading', run: () => A.insertText('heading') },
    { label: t('Body Text'), icon: 'pilcrow', run: () => A.insertText('body') },
    { label: t('Caption'), icon: 'type', run: () => A.insertText('caption') },
    { label: t('Quote'), icon: 'quote', run: () => A.insertText('quote') },
  ];
}

function chartMenu() {
  return [
    { label: t('From Data…'), icon: 'sheet', run: () => openModal('chartData') },
    '-',
    ...state.info.charts.map((c) => ({ label: t(c.name), icon: CHART_ICONS[c.id], run: () => A.insert('chart', c.id, { content: chartSample(c.id) }) })),
  ];
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

/**
 * Renames the presentation. What is typed is a draft of its own: other
 * updates (a toast, Save's busy state) leave it alone, and Save, Close and
 * Play commit it first. Enter or leaving the field commits; Escape cancels.
 */
function TitleField({ onDone }) {
  const [text, setText] = useState(deckTitle);
  const draft = useRef(text);
  const cancel = useRef(false);
  const commit = () => {
    const v = draft.current.trim();
    return v && v !== deckTitle() ? updateDeck({ title: v }) : undefined;
  };
  useEffect(() => registerFlush(commit), []);
  return html`<input
    class="title-input"
    value=${text}
    ref=${(el) => el && !el.dataset.f && ((el.dataset.f = 1), el.focus(), el.select())}
    onInput=${(e) => {
      draft.current = e.target.value;
      setText(e.target.value);
    }}
    onBlur=${() => {
      if (!cancel.current) commit();
      onDone();
    }}
    onKeyDown=${(e) => {
      if (e.key === 'Enter') e.target.blur();
      if (e.key === 'Escape') {
        cancel.current = true;
        e.target.blur();
      }
    }}
  />`;
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
          ? html`<${TitleField} onDone=${() => setEditingTitle(false)} />`
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

/** Shapes in a row of the picker (see .shape-picker in app.css). */
const PICKER_COLUMNS = 7;

/**
 * The Shape button's grid of shapes. It works like a menu: the arrow keys
 * move through it (mirrored in RTL), Enter or Space inserts, Escape closes
 * it and gives the focus back, and a click elsewhere closes it and still
 * does what it was for.
 */
export function ShapePicker() {
  const st = useStore();
  const ref = useRef();
  const open = !!st.shapePicker;
  useEffect(() => {
    if (!open) return;
    const before = document.activeElement;
    const close = (back) => {
      set({ shapePicker: null });
      if (back && before?.isConnected) before.focus();
    };
    const onDown = (e) => {
      if (!ref.current?.contains(e.target)) close(false);
    };
    const onKey = (e) => {
      const list = [...(ref.current?.querySelectorAll('.shape-opt') || [])];
      const at = list.indexOf(document.activeElement);
      const [forward, back] = isRtl() ? ['ArrowLeft', 'ArrowRight'] : ['ArrowRight', 'ArrowLeft'];
      const steps = { [forward]: 1, [back]: -1, ArrowDown: PICKER_COLUMNS, ArrowUp: -PICKER_COLUMNS };
      let next = null;
      if (e.key === 'Escape') close(true);
      else if (e.key === 'Tab') return close(false);
      else if (e.key in steps) next = at < 0 ? 0 : Math.min(list.length - 1, Math.max(0, at + steps[e.key]));
      else if (e.key === 'Home') next = 0;
      else if (e.key === 'End') next = list.length - 1;
      // Other keys work as usual: Enter and Space press the focused shape.
      else return;
      e.preventDefault();
      e.stopPropagation();
      if (next !== null) list[next]?.focus();
    };
    window.addEventListener('mousedown', onDown, true);
    window.addEventListener('keydown', onKey, true);
    return () => {
      window.removeEventListener('mousedown', onDown, true);
      window.removeEventListener('keydown', onKey, true);
    };
  }, [open]);
  if (!open) return null;
  return html`<div class="shape-picker" ref=${ref} role="menu" aria-label=${t('Shape')} style=${{ left: st.shapePicker.x + 'px', top: st.shapePicker.y + 'px' }}>
    ${st.info.shapes.map(
      (s) => html`<button
        type="button"
        class="shape-opt"
        role="menuitem"
        tabindex="-1"
        title=${t(s.name)}
        aria-label=${t(s.name)}
        onClick=${() => {
          set({ shapePicker: null });
          A.insert('shape', s.id);
        }}
      >
        <${ShapeGlyph} id=${s.id} />
      </button>`,
    )}
  </div>`;
}

export { COMMANDS };
