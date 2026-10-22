// Entry point: boot, global shortcuts, clipboard, file drops, window close.

import { html, render } from './lib.js';
import { App } from './app.js';
import { invoke, currentWindow, currentWebview } from './api.js';
import { state, set, boot, toast, confirmDiscard, openPath, stepSlide } from './store.js';
import { commandForKey } from './commands.js';
import * as A from './actions.js';
import { canvasApi, finishPath, cancelPath } from './canvas.js';
import { beginTextEdit, endAllEditing } from './textedit.js';
import { combo, isTyping, isControl, fileExt } from './util.js';
import { IMAGE_EXTS, VIDEO_EXTS } from './api.js';
import { subscribe } from './store.js';
import { ensureDiagrams } from './diagrams.js';
import { insertPicked, checkClipboardLink, openInSystemBrowser } from './media.js';
import { t } from './i18n.js';

// Events Rust sends to this window. (A global listener would also get the
// ones meant for other windows.)
const listen = (name, handler) => currentWindow().listen(name, handler).catch(() => {});

// Media chosen in a provider's website (Browse mode).
listen('cz-media-picked', (e) => insertPicked(e.payload));
listen('cz-media-browser-closed', (e) => {
  const name = { youtube: 'YouTube', vimeo: 'Vimeo', giphy: 'GIPHY' }[e.payload?.provider];
  const message = name
    ? t('{name} closed unexpectedly (a problem in the system web engine). Your presentation is safe.', { name })
    : t('The website closed unexpectedly (a problem in the system web engine). Your presentation is safe.');
  toast(message, 'error', {
    duration: 12000,
    action: { label: t('Use My Browser'), run: () => openInSystemBrowser(e.payload?.provider) },
  });
});
window.addEventListener('focus', () => setTimeout(checkClipboardLink, 200));

// The window itself never scrolls (its panels do): a focused control that
// sits outside a scrolled panel would otherwise move the whole interface.
window.addEventListener('scroll', () => {
  if (window.scrollX || window.scrollY) window.scrollTo(0, 0);
});

// Report unexpected errors to the terminal.
const report = (level, msg) => invoke('ui_log', { level, message: String(msg).slice(0, 4000) }).catch(() => {});
window.addEventListener('error', (e) => report('error', `${e.message} @ ${e.filename}:${e.lineno}:${e.colno}`));
window.addEventListener('unhandledrejection', (e) => report('error', `unhandled rejection: ${e.reason?.stack || e.reason}`));

render(html`<${App} />`, document.getElementById('app'));

boot()
  // A moment after starting, not to compete with opening the window.
  .then(() => setTimeout(() => import('./updates.js').then((m) => m.checkForUpdates()), 1500))
  .catch((e) => {
    console.error(e);
    document.getElementById('app').innerHTML = `<pre style="padding:2em;color:#f87171">${t('Cadenza failed to start:')}\n${t(String(e))}</pre>`;
  });

// Render diagrams that have no stored picture yet (new or imported ones).
let lastVersion = 0;
let diagramTimer = null;
subscribe((st) => {
  const v = st.doc?.version || 0;
  if (v === lastVersion) return;
  lastVersion = v;
  clearTimeout(diagramTimer);
  diagramTimer = setTimeout(ensureDiagrams, 250);
});

// ---------------------------------------------------------------------------
// Keyboard
// ---------------------------------------------------------------------------

// Browser behaviors that make no sense in a desktop app.
const SUPPRESS = new Set(['Mod+R', 'Mod+Shift+R', 'F5', 'Shift+F5', 'Mod+P', 'Mod+F', 'Mod+G', 'F3', 'F7', 'Mod+U', 'Mod+=', 'Mod+-', 'Mod+0', 'Mod+S', 'Mod+O', 'Mod+N', 'Mod+W']);

window.addEventListener('keydown', (e) => {
  const c = combo(e);
  if (SUPPRESS.has(c)) e.preventDefault();
  if (state.modal || state.menu || state.shapePicker) return;
  // Drawing a motion path: Enter keeps it, Esc drops it.
  if (state.drawPath && (e.key === 'Enter' || e.key === 'Escape')) {
    e.preventDefault();
    if (e.key === 'Enter') finishPath();
    else cancelPath();
    return;
  }
  const typing = isTyping(e.target);
  // A control of the interface (a button, toggle, slider…) has the keyboard:
  // Tab moves on to the next one rather than through the slide's objects.
  const control = isControl(e.target);

  // Clipboard: route through a hidden editable so copy/paste events fire
  // even when nothing editable has focus (WebKitGTK needs this).
  if (!typing && state.doc && (c === 'Mod+C' || c === 'Mod+X' || c === 'Mod+V')) {
    armClipboardCatcher(c);
    return;
  }

  const cmd = commandForKey(c);
  if (cmd) {
    if (typing && !cmd.global) return;
    e.preventDefault();
    if (!cmd.when || cmd.when()) cmd.run();
    return;
  }
  if (typing || !state.doc) return;

  const arrows = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
  const base = e.key in arrows ? e.key : null;
  // Sliders and radio buttons move with the arrow keys themselves.
  if (base && e.target.matches?.('input[type=range], input[type=radio]')) return;
  if (base && !e.ctrlKey && !e.metaKey && !e.altKey) {
    e.preventDefault();
    if (state.selection.length && state.focus === 'canvas') {
      const [dx, dy] = arrows[base];
      const step = e.shiftKey ? 10 : 1;
      A.nudge(dx * step, dy * step);
    } else {
      stepSlide(base === 'ArrowLeft' || base === 'ArrowUp' ? -1 : 1);
    }
    return;
  }
  if (e.key === 'Enter' && state.selection.length === 1 && canvasApi.view) {
    const el = state.doc.deck.slides.find((s) => s.id === state.slideId)?.elements.find((x) => x.id === state.selection[0]);
    if (el && (el.content.kind === 'text' || (el.content.kind === 'shape' && el.content.shape !== 'line'))) {
      e.preventDefault();
      beginTextEdit(canvasApi.view, el, null, { selectAll: true });
    }
    return;
  }
  if (e.key === 'Tab' && state.focus === 'canvas' && !control) {
    const slide = state.doc.deck.slides.find((s) => s.id === state.slideId);
    if (!slide?.elements.length) return;
    e.preventDefault();
    const i = slide.elements.findIndex((x) => x.id === state.selection[0]);
    const n = slide.elements.length;
    const next = slide.elements[(i + (e.shiftKey ? -1 : 1) + n) % n];
    set({ selection: [next.id] });
  }
});

