// Full print regression: production renderer data, present.html, Reveal,
// decoded WebM and Chromium PDF output. No user's browser/profile is touched.
// cargo run --manifest-path src-tauri/Cargo.toml --example print_fixture > /tmp/print-data.json
// node tools/test_print.cjs /tmp/print-data.json [artifact-directory] [chromium-binary]
// Requires Node >= 22, Chromium and Poppler's pdftoppm.
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const http = require('node:http'), cp = require('node:child_process');
const root = path.resolve(__dirname, '..');
const data = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const output = process.argv[3] || fs.mkdtempSync(path.join(os.tmpdir(), 'cadenza-print-result-'));
fs.mkdirSync(output, { recursive: true });
const browser = process.argv[4] || process.env.CHROMIUM || ['chromium', 'google-chrome', 'chromium-browser']
  .map(name => cp.spawnSync('which', [name], { encoding: 'utf8' }).stdout?.trim()).find(Boolean);
assert(browser, 'Chromium is required (pass its path as the fourth argument)');
// Data URLs make metadata available during pagination, as in a single-file
// export. A network-delayed video would hide the detached-slide regression.
data.sections = data.sections.replaceAll('/tools/fixtures/print.webm',
  `data:video/webm;base64,${fs.readFileSync(path.join(__dirname, 'fixtures/print.webm')).toString('base64')}`);
const stub = `<script>window.__TAURI__={core:{invoke:function(c){return Promise.resolve(c==='present_data'?${JSON.stringify(data).replaceAll('</', '<\\/')} :true);}},window:{getCurrentWindow:function(){return{close:function(){}}}}};</script>`;
const fixture = fs.readFileSync(path.join(root, 'ui/present.html'), 'utf8').replace('<head>', `<head><base href="/ui/">${stub}`);
const mime = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.woff2': 'font/woff2' };
const server = http.createServer((request, response) => {
  const name = decodeURIComponent(new URL(request.url, 'http://localhost').pathname);
  try {
    const file = path.resolve(root, `.${name}`);
    assert(file.startsWith(root + path.sep));
    const bytes = name === '/print-fixture.html' ? fixture : fs.readFileSync(file);
    response.writeHead(200, { 'Content-Type': mime[path.extname(file)] || 'application/octet-stream' });
    response.end(bytes);
  } catch { response.writeHead(404); response.end(); }
});
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(check, message) {
  for (let i = 0; i < 100; i++) { const value = await check(); if (value) return value; await delay(100); }
  throw new Error(message);
}

// Parse pdftoppm's binary PPM without image-library dependencies.
function ppm(file) {
  const bytes = fs.readFileSync(file); let offset = 0;
  function token() {
    while (bytes[offset] <= 32) offset++;
    if (bytes[offset] === 35) { while (bytes[offset] !== 10) offset++; return token(); }
    const start = offset; while (bytes[offset] > 32) offset++;
    return bytes.subarray(start, offset).toString();
  }
  assert.equal(token(), 'P6');
  const width = Number(token()), height = Number(token()); assert.equal(token(), '255'); offset++;
  assert.equal(bytes.length - offset, width * height * 3);
  return { width, height, bytes: bytes.subarray(offset) };
}
function pixels(image, matches) {
  let count = 0, x1 = Infinity, y1 = Infinity, x2 = -1, y2 = -1;
  for (let y = 0; y < image.height; y++) for (let x = 0; x < image.width; x++) {
    const i = (y * image.width + x) * 3;
    if (matches(...image.bytes.subarray(i, i + 3))) { count++; x1 = Math.min(x1, x); y1 = Math.min(y1, y); x2 = Math.max(x2, x); y2 = Math.max(y2, y); }
  }
  return { count, box: [x1, y1, x2 + 1, y2 + 1] };
}
function nearBox(actual, expected, label) {
  actual.forEach((value, i) => assert(Math.abs(value - expected[i]) <= 2, `${label}: ${actual}, expected ${expected}`));
}

