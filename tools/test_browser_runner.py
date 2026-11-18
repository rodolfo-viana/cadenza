#!/usr/bin/env python3
"""Fault-injection checks for the browser driver; no installed browser required."""
import csv
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

from test_browser import browser_profile, passed, run_harness

NODE = shutil.which("node")
HARNESS = pathlib.Path(__file__).resolve().with_name("browser_harness.cjs")
FAKE_BROWSER = r"""
const fs = require('node:fs'), http = require('node:http'), path = require('node:path'), cp = require('node:child_process');
const mode = process.argv[2];
if (mode === 'exit') process.exit(7);
const profile = process.argv.find(arg => arg.startsWith('--user-data-dir=')).slice(16);
const child = cp.spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { stdio: 'inherit' });
fs.writeFileSync(path.join(profile, 'descendant.pid'), String(child.pid));
const server = http.createServer((req, res) => {
  res.setHeader('Content-Type', 'application/json');
  res.end(JSON.stringify([{ type: 'page', webSocketDebuggerUrl: `ws://127.0.0.1:${server.address().port}/devtools/page/1` }]));
});
// Accept TCP, but never finish the WebSocket upgrade.
server.on('upgrade', (req, socket) => { socket.on('error', () => {}); });
server.listen(0, '127.0.0.1', () => fs.writeFileSync(path.join(profile, 'DevToolsActivePort'), String(server.address().port)));
"""


def alive(pid: int) -> bool:
    if os.name == "nt":
        output = subprocess.check_output(["tasklist", "/FI", f"PID eq {pid}", "/FO", "CSV", "/NH"], text=True)
        return any(len(row) > 1 and row[1] == str(pid) for row in csv.reader(output.splitlines()))
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    status = pathlib.Path(f"/proc/{pid}/stat")
    # A terminated child can await reaping briefly on Linux.
    try:
        return not status.exists() or status.read_text().rsplit(")", 1)[1].split()[0] != "Z"
    except FileNotFoundError:
        return False


@unittest.skipUnless(NODE, "Node.js 22 or newer is required")
class BrowserRunnerTests(unittest.TestCase):
    def assert_terminated(self, pid: int) -> None:
        deadline = time.monotonic() + 2
        while alive(pid) and time.monotonic() < deadline:
            time.sleep(0.02)
        self.assertFalse(alive(pid), f"Descendant {pid} survived process-tree cleanup")

    def test_stalled_websocket_fails_and_cleans_descendants(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            fake = root / "fake.cjs"
            fake.write_text(FAKE_BROWSER, encoding="utf-8")
            script = "const [h, c, p] = JSON.parse(process.argv[1]); require(h).runBrowser(c, 'http://localhost/test', p, {connectMs: 300});"
            result = run_harness([NODE, "-e", script, json.dumps([str(HARNESS), [NODE, str(fake), "hang"], directory])], timeout=10)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("WebSocket handshake timed out", result.stderr)
            self.assertIn("[browser] stopped", result.stderr)
            self.assertNotIn("supervisor timed out", result.stderr)
            self.assert_terminated(int((root / "descendant.pid").read_text()))

    def test_early_browser_exit_fails_without_waiting_for_startup_deadline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fake = pathlib.Path(directory) / "fake.cjs"
            fake.write_text(FAKE_BROWSER, encoding="utf-8")
            script = "const [h, c, p] = JSON.parse(process.argv[1]); require(h).runBrowser(c, 'http://localhost/test', p);"
            result = run_harness([NODE, "-e", script, json.dumps([str(HARNESS), [NODE, str(fake), "exit"], directory])], timeout=5)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Browser exited before completing the harness (7)", result.stderr)
            self.assertNotIn("supervisor timed out", result.stderr)

    def test_supervisor_timeout_preserves_partial_output_and_rejects_pass(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            pid_file = pathlib.Path(directory) / "descendant.pid"
            script = """
const cp = require('node:child_process'), fs = require('node:fs');
const child = cp.spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], {stdio:'inherit'});
fs.writeFileSync(process.argv[1], String(child.pid));
console.log('<body data-result="pass"><pre id="result">partial result</pre></body>');
console.error('driver stuck during cleanup');
setInterval(() => {}, 1000);
"""
            result = run_harness([NODE, "-e", script, str(pid_file)], timeout=2)
            self.assertFalse(passed(result))
            self.assertIn("partial result", result.stdout)
            self.assertIn("driver stuck during cleanup", result.stderr)
            self.assertIn("supervisor timed out", result.stderr)
            self.assert_terminated(int(pid_file.read_text()))

    @unittest.skipUnless(os.name == "nt", "Windows prevents deleting an open file")
    def test_profile_waits_for_delayed_file_release(self) -> None:
        self.check_profile_lock(release_after=1, cleanup_timeout=5)

    @unittest.skipUnless(os.name == "nt", "Windows prevents deleting an open file")
    def test_profile_still_fails_if_file_remains_locked(self) -> None:
        with self.assertRaises(PermissionError):
            self.check_profile_lock(release_after=30, cleanup_timeout=0.1)

    def check_profile_lock(self, release_after: float, cleanup_timeout: float) -> None:
        with tempfile.TemporaryDirectory() as cache:
            locker = None
            try:
                with browser_profile(pathlib.Path(cache), cleanup_timeout) as profile:
                    script = "import sys,time; f=open(sys.argv[1], 'w'); print('locked', flush=True); time.sleep(float(sys.argv[2])); f.close()"
                    locker = subprocess.Popen([sys.executable, "-c", script, str(pathlib.Path(profile) / "data_0"), str(release_after)],
                                              stdout=subprocess.PIPE, text=True)
                    self.assertEqual(locker.stdout.readline().strip(), "locked")
                    # The same profile is cleaned on context exit, while the file
                    # is held by another process as with Edge's cache workers.
            finally:
                if locker:
                    locker.kill()
                    locker.wait(timeout=5)
                    locker.stdout.close()

    def test_pass_requires_both_exit_status_and_page_result(self) -> None:
        for code, body, expected in (
            (0, '<body data-result="pass">', True),
            (1, '<body data-result="pass">', False),
            (0, '<body data-result="fail">', False),
            (0, '<pre id="result">not finished</pre>', False),
        ):
            with self.subTest(code=code, body=body):
                self.assertEqual(passed(subprocess.CompletedProcess([], code, body, "")), expected)


if __name__ == "__main__":
    unittest.main()