// ---------------------------------------------------------------------------
// Clipboard
// ---------------------------------------------------------------------------

const catcher = document.createElement('div');
catcher.contentEditable = 'true';
catcher.setAttribute('aria-hidden', 'true');
catcher.className = 'clipboard-catcher';
document.body.appendChild(catcher);
let restoreFocus = null;
let handled = false;

function armClipboardCatcher(c) {
  restoreFocus = document.activeElement;
  handled = false;
  catcher.textContent = '​';
  catcher.focus({ preventScroll: true });
  const r = document.createRange();
  r.selectNodeContents(catcher);
  const sel = window.getSelection();
  sel.removeAllRanges();
  sel.addRange(r);
  // If the webview did not fire the event, fall back to the app clipboard.
  setTimeout(() => {
    if (!handled && c === 'Mod+V') A.pasteFromMenu();
    disarm();
  }, 60);
}

function disarm() {
  catcher.textContent = '';
  if (document.activeElement === catcher) {
    catcher.blur();
    if (restoreFocus && restoreFocus !== document.body && document.contains(restoreFocus)) restoreFocus.focus?.({ preventScroll: true });
  }
}

const fromCatcher = (e) => e.target === catcher || catcher.contains(e.target);

document.addEventListener('copy', (e) => {
  if (!state.doc || (!fromCatcher(e) && isTyping(e.target))) return;
  handled = true;
  // With nothing selected, the clipboard keeps what it has (instead of the
  // catcher's invisible placeholder).
  if (fromCatcher(e)) e.preventDefault();
  A.onCopy(e, false);
});
document.addEventListener('cut', (e) => {
  if (!state.doc || (!fromCatcher(e) && isTyping(e.target))) return;
  handled = true;
  if (fromCatcher(e)) e.preventDefault();
  A.onCopy(e, true);
});
document.addEventListener('paste', (e) => {
  if (!state.doc || (!fromCatcher(e) && isTyping(e.target))) return;
  handled = true;
  A.onPaste(e);
  setTimeout(disarm);
});

// No browser context menu except in text fields.
document.addEventListener('contextmenu', (e) => {
  if (!isTyping(e.target)) e.preventDefault();
});

// Links on slides work in the show. Here a click on one selects or edits
// it: following it would put the web page in place of the editor.
for (const type of ['click', 'auxclick']) {
  document.addEventListener(
    type,
    (e) => {
      if (e.target.closest?.('.cz-editor-reveal a[href]')) e.preventDefault();
    },
    true,
  );
}

// ---------------------------------------------------------------------------
// Files dropped from the system
// ---------------------------------------------------------------------------

currentWebview()
  .onDragDropEvent(async (event) => {
    const p = event.payload;
    if (p.type === 'enter' || p.type === 'over') {
      if (!state.dropHint) set({ dropHint: state.doc ? t('Drop images, videos, Markdown or presentations') : t('Drop a presentation to open it') });
      return;
    }
    set({ dropHint: null });
    if (p.type !== 'drop') return;
    const paths = p.paths || [];
    const decks = paths.filter((f) => fileExt(f) === 'cadenza');
    for (const f of decks) await openPath(f);
    if (!state.doc) return;
    const media = paths.filter((f) => [...IMAGE_EXTS, ...VIDEO_EXTS].includes(fileExt(f)));
    const md = paths.filter((f) => ['md', 'markdown'].includes(fileExt(f)));
    if (media.length) {
      const dpr = window.devicePixelRatio || 1;
      const pt = canvasApi.clientToSlide(p.position.x / dpr, p.position.y / dpr);
      await A.importPaths(media, pt?.inside ? pt : null);
    }
    if (md.length) {
      try {
        const { ingestView } = await import('./store.js');
        ingestView(await invoke('import_markdown', { path: md[0], replace: false }));
        toast(t('Imported Markdown slides'));
      } catch (err) {
        toast(String(err), 'error');
      }
    }
    const unknown = paths.length - decks.length - media.length - md.length;
    if (unknown > 0) toast(t('Some files were not recognized (images, videos, .md and .cadenza are supported)'), 'error');
  })
  .catch(() => {});

// ---------------------------------------------------------------------------
// Window close
// ---------------------------------------------------------------------------

let closing = false;
currentWindow()
  .onCloseRequested(async (event) => {
    if (closing) return;
    event.preventDefault();
    if (!(await endAllEditing())) return;
    if (await confirmDiscard()) {
      try {
        await invoke('doc_close');
        closing = true;
        await currentWindow().destroy();
      } catch (e) {
        closing = false;
        toast(String(e), 'error');
      }
    }
  })
  .catch(() => {});