(async () => {
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  // Snap Chromium cannot access host /tmp profiles. Its common directory is
  // visible on both sides of the sandbox.
  const cache = browser.startsWith('/snap/') ? path.join(os.homedir(), 'snap/chromium/common') : os.tmpdir();
  fs.mkdirSync(cache, { recursive: true });
  const profile = fs.mkdtempSync(path.join(cache, 'cadenza-print-test-'));
  const browserProcess = cp.spawn(browser, ['--headless', '--no-sandbox', '--disable-gpu', '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank'], { stdio: 'ignore' });
  let socket, call;
  try {
    const port = await until(() => { try { return fs.readFileSync(path.join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0]; } catch { return null; } }, 'Chromium did not start');
    const tabs = await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(10000) }).then(response => response.json());
    socket = new WebSocket(tabs[0].webSocketDebuggerUrl);
    await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }); });
    let sequence = 0; const pending = new Map();
    socket.addEventListener('message', event => {
      const message = JSON.parse(event.data); if (!message.id) return;
      const handlers = pending.get(message.id); if (!handlers) return;
      pending.delete(message.id); clearTimeout(handlers.timer); message.error ? handlers.reject(new Error(JSON.stringify(message.error))) : handlers.resolve(message.result);
    });
    socket.addEventListener('close', () => { for (const task of pending.values()) { clearTimeout(task.timer); task.reject(new Error('Chromium connection closed')); } pending.clear(); });
    call = (method, params = {}) => new Promise((resolve, reject) => {
      const id = ++sequence;
      const timer = setTimeout(() => { pending.delete(id); reject(new Error(`${method} timed out`)); }, 15000);
      pending.set(id, { resolve, reject, timer }); socket.send(JSON.stringify({ id, method, params }));
    });
    const evaluate = async expression => {
      const result = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
      assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails)); return result.result.value;
    };
    await call('Network.setUserAgentOverride', { userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/157.0 Safari/537.36' });
    await call('Page.navigate', { url: `http://127.0.0.1:${server.address().port}/print-fixture.html?print-pdf` });
    await until(() => evaluate("!!document.querySelector('.cz-print-bar') && document.querySelectorAll('.pdf-page').length === 15"), 'Print preparation did not finish');
    const geometry = await evaluate(`(() => { const canvas = document.querySelector('.cz-video-box canvas'); const box = document.querySelector('.cz-video-box'); return { classes: document.documentElement.className, cropIsolation: [...document.querySelectorAll('.cz-crop-viewport')].map(v => getComputedStyle(v).isolation), video: canvas && canvas.getBoundingClientRect().toJSON(), box: box.getBoundingClientRect().toJSON(), intrinsic: canvas && [canvas.width,canvas.height], style: canvas && canvas.getAttribute('style') }; })()`);
    fs.writeFileSync(path.join(output, 'geometry.json'), JSON.stringify(geometry, null, 2));
    assert(geometry.video, 'Actual video decoding must produce a still canvas');
    assert.deepEqual(geometry.intrinsic, [640, 360]);
    for (const key of ['x', 'y', 'width', 'height']) assert.equal(geometry.video[key], geometry.box[key], `Video ${key} must match its authored box`);
    const pdf = await call('Page.printToPDF', { printBackground: true, preferCSSPageSize: true, paperWidth: data.width / 96, paperHeight: data.height / 96, marginTop: 0, marginBottom: 0, marginLeft: 0, marginRight: 0 });
    const pdfPath = path.join(output, 'print.pdf'); fs.writeFileSync(pdfPath, Buffer.from(pdf.data, 'base64'));
    cp.execFileSync('pdftoppm', ['-r', '96', pdfPath, path.join(output, 'page')]);
    const video = ppm(path.join(output, 'page-09.ppm'));
    assert.deepEqual([video.width, video.height], [1280, 720]);
    for (const pageNumber of [1, 8]) {
      const crop = ppm(path.join(output, `page-${String(pageNumber).padStart(2, '0')}.ppm`));
      assert.deepEqual([crop.width, crop.height], [1280, 720]);
      const clipped = pixels(crop, (r, g, b) => (r === 248 && g === 113 && b === 113) || (r === 52 && g === 211 && b === 153));
      assert(clipped.count > 300000, `Crop on page ${pageNumber} must remain visible`);
      nearBox(clipped.box, [304, 108, 976, 612], `Crop clip on page ${pageNumber}`);
      assert.equal(pixels(crop, (r, g, b) => (r === 96 && g === 165 && b === 250) || (r === 251 && g === 191 && b === 36)).count, 0, 'Excluded crop quadrants must not print');
    }
    const contain = ppm(path.join(output, 'page-10.ppm'));
    const keptColor = (r,g,b) => (r === 248 && g === 113 && b === 113) || (r === 52 && g === 211 && b === 153);
    nearBox(pixels(contain, keptColor).box, [472,108,808,612], 'Contain crop letterbox');
    nearBox(pixels(ppm(path.join(output, 'page-11.ppm')), keptColor).box, [304,108,976,612], 'Fill crop');
    const at = (page,x,y) => [...page.bytes.subarray((y*page.width+x)*3,(y*page.width+x)*3+3)];
    assert.deepEqual(at(ppm(path.join(output, 'page-12.ppm')),640,150), [52,211,153], 'Vertical flip keeps the selected region');
    const filtered = ppm(path.join(output, 'page-13.ppm'));
    const center = at(filtered,640,200), corner = at(filtered,305,109);
    assert(Math.max(...center)-Math.min(...center) <= 1, 'Grayscale filter is preserved');
    assert(Math.max(...corner)-Math.min(...corner) > 5, 'Rounded corner remains clipped to the slide background');
    nearBox(pixels(ppm(path.join(output, 'page-14.ppm')), keptColor).box, [304,108,976,612], 'Raster image crop');
    const frame = pixels(video, (r, g, b) => r < 70 && g > 150 && g < 180 && b > 190);
    nearBox(frame.box, [256, 144, 1024, 576], 'Video PDF placement');
    console.log(`PASS: production print boot, decoded video, authored frame geometry and PDF crop/video pixels. Artifacts: ${output}`);
  } finally {
    try { if (call) await call('Browser.close'); } catch {}
    socket?.close(); try { browserProcess.kill(); } catch {}
    server.close(); await delay(200); fs.rmSync(profile, { recursive: true, force: true });
  }
})().catch(error => { console.error(error); server.close(); process.exitCode = 1; });
