// Editor actions shared by the toolbar, menus, shortcuts and the command
// palette.

import { invoke, importBytes, dialog, opener, clipboard, IMAGE_EXTS, VIDEO_EXTS } from './api.js';
import {
  state,
  set,
  apply,
  deck,
  currentSlide,
  selectedElements,
  updateSelected,
  updateSlides,
  flushEdits,
  toast,
  ask,
  copyElements,
  copySlides,
  readClip,
  pasteClip,
  goToSlide,
  ingestView,
  deckTitle,
} from './store.js';
import { t, N_ } from './i18n.js';
import { chartSteps } from './charts.js';
import { fileExt, fileName, textToHtml, bounds, unionBounds } from './util.js';

// ---------------------------------------------------------------------------
// Inserting elements
// ---------------------------------------------------------------------------

/** Adds an element of a kind (and variant: shape id, chart type, text role). */
export async function insert(kind, variant = '', props = null, target = { docId: state.doc?.docId, slideId: state.slideId }) {
  if (!target.docId || state.doc?.docId !== target.docId || !target.slideId) return null;
  if (!(await flushEdits())) return null;
  if (state.doc?.docId !== target.docId || !deck().slides.some((s) => s.id === target.slideId)) return null;
  set({ editing: null, editingTable: null });
  const out = await apply({ op: 'addElement', slide: target.slideId, kind, variant, props }, { docId: target.docId });
  return out?.created?.[0] || null;
}

function fitInto(w, h, maxW, maxH) {
  const s = Math.min(1, maxW / Math.max(1, w), maxH / Math.max(1, h));
  return { w: Math.round(w * s), h: Math.round(h * s) };
}

/** Creates an element for an imported asset, centered at `at` if given. */
export async function insertAsset(asset, at = null, offset = 0, extra = {}, target = { docId: state.doc?.docId, slideId: state.slideId }) {
  const d = deck();
  if (!d || state.doc.docId !== target.docId) return null;
  let kind;
  let size;
  let content;
  if (asset.kind === 'image') {
    kind = 'image';
    size = fitInto(asset.width || 800, asset.height || 600, d.width * 0.7, d.height * 0.7);
    content = { kind: 'image', src: asset.name, naturalWidth: asset.width, naturalHeight: asset.height, fit: 'cover', alt: extra.alt || '' };
  } else if (asset.kind === 'video') {
    kind = 'video';
    size = { w: Math.round(d.width * 0.6), h: Math.round(d.width * 0.6 * 9 / 16) };
    content = { kind: 'video', src: asset.name };
  } else {
    toast(t('{name}: unsupported file type', { name: asset.name }), 'error');
    return null;
  }
  const cx = at ? at.x : d.width / 2;
  const cy = at ? at.y : d.height / 2;
  const props = {
    x: Math.round(cx - size.w / 2 + offset),
    y: Math.round(cy - size.h / 2 + offset),
    w: size.w,
    h: size.h,
    name: kind === 'image' ? N_('Image') : N_('Video'),
    content,
  };
  return insert(kind, '', props, target);
}

export async function importPaths(paths, at = null) {
  let i = 0;
  for (const path of paths) {
    try {
      const asset = await invoke('asset_import_file', { path });
      await insertAsset(asset, at, i * 24);
      i++;
    } catch (e) {
      toast(t('{file}: {error}', { file: fileName(path), error: t(String(e)) }), 'error');
    }
  }
}

export async function insertImage() {
  const paths = await dialog.open({ title: t('Insert Image'), multiple: true, filters: [{ name: t('Images'), extensions: IMAGE_EXTS }] });
  if (paths) await importPaths([].concat(paths));
}

export async function insertVideoFile() {
  const path = await dialog.open({ title: t('Insert Video'), multiple: false, filters: [{ name: t('Videos'), extensions: VIDEO_EXTS }] });
  if (path) await importPaths([path]);
}

export async function insertOnlineVideo() {
  const url = await ask('prompt', {
    title: t('Insert Online Video'),
    label: t('YouTube or Vimeo address'),
    placeholder: 'https://www.youtube.com/watch?v=…',
    confirm: t('Insert'),
  });
  if (url) await insert('video', '', { name: N_('Video'), content: { kind: 'video', src: url.trim(), autoplay: false } });
}

export async function insertWebPage() {
  const url = await ask('prompt', { title: t('Embed Web Page'), label: t('Page address'), placeholder: 'https://example.com', confirm: t('Embed') });
  if (!url) return;
  const u = /^https?:\/\//i.test(url.trim()) ? url.trim() : `https://${url.trim()}`;
  await insert('embed', '', { name: N_('Web Page'), content: { kind: 'embed', url: u } });
}

