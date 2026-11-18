// Wait for the harness's explicit result in real time. A virtual-time budget
// can expire while modules, fonts or native browser work are still pending.
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

// test_browser.py owns the process group on POSIX, including any descendants
// left by a crashed browser. On Windows, terminate the browser tree while its
// root is still alive; merely closing the CDP socket does not close Chromium.
async function stopBrowser(child) {
  try {
    if (child.pid && process.platform === 'win32' && child.exitCode === null) {
      const error = await new Promise(resolve => cp.execFile('taskkill', ['/PID', String(child.pid), '/T', '/F'],
        { timeout: 5000, windowsHide: true }, resolve));
      if (error && child.exitCode === null && child.signalCode === null) {
        console.error('Could not terminate the browser process tree:', error);
        process.exitCode = 1;
        child.kill();
      }
    } else {
      child.kill('SIGKILL');
    }
    // TerminateProcess/taskkill can return before Windows releases resources.
    if (child.pid && child.exitCode === null && child.signalCode === null) {
      await new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error('Browser did not exit after termination')), 5000);
        child.once('exit', () => { clearTimeout(timer); resolve(); });
      });
    }
  } finally {
    // Descendants may inherit stderr. Do not wait for their pipe to close.
    child.stderr?.destroy();
    child.unref();
  }
}

async function runBrowser(command, url, profile, { startupMs = 30000, connectMs = 5000, testMs = 30000 } = {}) {
  const child = cp.spawn(command[0], [...command.slice(1), '--headless', '--no-sandbox', '--disable-gpu',
    '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
  let launchError, stderr = '', socket, call, printed = false;
  const browserErrors = [], pending = new Map();
  const phase = name => console.error(`[browser] ${name}`);
  child.on('error', error => { launchError = error; });
  child.stderr.on('data', chunk => { stderr = (stderr + chunk).slice(-4000); });
  async function until(check, message, timeout) {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      if (launchError) throw launchError;
      if (child.exitCode !== null || child.signalCode !== null) {
        throw new Error(`Browser exited before completing the harness (${child.exitCode ?? child.signalCode})`);
      }
      const result = await check();
      if (result) return result;
      await delay(100);
    }
    throw new Error(message);
  }
  try {
    phase('starting');
    const port = await until(() => {
      try { return fs.readFileSync(path.join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0].trim(); }
      catch { return null; }
    }, 'Browser did not start', startupMs);
    assert(/^\d+$/.test(port), 'Invalid browser debugging port');
    phase('discovering page');
    const tabs = await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(5000) }).then(response => response.json());
    const page = tabs.find(tab => tab.type === 'page');
    assert(page, 'Browser did not create a page');
    phase('connecting');
    socket = new WebSocket(page.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => finish(new Error('Browser WebSocket handshake timed out')), connectMs);
      function finish(error) {
        clearTimeout(timer);
        socket.removeEventListener('open', open);
        socket.removeEventListener('error', failed);
        socket.removeEventListener('close', failed);
        error ? reject(error) : resolve();
      }
      const open = () => finish();
      const failed = () => finish(new Error('Browser WebSocket closed before connecting'));
      socket.addEventListener('open', open, { once: true });
      socket.addEventListener('error', failed, { once: true });
      socket.addEventListener('close', failed, { once: true });
    });
    let sequence = 0;
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
      try { socket.send(JSON.stringify({ id, method, params })); }
      catch (error) { pending.delete(id); clearTimeout(timer); reject(error); }
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
    phase('navigating');
    // A cold Windows runner can take more than five seconds to commit the
    // navigation. Keep its loading deadline separate from test completion.
    await call('Page.navigate', { url }, 30000);
    phase('waiting for assertions');
    try {
      await until(async () => ['pass', 'fail'].includes(await evaluate('document.body?.dataset.result')),
        'Browser harness did not finish within its deadline', testMs);
      if (await evaluate('document.body?.dataset.result') === 'fail') {
        process.exitCode = 1;
        if (browserErrors.length) console.error(browserErrors.join('\n'));
      }
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
    phase('stopping');
    for (const task of pending.values()) clearTimeout(task.timer);
    pending.clear();
    socket?.close();
    await stopBrowser(child);
    phase('stopped');
  }
}

module.exports = { runBrowser };
if (require.main === module) {
  const [browser, url, profile] = process.argv.slice(2);
  assert(browser && url && profile, 'Expected browser, harness URL and isolated profile');
  runBrowser([browser], url, profile).catch(error => { console.error(error); process.exitCode = 1; });
}
