// Fast failure-path tests complement tools/test_print.cjs's real PDF loop.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const code = fs.readFileSync(path.join(__dirname, '../ui/js/print-media.js'), 'utf8');
function events(object = {}) {
  const listeners = new Map();
  return Object.assign(object, {
    listeners,
    addEventListener(type, fn, options) { const entries = listeners.get(type) || []; entries.push({ fn, once: options?.once }); listeners.set(type, entries); },
    removeEventListener(type, fn) { listeners.set(type, (listeners.get(type) || []).filter(entry => entry.fn !== fn)); },
    dispatch(type) { for (const entry of [...(listeners.get(type) || [])]) { if (entry.once) this.removeEventListener(type, entry.fn); entry.fn(); } },
    on(type, fn) { this.addEventListener(type, fn); }, off(type, fn) { this.removeEventListener(type, fn); },
    count() { return [...listeners.values()].reduce((n, list) => n + list.length, 0); },
  });
}
function video(overrides = {}) {
  return events({ readyState: 0, videoWidth: 640, videoHeight: 360, duration: 2, className: 'authored-video', dataset: { src: 'movie.webm' },
    getAttribute() { return ''; }, load() { this.loaded = true; }, pause() { this.paused = true; }, replaceWith(node) { this.replacement = node; }, ...overrides });
}
function harness(videos = [], frames = [], options = {}) {
  let sequence = 0; const timers = new Map(), canvases = [], images = [];
  const window = {};
  const setTimeout = (fn, duration) => { timers.set(++sequence, { fn, duration }); return sequence; };
  const document = {
    querySelectorAll(selector) { return selector.endsWith('video') ? videos : selector.endsWith(' img') ? options.cropImages || [] : frames; },
    createElement(tag) {
      assert.equal(tag, 'canvas');
      const node = { style: { setProperty(key, value) { this[key] = value; } }, attributes: {}, setAttribute(key, value) { this.attributes[key] = value; }, getContext() { return { ...(options.noFilter ? {} : { filter: 'none' }), scale(x,y) { node.scale = [x,y]; }, beginPath() {}, rect(...box) { node.clip = box; }, clip() {}, drawImage(source, ...args) { if (options.drawError) throw Error('unreadable frame'); node.source = source; node.draw = args; } }; } };
      canvases.push(node); return node;
    },
  };
  class Image { constructor() { images.push(this); this.style = {}; } remove() { this.removed = true; } }
  const css = { position: 'static', top: 'auto', left: 'auto', width: '768px', height: '432px', 'object-fit': 'contain', 'object-position': '50% 50%', 'max-width': 'none', 'max-height': 'none' };
  vm.runInNewContext(code, { window, document, Image, getComputedStyle: () => ({ filter: options.filter || 'none', opacity: '1', getPropertyValue: key => css[key] || '' }), setTimeout, clearTimeout: id => timers.delete(id) });
  return { print: window.CadenzaPrint, timers, canvases, images, fireTimers() { for (const [id, timer] of [...timers]) { timers.delete(id); timer.fn(); } } };
}
function frame(url) { return { getAttribute(name) { return name === 'data-src' ? url : ''; }, replaceWith(node) { this.replacement = node; } }; }

test('print layout waits for pdf-ready and removes its subscription', async () => {
  const h = harness(), deck = events(); let ready = false;
  const promise = h.print.layoutReady(deck).then(() => { ready = true; });
  await Promise.resolve(); assert.equal(ready, false); assert.equal(deck.count(), 1);
  deck.dispatch('ready'); await Promise.resolve(); assert.equal(ready, false);
  deck.dispatch('pdf-ready'); await promise;
  assert.equal(ready, true); assert.equal(deck.count(), 0); assert.equal(h.timers.size, 0);
});
test('a failed pagination has a deadline and cleans up its listener', async () => {
  const h = harness(), deck = events();
  const ready = h.print.layoutReady(deck);
  assert.equal(h.timers.size, 1);
  const rejection = assert.rejects(ready, /printing/);
  h.fireTimers(); await rejection; assert.equal(deck.count(), 0);
});
test('a decoded frame preserves CSS sizing separately from bitmap resolution', async () => {
  const v = video(), h = harness([v]);
  const ready = h.print.prepareMedia();
  assert(v.loaded); assert.equal(v.src, 'movie.webm'); assert.equal(v.preload, 'auto');
  v.dispatch('loadedmetadata'); assert.equal(v.currentTime, .1);
  v.dispatch('seeked'); await ready;
  const c = v.replacement; assert(c); assert.equal(c.width, 640); assert.equal(c.height, 360);
  assert.equal(c.style.width, '768px'); assert.equal(c.style.height, '432px'); assert.equal(c.style['object-fit'], 'contain');
  assert.equal(c.className, v.className); assert.equal(c.source, v); assert(v.paused);
  assert.equal(v.count(), 0); assert.equal(h.timers.size, 0);
});
for (const outcome of ['timeout', 'error', 'draw error', 'seek error', 'load error']) {
  test(`${outcome} preserves the video and forbids late mutation after printing becomes ready`, async () => {
    const v = video(); if (outcome === 'seek error') Object.defineProperty(v, 'currentTime', { set() { throw Error('seek failed'); } });
    if (outcome === 'load error') v.load = () => { throw Error('load failed'); };
    const h = harness([v], [], { drawError: outcome === 'draw error' });
    const ready = h.print.prepareMedia();
    if (outcome === 'timeout') h.fireTimers();
    else if (outcome === 'error') v.dispatch('error');
    else if (outcome !== 'load error') { v.dispatch('loadedmetadata'); if (outcome === 'draw error') v.dispatch('seeked'); }
    await ready; assert.equal(v.replacement, undefined); assert.equal(v.count(), 0);
    v.dispatch('loadedmetadata'); v.dispatch('seeked'); assert.equal(v.replacement, undefined);
    assert.equal(h.timers.size, 0);
  });
}
test('multiple videos finish independently before printing becomes ready', async () => {
  const a = video({ readyState: 1 }), b = video({ readyState: 1 }), h = harness([a, b]); let ready = false;
  const promise = h.print.prepareMedia().then(() => { ready = true; });
  a.dispatch('seeked'); await Promise.resolve(); assert.equal(ready, false);
  b.dispatch('seeked'); await promise; assert(a.replacement && b.replacement);
});
test('YouTube frames use a thumbnail; Vimeo and unrelated frames are preserved', async () => {
  const youtube = frame('https://www.youtube.com/embed/abc_123?controls=0'), vimeo = frame('https://player.vimeo.com/video/12345');
  const h = harness([], [youtube, vimeo]); const ready = h.print.prepareMedia();
  assert.equal(h.images.length, 1); const img = youtube.replacement;
  assert.equal(img.src, 'https://img.youtube.com/vi/abc_123/hqdefault.jpg'); assert(img.style.cssText.includes('width:100%;height:100%;object-fit:cover'));
  img.onload(); await ready; assert.equal(vimeo.replacement, undefined); assert.equal(h.timers.size, 0);
});
for (const outcome of ['error', 'timeout']) {
  test(`an unavailable YouTube thumbnail (${outcome}) does not stall printing`, async () => {
    const h = harness([], [frame('https://www.youtube.com/embed/abc_123')]); const ready = h.print.prepareMedia();
    if (outcome === 'error') h.images[0].onerror(); else h.fireTimers();
    await ready; assert.equal(h.timers.size, 0);
  });
}