/** Replaces the image of an image element (or fills a placeholder). */
export async function replaceImage(el) {
  const path = await dialog.open({ title: t('Choose Image'), multiple: false, filters: [{ name: t('Images'), extensions: IMAGE_EXTS }] });
  if (!path) return;
  try {
    const a = await invoke('asset_import_file', { path });
    const patch = { content: { src: a.name, naturalWidth: a.width, naturalHeight: a.height } };
    if (!el.content.src && a.width && a.height) {
      // Placeholders keep their frame; images fill it.
      patch.content.fit = 'cover';
    }
    await apply({ op: 'updateElements', slide: state.slideId, updates: [{ id: el.id, patch }] });
  } catch (e) {
    toast(String(e), 'error');
  }
}

export async function replaceVideo(el) {
  const path = await dialog.open({ title: t('Choose Video'), multiple: false, filters: [{ name: t('Videos'), extensions: VIDEO_EXTS }] });
  if (!path) return;
  try {
    const a = await invoke('asset_import_file', { path });
    await apply({ op: 'updateElements', slide: state.slideId, updates: [{ id: el.id, patch: { content: { src: a.name } } }] });
  } catch (e) {
    toast(String(e), 'error');
  }
}

/** Picks an image for the slide background. */
export async function chooseBackgroundImage(slideIds) {
  const path = await dialog.open({ title: t('Background Image'), multiple: false, filters: [{ name: t('Images'), extensions: IMAGE_EXTS }] });
  if (!path) return;
  try {
    const a = await invoke('asset_import_file', { path });
    await updateSlides(slideIds, { background: { kind: 'image', image: a.name } });
  } catch (e) {
    toast(String(e), 'error');
  }
}

export async function chooseBackgroundVideo(slideIds) {
  const path = await dialog.open({ title: t('Background Video'), multiple: false, filters: [{ name: t('Videos'), extensions: VIDEO_EXTS }] });
  if (!path) return;
  try {
    const a = await invoke('asset_import_file', { path });
    await updateSlides(slideIds, { background: { kind: 'video', video: a.name } });
  } catch (e) {
    toast(String(e), 'error');
  }
}

// ---------------------------------------------------------------------------
// Element editing
// ---------------------------------------------------------------------------

export async function deleteSelection() {
  if (!state.selection.length) return;
  const ids = [...state.selection];
  set({ selection: [], editing: null, editingTable: null });
  await apply({ op: 'deleteElements', slide: state.slideId, ids });
}

export async function duplicateSelection() {
  const els = selectedElements();
  if (!els.length) return;
  await apply({ op: 'insertElements', slide: state.slideId, elements: els, offset: 24 });
}

export const arrange = (action) =>
  state.selection.length && apply({ op: 'arrangeElements', slide: state.slideId, ids: state.selection, action });

export const align = (mode, toSlide = false) =>
  state.selection.length && apply({ op: 'alignElements', slide: state.slideId, ids: state.selection, mode, toSlide });

export function toggleLock() {
  const els = selectedElements();
  if (!els.length) return;
  const lock = !els.every((e) => e.locked);
  updateSelected({ locked: lock });
}

export function selectAll() {
  const s = currentSlide();
  if (s) set({ selection: s.elements.map((e) => e.id), focus: 'canvas' });
}

export function nudge(dx, dy) {
  const els = selectedElements().filter((e) => !e.locked);
  if (!els.length) return;
  apply(
    { op: 'updateElements', slide: state.slideId, updates: els.map((e) => ({ id: e.id, patch: { x: e.x + dx, y: e.y + dy } })) },
    { coalesce: 'nudge:' + els.map((e) => e.id).join(',') },
  );
}

/** Makes an image element match its picture's proportions. */
export function resetImageSize(el) {
  const c = el.content;
  if (!c.naturalWidth || !c.naturalHeight) return;
  const h = Math.round((el.w * c.naturalHeight) / c.naturalWidth);
  apply({ op: 'updateElements', slide: state.slideId, updates: [{ id: el.id, patch: { h } }] });
}

// ---------------------------------------------------------------------------
// Slides
// ---------------------------------------------------------------------------

export async function addSlide(layout = null) {
  if (!state.doc) return;
  if (!(await flushEdits())) return;
  const cur = currentSlide();
  const l = layout || (cur && cur.layout === 'title' ? 'title-content' : cur?.layout || 'title-content');
  await apply({ op: 'addSlide', after: state.slideId, layout: l });
}

