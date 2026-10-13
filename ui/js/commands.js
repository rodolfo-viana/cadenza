// Command registry: one place for labels, shortcuts and behavior. Used by
// keyboard handling, menus and the command palette.

import { invoke } from './api.js';
import { t, N_ } from './i18n.js';
import * as S from './store.js';
import * as A from './actions.js';

const hasDoc = () => !!S.state.doc;
const hasSel = () => S.state.selection.length > 0;
const singleSel = () => S.state.selection.length === 1;

/**
 * - `keys`: shortcuts ("Mod" = Ctrl, or ⌘ on macOS)
 * - `global`: also works while typing in a text field
 * - `when`: enabled state
 */
export const COMMANDS = [
  // File
  { id: 'file.new', group: N_('File'), label: N_('New Presentation…'), icon: 'file-plus', keys: ['Mod+N'], global: true,
    run: () => (S.state.doc ? invoke('window_new', { path: null }) : S.closeDocument()) },
  { id: 'file.newWindow', group: N_('File'), label: N_('New Window'), icon: 'app-window', keys: ['Mod+Shift+N'], global: true,
    run: () => invoke('window_new', { path: null }) },
  { id: 'file.open', group: N_('File'), label: N_('Open…'), icon: 'folder-open', keys: ['Mod+O'], global: true, run: S.openDialog },
  { id: 'file.save', group: N_('File'), label: N_('Save'), icon: 'save', keys: ['Mod+S'], global: true, when: hasDoc, run: () => S.save() },
  { id: 'file.saveAs', group: N_('File'), label: N_('Save As…'), keys: ['Mod+Shift+S'], global: true, when: hasDoc, run: () => S.save({ as: true }) },
  { id: 'file.close', group: N_('File'), label: N_('Close Presentation'), keys: ['Mod+W'], global: true, when: hasDoc, run: S.closeDocument },
  { id: 'file.importMd', group: N_('File'), label: N_('Import Markdown…'), icon: 'file-down', when: hasDoc, run: A.importMarkdown },
  { id: 'file.exportFolder', group: N_('Export'), label: N_('Export as Website (folder)…'), icon: 'folder-output', when: hasDoc, run: () => A.exportHtml(false) },
  { id: 'file.exportSingle', group: N_('Export'), label: N_('Export as Single HTML File…'), icon: 'file-code-2', when: hasDoc, run: () => A.exportHtml(true) },
  { id: 'file.exportPdf', group: N_('Export'), label: N_('Export as PDF / Print…'), icon: 'file-text', keys: ['Mod+P'], global: true, when: hasDoc, run: A.exportPdf },

  // Edit
  { id: 'edit.undo', group: N_('Edit'), label: N_('Undo'), icon: 'undo-2', keys: ['Mod+Z'], when: () => S.state.doc?.canUndo, run: S.undo },
  { id: 'edit.redo', group: N_('Edit'), label: N_('Redo'), icon: 'redo-2', keys: ['Mod+Shift+Z', 'Mod+Y'], when: () => S.state.doc?.canRedo, run: S.redo },
  { id: 'edit.duplicate', group: N_('Edit'), label: N_('Duplicate'), icon: 'copy', keys: ['Mod+D'], when: hasDoc,
    run: () => (S.state.focus === 'slides' ? A.duplicateSlides() : A.duplicateSelection()) },
  { id: 'edit.delete', group: N_('Edit'), label: N_('Delete'), icon: 'trash-2', keys: ['Delete', 'Backspace'], when: hasDoc,
    run: () => (S.state.focus === 'slides' ? A.deleteSlides() : A.deleteSelection()) },
  { id: 'edit.selectAll', group: N_('Edit'), label: N_('Select All'), keys: ['Mod+A'], when: hasDoc, run: A.selectAll },
  { id: 'edit.deselect', group: N_('Edit'), label: N_('Deselect'), keys: ['Escape'], when: hasDoc, hidden: true,
    run: () => S.set({ selection: [], menu: null }) },
  { id: 'edit.lock', group: N_('Arrange'), label: N_('Lock / Unlock'), icon: 'lock', keys: ['Mod+L'], when: hasSel, run: A.toggleLock },

  // Insert
  { id: 'insert.text', group: N_('Insert'), label: N_('Text Box'), icon: 'type', keys: ['Mod+Shift+T'], when: hasDoc, run: () => A.insert('text', 'body') },
  { id: 'insert.title', group: N_('Insert'), label: N_('Title'), icon: 'heading', when: hasDoc, run: () => A.insert('text', 'title') },
  { id: 'insert.shape', group: N_('Insert'), label: N_('Rectangle'), icon: 'square', when: hasDoc, run: () => A.insert('shape', 'rect') },
  { id: 'insert.ellipse', group: N_('Insert'), label: N_('Oval'), icon: 'circle', when: hasDoc, run: () => A.insert('shape', 'ellipse') },
  { id: 'insert.line', group: N_('Insert'), label: N_('Arrow Line'), icon: 'arrow-right', when: hasDoc, run: () => A.insert('shape', 'line') },
  { id: 'insert.image', group: N_('Insert'), label: N_('Image…'), icon: 'image', keys: ['Mod+Shift+I'], when: hasDoc, run: A.insertImage },
  { id: 'insert.video', group: N_('Insert'), label: N_('Video File…'), icon: 'film', when: hasDoc, run: A.insertVideoFile },
  { id: 'insert.onlineVideo', group: N_('Insert'), label: N_('Online Video (YouTube, Vimeo)…'), icon: 'square-play', when: hasDoc, run: () => import('./media.js').then((m) => m.openMediaLibrary('videos')) },
  { id: 'media.photos', group: N_('Insert'), label: N_('Photos (Unsplash, Openverse)…'), icon: 'images', keys: ['Mod+Shift+M'], when: hasDoc, run: () => import('./media.js').then((m) => m.openMediaLibrary('photos')) },
  { id: 'media.gifs', group: N_('Insert'), label: N_('GIFs (GIPHY)…'), icon: 'sticker', when: hasDoc, run: () => import('./media.js').then((m) => m.openMediaLibrary('gifs')) },
  { id: 'media.videos', group: N_('Insert'), label: N_('YouTube & Vimeo…'), icon: 'square-play', when: hasDoc, run: () => import('./media.js').then((m) => m.openMediaLibrary('videos')) },
  { id: 'browse.youtube', group: N_('Insert'), label: N_('Browse YouTube…'), icon: 'square-play', when: hasDoc, run: () => import('./media.js').then((m) => m.browseProvider('youtube')) },
  { id: 'browse.unsplash', group: N_('Insert'), label: N_('Browse Unsplash…'), icon: 'images', when: hasDoc, run: () => import('./media.js').then((m) => m.browseProvider('unsplash')) },
  { id: 'browse.giphy', group: N_('Insert'), label: N_('Browse GIPHY…'), icon: 'sticker', when: hasDoc, run: () => import('./media.js').then((m) => m.browseProvider('giphy')) },
  { id: 'insert.web', group: N_('Insert'), label: N_('Web Page…'), icon: 'globe', when: hasDoc, run: A.insertWebPage },
  { id: 'insert.table', group: N_('Insert'), label: N_('Table'), icon: 'table', when: hasDoc, run: () => A.insert('table') },
  { id: 'insert.chart', group: N_('Insert'), label: N_('Chart'), icon: 'chart-column', when: hasDoc, run: () => A.insert('chart', 'column') },
  { id: 'insert.code', group: N_('Insert'), label: N_('Code Block'), icon: 'code', when: hasDoc, run: () => A.insert('code') },
  { id: 'insert.math', group: N_('Insert'), label: N_('Equation (LaTeX)'), icon: 'sigma', when: hasDoc, run: () => A.insert('math') },
  { id: 'insert.diagram', group: N_('Insert'), label: N_('Diagram (Mermaid)'), icon: 'workflow', when: hasDoc, run: () => A.insert('diagram') },

  // Slides
  { id: 'slide.new', group: N_('Slide'), label: N_('New Slide'), icon: 'plus', keys: ['Mod+M', 'Mod+Enter'], when: hasDoc, run: () => A.addSlide() },
  { id: 'slide.duplicate', group: N_('Slide'), label: N_('Duplicate Slide'), icon: 'copy', keys: ['Mod+Shift+D'], when: hasDoc, run: () => A.duplicateSlides() },
  { id: 'slide.delete', group: N_('Slide'), label: N_('Delete Slide'), icon: 'trash-2', when: hasDoc, run: () => A.deleteSlides() },
  { id: 'slide.skip', group: N_('Slide'), label: N_('Skip / Unskip Slide'), icon: 'eye-off', when: hasDoc, run: () => A.toggleSkip() },
  { id: 'slide.indent', group: N_('Slide'), label: N_('Make Vertical Sub-slide'), icon: 'indent-increase', when: hasDoc, run: () => A.setNested(true) },
  { id: 'slide.outdent', group: N_('Slide'), label: N_('Make Top-level Slide'), icon: 'indent-decrease', when: hasDoc, run: () => A.setNested(false) },
  { id: 'slide.next', group: N_('Slide'), label: N_('Next Slide'), keys: ['PageDown'], when: hasDoc, hidden: true, run: () => S.stepSlide(1) },
  { id: 'slide.prev', group: N_('Slide'), label: N_('Previous Slide'), keys: ['PageUp'], when: hasDoc, hidden: true, run: () => S.stepSlide(-1) },

  // Arrange
  { id: 'arrange.front', group: N_('Arrange'), label: N_('Bring to Front'), icon: 'bring-to-front', keys: ['Mod+Shift+]'], when: hasSel, run: () => A.arrange('front') },
  { id: 'arrange.forward', group: N_('Arrange'), label: N_('Bring Forward'), icon: 'arrow-up', keys: ['Mod+]'], when: hasSel, run: () => A.arrange('forward') },
  { id: 'arrange.backward', group: N_('Arrange'), label: N_('Send Backward'), icon: 'arrow-down', keys: ['Mod+['], when: hasSel, run: () => A.arrange('backward') },
  { id: 'arrange.back', group: N_('Arrange'), label: N_('Send to Back'), icon: 'send-to-back', keys: ['Mod+Shift+['], when: hasSel, run: () => A.arrange('back') },
  { id: 'align.left', group: N_('Arrange'), label: N_('Align Left'), icon: 'align-start-vertical', when: hasSel, run: () => A.align('left') },
  { id: 'align.hcenter', group: N_('Arrange'), label: N_('Align Centers Horizontally'), icon: 'align-center-vertical', when: hasSel, run: () => A.align('hcenter') },
  { id: 'align.right', group: N_('Arrange'), label: N_('Align Right'), icon: 'align-end-vertical', when: hasSel, run: () => A.align('right') },
  { id: 'align.top', group: N_('Arrange'), label: N_('Align Top'), icon: 'align-start-horizontal', when: hasSel, run: () => A.align('top') },
  { id: 'align.vcenter', group: N_('Arrange'), label: N_('Align Middles'), icon: 'align-center-horizontal', when: hasSel, run: () => A.align('vcenter') },
  { id: 'align.bottom', group: N_('Arrange'), label: N_('Align Bottom'), icon: 'align-end-horizontal', when: hasSel, run: () => A.align('bottom') },
  { id: 'align.distH', group: N_('Arrange'), label: N_('Distribute Horizontally'), icon: 'align-horizontal-distribute-center', when: hasSel, run: () => A.align('distribute-h') },
  { id: 'align.distV', group: N_('Arrange'), label: N_('Distribute Vertically'), icon: 'align-vertical-distribute-center', when: hasSel, run: () => A.align('distribute-v') },
  { id: 'align.centerSlide', group: N_('Arrange'), label: N_('Center on Slide'), icon: 'move', when: hasSel,
    run: async () => { await A.align('hcenter', true); await A.align('vcenter', true); } },

  // Play
  { id: 'play.start', group: N_('Play'), label: N_('Play from Beginning'), icon: 'play', keys: ['F5', 'Mod+Alt+P'], global: true, when: hasDoc, run: () => S.play(false) },
  { id: 'play.current', group: N_('Play'), label: N_('Play from Current Slide'), icon: 'monitor-play', keys: ['Shift+F5'], global: true, when: hasDoc, run: () => S.play(true) },

  // View
  { id: 'view.zoomIn', group: N_('View'), label: N_('Zoom In'), icon: 'zoom-in', keys: ['Mod+=', 'Mod+Shift+=', 'Mod++'], global: true, when: hasDoc, run: () => zoomBy(1.25) },
  { id: 'view.zoomOut', group: N_('View'), label: N_('Zoom Out'), icon: 'zoom-out', keys: ['Mod+-'], global: true, when: hasDoc, run: () => zoomBy(0.8) },
  { id: 'view.zoomFit', group: N_('View'), label: N_('Zoom to Fit'), icon: 'maximize', keys: ['Mod+0'], global: true, when: hasDoc, run: () => S.set({ zoom: 'fit' }) },
  { id: 'view.zoom100', group: N_('View'), label: N_('Actual Size'), keys: ['Mod+1'], global: true, when: hasDoc, run: () => S.set({ zoom: 1 }) },
  { id: 'view.inspector', group: N_('View'), label: N_('Show / Hide Inspector'), icon: 'panel-right', keys: ['Mod+Alt+I'], global: true,
    run: () => S.updateSettings({ showInspector: !S.settings().showInspector }) },
  { id: 'view.notes', group: N_('View'), label: N_('Show / Hide Speaker Notes'), icon: 'panel-bottom', keys: ['Mod+Alt+N'], global: true,
    run: () => S.updateSettings({ showNotes: !S.settings().showNotes }) },
  { id: 'view.rulers', group: N_('View'), label: N_('Show / Hide Rulers'), icon: 'ratio', keys: ['Mod+R'], global: true, when: hasDoc,
    run: () => S.updateSettings({ showRulers: S.settings().showRulers === false }) },
  { id: 'view.guides', group: N_('View'), label: N_('Show / Hide Guides'), icon: 'ratio', keys: ['Mod+;'], global: true, when: hasDoc,
    run: () => S.updateSettings({ showGuides: S.settings().showGuides === false }) },
  { id: 'guides.addV', group: N_('View'), label: N_('Add Vertical Guide at Center'), when: hasDoc, run: () => import('./canvas.js').then((m) => m.addGuide('x')) },
  { id: 'guides.addH', group: N_('View'), label: N_('Add Horizontal Guide at Center'), when: hasDoc, run: () => import('./canvas.js').then((m) => m.addGuide('y')) },
  { id: 'guides.clear', group: N_('View'), label: N_('Clear All Guides'), when: () => hasDoc() && (S.state.doc.deck.guides.x.length + S.state.doc.deck.guides.y.length) > 0,
    run: () => S.updateDeck({ guides: { x: [], y: [] } }) },
  { id: 'view.format', group: N_('View'), label: N_('Format Inspector'), icon: 'paintbrush', when: hasDoc, run: () => showTab('format') },
  { id: 'view.animate', group: N_('View'), label: N_('Animate Inspector'), icon: 'sparkles', when: hasDoc, run: () => showTab('animate') },
  { id: 'view.document', group: N_('View'), label: N_('Document Inspector'), icon: 'file-text', when: hasDoc, run: () => showTab('document') },

  // Themes
  { id: 'theme.builder', group: N_('Theme'), label: N_('Theme Builder…'), icon: 'swatch-book', when: hasDoc, run: () => import('./themebuilder.js').then((m) => m.openThemeBuilder()) },
  { id: 'theme.import', group: N_('Theme'), label: N_('Import Theme…'), icon: 'upload', when: hasDoc,
    run: () => import('./themebuilder.js').then(async (m) => { const d = await m.importTheme(); if (d) m.applyCustomTheme(d); }) },

  // AI assistant (only when turned on in Settings)
  { id: 'ai.assistant', group: N_('AI'), label: N_('Ask the Assistant…'), icon: 'wand-sparkles', keys: ['Mod+J'], global: true,
    when: () => hasDoc() && !!S.settings().aiEnabled, hidden: () => !S.settings().aiEnabled,
    run: () => import('./ai.js').then((m) => m.askAssistant()) },
  { id: 'ai.generate', group: N_('AI'), label: N_('Create a Presentation with AI…'), icon: 'wand-sparkles',
    when: () => !!S.settings().aiEnabled, hidden: () => !S.settings().aiEnabled,
    run: () => (S.state.doc ? invoke('window_new', { path: null }) : S.openModal('aiGenerate', {})) },
  { id: 'ai.notes', group: N_('AI'), label: N_('Write Speaker Notes for All Slides (AI)'), icon: 'message-square-text',
    when: () => hasDoc() && !!S.settings().aiEnabled, hidden: () => !S.settings().aiEnabled,
    run: () => import('./ai.js').then((m) => m.askAssistant(t('Write speaker notes for every slide'))) },

  // App
  { id: 'app.palette', group: N_('Help'), label: N_('Command Palette…'), icon: 'search', keys: ['Mod+K', 'Mod+Shift+P'], global: true, run: () => S.openModal('palette') },
  { id: 'app.settings', group: N_('Help'), label: N_('Settings…'), icon: 'settings', keys: ['Mod+,'], global: true, run: () => S.openModal('settings') },
  { id: 'app.shortcuts', group: N_('Help'), label: N_('Keyboard Shortcuts'), icon: 'keyboard', keys: ['Mod+/', 'F1'], global: true, run: () => S.openModal('shortcuts') },
  { id: 'app.about', group: N_('Help'), label: N_('About Cadenza'), icon: 'info', run: () => S.openModal('about') },
];

function showTab(tab) {
  if (!S.settings().showInspector) S.updateSettings({ showInspector: true });
  S.set({ tab });
}

function zoomBy(f) {
  const z = S.state.zoom === 'fit' ? S.state.fitScale || 1 : S.state.zoom;
  S.set({ zoom: Math.min(4, Math.max(0.1, Math.round(z * f * 100) / 100)) });
}

export const byId = Object.fromEntries(COMMANDS.map((c) => [c.id, c]));

const byKey = new Map();
for (const c of COMMANDS) for (const k of c.keys || []) byKey.set(k, c);

export const commandForKey = (combo) => byKey.get(combo);

export const enabled = (c) => !c.when || !!c.when();

export function run(id) {
  const c = byId[id];
  if (c && enabled(c)) return c.run();
}

/** A menu item for a command id (for context and dropdown menus). */
export function item(id, extra = {}) {
  const c = byId[id];
  return { label: t(c.label), icon: c.icon, keys: c.keys?.[0], disabled: !enabled(c), run: c.run, ...extra };
}

export { singleSel };
