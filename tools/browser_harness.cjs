// Wait for the harness's explicit result in real time. A virtual-time budget
// can expire while modules, fonts or native browser work are still pending.
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process');
const [browser, url, profile] = process.argv.slice(2);
assert(browser && url && profile, 'Expected browser, harness URL and isolated profile');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const child = cp.spawn(browser, ['--headless', '--no-sandbox', '--disable-gpu', '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
let launchError, stderr = '', socket, call;
const browserErrors = [];
child.on('error', error => { launchError = error; });
child.stderr.on('data', chunk => { stderr = (stderr + chunk).slice(-4000); });
async function until(check, message) {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    if (launchError) throw launchError;
    const result = await check();
    if (result) return result;
    await delay(100);
  }
  throw new Error(message);
}

(async () => {
  let printed = false;
  try {
    const port = await until(() => {
      try { return fs.readFileSync(path.join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0].trim(); }
      catch { return null; }
    }, 'Browser did not start');
    assert(/^\d+$/.test(port), 'Invalid browser debugging port');
    const tabs = await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(5000) }).then(response => response.json());
    const page = tabs.find(tab => tab.type === 'page');
    assert(page, 'Browser did not create a page');
    socket = new WebSocket(page.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => {
      socket.addEventListener('open', resolve, { once: true });
      socket.addEventListener('error', reject, { once: true });
    });
    let sequence = 0;
    const pending = new Map();
    socket.addEventListener('message', event => {
      const message = JSON.parse(event.data), task = pending.get(message.id);
      if (message.method === 'Log.entryAdded' && message.params.entry.level === 'error') browserErrors.push(message.params.entry.text);
      if (message.method === 'Runtime.exceptionThrown') browserErrors.push(message.params.exceptionDetails.exception?.description || message.params.exceptionDetails.text);
      if (!task) return;
      pending.delete(message.id); clearTimeout(task.timer);
      message.error ? task.reject(new Error(JSON.stringify(message.error))) : task.resolve(message.result);
    });
    socket.addEventListener('close', () => {
      for (const task of pending.values()) { clearTimeout(task.timer); task.reject(new Error('Browser connection closed')); }
      pending.clear();
    });
    call = (method, params = {}, timeout = 5000) => new Promise((resolve, reject) => {
      const id = ++sequence;
      const timer = setTimeout(() => { pending.delete(id); reject(new Error(`${method} timed out`)); }, timeout);
      pending.set(id, { resolve, reject, timer });
      socket.send(JSON.stringify({ id, method, params }));
    });
    const evaluate = async expression => {
      const result = await call('Runtime.evaluate', { expression, returnByValue: true });
      assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
      return result.result.value;
    };
    await call('Page.enable');
    await call('Log.enable');
    await call('Runtime.enable');
    await call('Page.bringToFront');
    // A cold Windows runner can take more than five seconds to commit the
    // navigation. Keep its loading deadline separate from test completion.
    await call('Page.navigate', { url }, 30000);
    try {
      await until(async () => ['pass', 'fail'].includes(await evaluate('document.body?.dataset.result')), 'Browser harness did not finish within 30 seconds');
      if (await evaluate('document.body?.dataset.result') === 'fail' && browserErrors.length) console.error(browserErrors.join('\n'));
    } finally {
      process.stdout.write(await evaluate('document.documentElement.outerHTML'));
      printed = true;
    }
  } catch (error) {
    if (!printed) process.stdout.write('<pre id="result">Browser did not finish the test harness.</pre>');
    console.error(error);
    if (browserErrors.length) console.error(browserErrors.join('\n'));
    if (stderr) console.error(stderr);
    process.exitCode = 1;
  } finally {
    try { if (call) await call('Browser.close'); } catch {}
    socket?.close();
    child.kill();
    await delay(200);
  }
})();
