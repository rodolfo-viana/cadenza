// Wait for the harness's explicit result in real time. A virtual-time budget
// can expire while modules, fonts or native browser work are still pending.
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

function exited(child) {
  return !child.pid || child.exitCode !== null || child.signalCode !== null;
}

function waitForExit(child, timeout) {
  if (exited(child)) return Promise.resolve(true);
  return new Promise(resolve => {
    const finished = () => { clearTimeout(timer); resolve(true); };
    const timer = setTimeout(() => { child.removeListener('exit', finished); resolve(false); }, timeout);
    child.once('exit', finished);
  });
}

// Prefer Chromium's normal shutdown: force-killing sandboxed Edge workers can
// race their own exit. The caller or this driver owns the POSIX process group.
async function stopBrowser(child, graceful, ownProcessGroup) {
  try {
    if (exited(child) || (graceful && await waitForExit(child, 5000))) return;
    let killError;
    if (process.platform === 'win32') {
      killError = await new Promise(resolve => cp.execFile('taskkill', ['/PID', String(child.pid), '/T', '/F'],
        { timeout: 5000, windowsHide: true }, resolve));
    }
    if (!exited(child)) child.kill('SIGKILL');
    // taskkill can report an already-exiting worker as an error. Verify the
    // actual process exit; profile cleanup separately verifies handle release.
    if (!await waitForExit(child, 5000)) {
      throw new Error('Browser did not exit after termination', { cause: killError });
    }
  } finally {
    // Export drivers run without the Python supervisor. Own and clean their
    // POSIX process group, including children left after an early browser exit.
    if (ownProcessGroup && process.platform !== 'win32' && child.pid) {
      try { process.kill(-child.pid, 'SIGKILL'); }
      catch (error) { if (error.code !== 'ESRCH') throw error; }
    }
    // Descendants may inherit stderr. Do not wait for their pipe to close.
    child.stderr?.destroy();
    child.unref();
  }
}

// Share bounded startup, CDP and shutdown across UI, PDF and PPTX tests. The
// caller creates/removes the profile; standalone drivers own their process group.
async function withBrowser(command, profile, task, {
  startupMs = 30000, connectMs = 5000, commandMs = 5000,
  logFile, ownProcessGroup = false,
} = {}) {
  const report = message => {
    console.error(message);
    if (logFile) fs.appendFileSync(logFile, `${message}\n`);
  };
  if (logFile) fs.writeFileSync(logFile, '');
  const child = cp.spawn(command[0], [...command.slice(1), '--headless', '--no-sandbox', '--disable-gpu',
    '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank'], {
    stdio: ['ignore', 'ignore', 'pipe'], detached: ownProcessGroup && process.platform !== 'win32',
  });
  let launchError, stderr = '', socket, call, failure;
  const browserErrors = [], pending = new Map();
  const phase = name => report(`[browser] ${name}`);
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
    call = (method, params = {}, timeout = commandMs) => new Promise((resolve, reject) => {
      const id = ++sequence;
      const timer = setTimeout(() => { pending.delete(id); reject(new Error(`${method} timed out`)); }, timeout);
      pending.set(id, { resolve, reject, timer });
      try { socket.send(JSON.stringify({ id, method, params })); }
      catch (error) { pending.delete(id); clearTimeout(timer); reject(error); }
    });
    const evaluate = async (expression, { awaitPromise = false } = {}) => {
      const result = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise });
      assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
      return result.result.value;
    };
    return await task({ call, evaluate, until, phase, browserErrors });
  } catch (error) {
    failure = error;
    report(error.stack || String(error));
    if (browserErrors.length) report(browserErrors.join('\n'));
    if (stderr) report(stderr);
    throw error;
  } finally {
    phase('stopping');
    const graceful = !!call && socket.readyState === WebSocket.OPEN;
    // Chromium often closes the connection before acknowledging Browser.close.
    try { if (graceful) await call('Browser.close', {}, 5000); } catch {}
    for (const task of pending.values()) clearTimeout(task.timer);
    pending.clear();
    socket?.close();
    try { await stopBrowser(child, graceful, ownProcessGroup); }
    catch (error) { if (!failure) throw error; report(`Cleanup failed: ${error.stack || error}`); }
    phase('stopped');
  }
}

async function runBrowser(command, url, profile, options = {}) {
  let printed = false;
  try {
    await withBrowser(command, profile, async ({ call, evaluate, until, phase, browserErrors }) => {
      await call('Page.enable');
      await call('Log.enable');
      await call('Runtime.enable');
      await call('Page.bringToFront');
      phase('navigating');
      // Navigation and test completion have independent deadlines.
      await call('Page.navigate', { url }, 30000);
      phase('waiting for assertions');
      try {
        await until(async () => ['pass', 'fail'].includes(await evaluate('document.body?.dataset.result')),
          'Browser harness did not finish within its deadline', options.testMs ?? 30000);
        if (await evaluate('document.body?.dataset.result') === 'fail') {
          process.exitCode = 1;
          if (browserErrors.length) console.error(browserErrors.join('\n'));
        }
      } finally {
        process.stdout.write(await evaluate('document.documentElement.outerHTML'));
        printed = true;
      }
    }, options);
  } catch (error) {
    if (!printed) process.stdout.write('<pre id="result">Browser did not finish the test harness.</pre>');
    console.error(error);
    process.exitCode = 1;
  }
}

module.exports = { runBrowser, withBrowser };
if (require.main === module) {
  const [browser, url, profile] = process.argv.slice(2);
  assert(browser && url && profile, 'Expected browser, harness URL and isolated profile');
  runBrowser([browser], url, profile).catch(error => { console.error(error); process.exitCode = 1; });
}