function cropImage(overrides = {}) {
  const viewport = { clientWidth: 672, clientHeight: 504, style: { transform: 'scale(-1,1)' }, replaceChildren(node) { this.replacement = node; } };
  const region = { offsetLeft: 0, offsetTop: -252, clientWidth: 672, clientHeight: 1008 };
  const img = events({ src: 'local-asset.svg', complete: true, naturalWidth: 640, naturalHeight: 480, offsetWidth: 1344, offsetHeight: 1008,
    offsetLeft: 0, offsetTop: 0, parentElement: region, closest: () => viewport, alt: 'Selected image', className: 'picture', ...overrides });
  return { img, viewport, region };
}
test('SVG print captures bake both crop clips and leave the outer flip intact', async () => {
  const { img, viewport } = cropImage(); const h = harness([], [], { cropImages: [img] });
  await h.print.prepareMedia(); const c = viewport.replacement;
  assert(c); assert.equal(c.width, 1344); assert.equal(c.height, 1008);
  assert.deepEqual(c.clip, [0, -252, 672, 1008]); assert.deepEqual(c.draw, [0, -252, 1344, 1008]);
  assert.equal(c.style.opacity, '1'); assert.equal(c.attributes['aria-label'], img.alt);
  assert.equal(viewport.style.transform, 'scale(-1,1)'); assert.equal(img.count(), 0); assert.equal(h.timers.size, 0);
});
test('cropped SVG print raster size is bounded even for enormous authored frames', async () => {
  const { img, viewport } = cropImage(); viewport.clientWidth = 100000; viewport.clientHeight = 100;
  const h = harness([], [], { cropImages: [img] }); await h.print.prepareMedia();
  const c = viewport.replacement; assert(c.width <= 8192 && c.height <= 8192); assert(c.width * c.height <= 16777216);
});
for (const outcome of ['timeout', 'error', 'draw error', 'unavailable filters']) {
  test(`SVG crop ${outcome} keeps its image and ignores late load events`, async () => {
    const { img, viewport } = cropImage({ complete: outcome === 'draw error' || outcome === 'unavailable filters' });
    const h = harness([], [], { cropImages: [img], drawError: outcome === 'draw error', noFilter: outcome === 'unavailable filters', filter: 'blur(4px)' });
    const ready = h.print.prepareMedia();
    if (outcome === 'timeout') h.fireTimers(); else if (outcome === 'error') img.dispatch('error');
    await ready; assert.equal(viewport.replacement, undefined); assert.equal(img.count(), 0);
    img.dispatch('load'); assert.equal(viewport.replacement, undefined); assert.equal(h.timers.size, 0);
  });
}
test('raster and data SVG crops share the same faithful print preparation', async () => {
  const png = cropImage({ src: 'picture.png' }), svg = cropImage({ src: 'data:image/svg+xml;base64,PHN2Zy8+' });
  const h = harness([], [], { cropImages: [png.img, svg.img] }); await h.print.prepareMedia();
  assert(png.viewport.replacement); assert(svg.viewport.replacement);
});

test('a deck with many large crops also respects a document-wide raster memory budget', async () => {
  const images = Array.from({ length: 8 }, () => { const c = cropImage(); c.viewport.clientWidth = c.viewport.clientHeight = 4096; return c.img; });
  const h = harness([], [], { cropImages: images }); await h.print.prepareMedia();
  assert(h.canvases.reduce((n,c) => n + c.width * c.height, 0) <= 67108864);
});