export async function duplicateSlides(ids = state.selectedSlides) {
  if (!ids.length) return;
  if (!(await flushEdits())) return;
  await apply({ op: 'duplicateSlides', ids });
}

export async function deleteSlides(ids = state.selectedSlides) {
  if (!ids.length) return;
  if (!(await flushEdits())) return;
  set({ selection: [], editing: null });
  await apply({ op: 'deleteSlides', ids });
}

export function toggleSkip(ids = state.selectedSlides) {
  const slides = deck().slides.filter((s) => ids.includes(s.id));
  if (!slides.length) return;
  const hide = !slides.every((s) => s.hidden);
  updateSlides(ids, { hidden: hide });
}

export function setNested(nested, ids = state.selectedSlides) {
  const first = deck().slides[0]?.id;
  const target = ids.filter((id) => !(nested && id === first));
  if (target.length) updateSlides(target, { nested });
}

export async function moveSlides(ids, to) {
  await apply({ op: 'moveSlides', ids, to }, { navigate: false });
}

// ---------------------------------------------------------------------------
// Clipboard
// ---------------------------------------------------------------------------

let pasteCount = 0;
let lastCopySlide = null;
let copyChain = Promise.resolve();
let copying = 0;

/** Handles the DOM `copy`/`cut` events outside text fields. */
export function onCopy(e, cut = false) {
  const slides = state.focus === 'slides';
  const ids = [...(slides ? state.selectedSlides : state.selection)];
  if (!state.doc || !ids.length) return;
  const docId = state.doc.docId;
  const slide = state.slideId;
  // The backend snapshots and writes the clipboard under one lock shared by
  // all windows. Never cut until that complete operation succeeds.
  e.preventDefault();
  copying++;
  const copy = async () => {
    try {
      if (!(await flushEdits())) return;
      if (state.doc?.docId !== docId) return;
      await (slides
        ? copySlides(deck().slides.filter((s) => ids.includes(s.id)))
        : copyElements((deck().slides.find((s) => s.id === slide)?.elements || []).filter((el) => ids.includes(el.id))));
      lastCopySlide = slide;
      pasteCount = 0;
      if (cut && state.doc?.docId === docId) {
        await apply(slides ? { op: 'deleteSlides', ids } : { op: 'deleteElements', slide, ids });
      }
    } catch (err) {
      toast(String(err), 'error');
    } finally {
      copying--;
    }
  };
  copyChain = copyChain.then(copy, copy);
  return copyChain;
}

/** Handles the DOM `paste` event outside text fields. */
export async function onPaste(e) {
  if (!state.doc) return;
  const docId = state.doc.docId;
  const target = state.slideId;
  const dt = e.clipboardData;
  e.preventDefault();
  const files = [...(dt?.files || [])];
  let text = dt?.getData('text/plain') || '';
  if (copying) {
    await copyChain;
    try {
      text = await clipboard.readText();
    } catch {
      text = null;
    }
  }
  if (state.doc?.docId === docId) await pasteData(files, text, target, docId);
}

/** "Paste" chosen from a menu: webviews block execCommand('paste'). */
export async function pasteFromMenu() {
  const docId = state.doc?.docId;
  const target = state.slideId;
  await copyChain;
  let text = null;
  try {
    text = await clipboard.readText();
  } catch {
    text = null;
  }
  if (docId && state.doc?.docId === docId) await pasteData([], text, target, docId);
}

async function pasteData(files, text, target, docId) {
  // `text === null` means the system clipboard is unreadable: use ours.
  const clip = readClip(text);
  if (clip?.type === 'elements') {
    pasteCount = lastCopySlide === target ? pasteCount + 1 : 0;
    await pasteClip(clip, target, 24 * pasteCount);
    return;
  }
  if (clip?.type === 'slides') {
    await pasteClip(clip, target);
    return;
  }
  const images = files.filter((f) => f.type.startsWith('image/') || f.type.startsWith('video/'));
  if (images.length) {
    let i = 0;
    for (const f of images) {
      try {
        const ext = (f.name && fileExt(f.name)) || f.type.split('/')[1] || '';
        const bytes = new Uint8Array(await f.arrayBuffer());
        if (state.doc?.docId !== docId) return;
        const a = await importBytes(bytes, ext, docId);
        if (state.doc?.docId !== docId) return;
        await insertAsset(a, null, 24 * i++, {}, { docId, slideId: target });
      } catch (err) {
        toast(String(err), 'error');
      }
    }
    return;
  }
  if (text && text.trim()) {
    const txt = text.trim();
    if (/^https?:\/\/\S+$/i.test(txt) && /youtube\.com|youtu\.be|vimeo\.com/i.test(txt)) {
      await insert('video', '', { name: N_('Video'), content: { kind: 'video', src: txt, autoplay: false } }, { docId, slideId: target });
      return;
    }
    await insert('text', 'body', { content: { kind: 'text', html: textToHtml(txt) } }, { docId, slideId: target });
  }
}

