// A real, isolated native WebView paints the same HTML/CSS as the editor.
// No presenter runtime is started: cameras, players and remote embeds stay inert.
import { postProcess } from './slideview.js';
import { inertPreview } from './transition-preview.js';
import { N_ } from './i18n.js';

const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);
async function bounded(promise, milliseconds, message) {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(message)), milliseconds); })]); }
  finally { clearTimeout(timer); }
}

async function run() {
  const data = await invoke('pptx_capture_data');
  const theme = document.getElementById('cz-theme');
  const themeReady = data.themeCss ? new Promise((resolve, reject) => {
    theme.onload = resolve; theme.onerror = () => reject(new Error(N_('Could not load the presentation theme')));
    theme.href = data.themeCss;
  }) : Promise.resolve();
  const code = document.getElementById('cz-code');
  const codeHref = `vendor/reveal/plugin/highlight/${data.codeTheme === 'zenburn' ? 'zenburn' : 'monokai'}.css`;
  const codeReady = code.sheet && code.href === new URL(codeHref, document.baseURI).href ? Promise.resolve() : new Promise((resolve, reject) => {
    code.onload = resolve; code.onerror = () => reject(new Error(N_('Could not load code colors')));
    code.href = codeHref;
  });
  document.getElementById('cz-deck-css').textContent = data.deckCss;
  const template = document.createElement('template');
  template.innerHTML = data.html;
  inertPreview(template.content, 'Media');
  const section = document.querySelector('section');
  section.className = `cz-slide present ${data.classes || ''}`;
  section.append(template.content);
  const stage = document.querySelector('.slides');
  stage.style.width = section.style.width = `${data.width}px`;
  stage.style.height = section.style.height = `${data.height}px`;
  let target;
  if (data.background) {
    for (const node of section.querySelectorAll('.cz-el')) node.style.visibility = 'hidden';
  }
  if (data.elementId) {
    document.documentElement.classList.add('isolated');
    for (const node of section.children) {
      if (node.dataset.czId === data.elementId) target = node;
      else node.style.visibility = 'hidden';
    }
    if (!target) throw new Error(N_('The export object was not rendered'));
  }
  postProcess(section);
  await bounded(Promise.all([themeReady, codeReady]), 10000, N_('The presentation theme did not finish loading'));
  const viewport = document.getElementById('viewport');
  const backgrounds = new Set();
  for (const node of [viewport, section, ...section.querySelectorAll('*')]) {
    for (const match of getComputedStyle(node).backgroundImage.matchAll(/url\(["']?([^"')]+)["']?\)/g)) backgrounds.add(match[1]);
  }
  await bounded(Promise.all([...backgrounds].map((url) => { const image = new Image(); image.src = url; return image.decode(); })), 12000, N_('A background image could not be loaded for export'));
  await bounded(Promise.all([...section.querySelectorAll('img[src]')].map((img) => img.decode())), 12000, N_('An image could not be loaded for export'));
  await bounded(document.fonts.ready, 12000, N_('Fonts could not be loaded for export'));
  const scale = Math.min(innerWidth / data.width, innerHeight / data.height);
  stage.style.transform = `scale(${scale})`;
  viewport.style.width = `${data.width * scale}px`;
  viewport.style.height = `${data.height * scale}px`;
  // Painting of a realized WebView follows layout asynchronously on both GTK
  // and WebView2. A short settling interval precedes the native capture.
  await new Promise((resolve) => setTimeout(resolve, 100));
  let bounds = { x: 0, y: 0, w: data.width * scale, h: data.height * scale };
  if (target) {
    // Rotation and content scaling live on descendants, not the element
    // wrapper. Include their painted geometry before cropping the native PNG.
    const nodes = [target, ...target.querySelectorAll('*')];
    const rectangles = nodes.map((node) => node.getBoundingClientRect()).filter((r) => r.width > 0 && r.height > 0);
    const rect = rectangles.reduce((area, r) => ({ left: Math.min(area.left, r.left), top: Math.min(area.top, r.top),
      right: Math.max(area.right, r.right), bottom: Math.max(area.bottom, r.bottom) }), { left: Infinity, top: Infinity, right: -Infinity, bottom: -Infinity });
    const shadow = nodes.some((node) => { const css = getComputedStyle(node); return css.boxShadow !== 'none' || css.textShadow !== 'none' || css.filter !== 'none'; });
    // The largest built-in shadow has a 36px blur plus 14px offset. 96px
    // covers its falloff; arbitrary custom CSS is captured as a whole slide.
    const pad = (shadow ? 96 : 24) * scale;
    const x = Math.max(0, Math.floor(rect.left - pad)), y = Math.max(0, Math.floor(rect.top - pad));
    bounds = { x, y, w: Math.min(data.width * scale, Math.ceil(rect.right + pad)) - x,
      h: Math.min(data.height * scale, Math.ceil(rect.bottom + pad)) - y };
  }
  await invoke('pptx_capture_ready', { bounds: { ...bounds, scale, viewportWidth: innerWidth } });
}
run().catch((error) => invoke('pptx_capture_ready', {
  bounds: { x: 0, y: 0, w: 1, h: 1, scale: 1, viewportWidth: 1 }, error: error?.message || String(error),
}).catch(() => {}));