// ---------------------------------------------------------------------------
// Import & export
// ---------------------------------------------------------------------------

export async function importMarkdown() {
  const path = await dialog.open({ title: t('Import Markdown'), multiple: false, filters: [{ name: t('Markdown'), extensions: ['md', 'markdown', 'txt'] }] });
  if (!path) return;
  const d = deck();
  const onlyEmpty = d.slides.length === 1 && d.slides[0].elements.every((e) => !e.content.html);
  let replace = onlyEmpty;
  if (!onlyEmpty) {
    const choice = await ask('choice', {
      title: t('Import Markdown'),
      message: t('Add the imported slides after the existing ones, or replace the whole presentation?'),
      choices: [
        { value: 'append', label: t('Add Slides'), primary: true },
        { value: 'replace', label: t('Replace All') },
      ],
    });
    if (!choice) return;
    replace = choice === 'replace';
  }
  try {
    if (!(await flushEdits())) return;
    ingestView(await invoke('import_markdown', { path, replace }));
    toast(t('Imported {name}', { name: fileName(path) }));
  } catch (e) {
    toast(String(e), 'error');
  }
}

function safeTitle() {
  return (deckTitle() || t('Presentation')).replace(/[\\/:*?"<>|]+/g, '-').trim() || t('Presentation');
}

export async function exportHtml(single) {
  if (!state.doc) return;
  if (!(await flushEdits())) return;
  const path = single
    ? await dialog.save({ title: t('Export as Single HTML File'), defaultPath: `${safeTitle()}.html`, filters: [{ name: t('Web Page'), extensions: ['html'] }] })
    : await dialog.save({ title: t('Export as Website Folder (name the new folder)'), defaultPath: safeTitle() });
  if (!path) return;
  try {
    set({ busy: t('Exporting…') });
    const out = await invoke('export_html', { path, single });
    toast(single ? t('Exported HTML file') : t('Exported website folder'), 'success', {
      action: { label: t('Show in Folder'), run: () => opener.revealItemInDir(out) },
      duration: 7000,
    });
  } catch (e) {
    toast(String(e), 'error');
  } finally {
    set({ busy: null });
  }
}

export async function exportPdf() {
  if (!state.doc) return;
  if (!(await flushEdits())) return;
  try {
    await invoke('export_pdf');
  } catch (e) {
    toast(String(e), 'error');
  }
}

// ---------------------------------------------------------------------------
// Misc
// ---------------------------------------------------------------------------

/** Selection bounds in slide coordinates (for menus and toolbars). */
export function selectionBounds() {
  return unionBounds(selectedElements().map(bounds));
}

export function goToNextSlideWithSelection(delta) {
  const slides = deck()?.slides || [];
  const i = slides.findIndex((s) => s.id === state.slideId);
  const n = slides[i + delta];
  if (n) goToSlide(n.id);
}

// ---------------------------------------------------------------------------
// Build steps
// ---------------------------------------------------------------------------

/** Steps a chart takes after its build-in when it builds by series (same rule as `Element::series_steps`). */
export function seriesSteps(el) {
  if (!el.build.effect || el.build.delivery !== 'series' || el.content.kind !== 'chart') return 0;
  return chartSteps(el.content);
}

export const hasPath = (el) => (el.build.path?.length || 0) >= 2;

/** The slide's builds in play order: each element's build-in and its motion path. */
export function buildEvents(slide) {
  return slide.elements
    .flatMap((el) => [el.build.effect && { el, kind: 'in', step: el.build.order }, hasPath(el) && { el, kind: 'path', step: el.build.pathOrder }].filter(Boolean))
    .sort((a, b) => (a.step || 999) - (b.step || 999) || (a.kind === 'in' ? -1 : 1));
}

/** The first step after every build on the slide. */
export function nextBuildStep(slide) {
  const last = (el) => Math.max(el.build.order ? el.build.order + seriesSteps(el) : 0, el.build.outOrder || 0, el.build.pathOrder || 0);
  return Math.max(0, ...slide.elements.map(last)) + 1;
}

/** Numbers the slide's builds one step after another, in `events` order. */
export function setBuildOrder(slide, events) {
  return apply({ op: 'setBuildOrder', slide: slide.id, ids: events.map((ev) => (ev.kind === 'path' ? `${ev.el.id}#path` : ev.el.id)) });
}
