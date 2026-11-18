#!/usr/bin/env python3
"""Run a browser regression harness to completion in a fresh Chromium profile."""
import argparse
import contextlib
import functools
import html
import http.server
import os
import pathlib
import re
import signal
import shutil
import subprocess
import tempfile
import threading
import time
from collections.abc import Iterator


def run_harness(command: list[str], timeout: float = 105) -> subprocess.CompletedProcess[str]:
    """Bound the entire process tree and retain output even if the driver hangs."""
    # Files avoid communicate() waiting forever on pipes inherited by Chromium.
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        proc = subprocess.Popen(command, stdout=stdout, stderr=stderr, start_new_session=os.name != "nt")
        timed_out = False
        cleanup_error = ""
        try:
            proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            if os.name == "nt":
                try:
                    subprocess.run(["taskkill", "/PID", str(proc.pid), "/T", "/F"],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5, check=True)
                except (OSError, subprocess.SubprocessError) as error:
                    cleanup_error = f"\nCould not terminate the harness process tree: {error}"
            else:
                try:
                    os.killpg(proc.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            proc.kill()
            proc.wait(timeout=5)
        finally:
            if os.name != "nt":
                # A crashed browser can leave descendants after Node has exited.
                try:
                    os.killpg(proc.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
        stdout.seek(0)
        stderr.seek(0)
        out, err = stdout.read().decode("utf-8", "replace"), stderr.read().decode("utf-8", "replace")
    if timed_out:
        err += f"\nBrowser supervisor timed out after {timeout:g} seconds.{cleanup_error}\n"
    return subprocess.CompletedProcess(command, proc.returncode or int(timed_out), out, err)


def passed(result: subprocess.CompletedProcess[str]) -> bool:
    """Require both a successful driver and the page's explicit pass marker."""
    return result.returncode == 0 and bool(re.search(r'<body\b[^>]*data-result="pass"', result.stdout))


@contextlib.contextmanager
def browser_profile(cache: pathlib.Path, cleanup_timeout: float = 5) -> Iterator[str]:
    """Remove the isolated profile once Windows has released its file handles."""
    directory = tempfile.TemporaryDirectory(prefix="cadenza-browser-test-", dir=cache)
    try:
        yield directory.name
    finally:
        deadline = time.monotonic() + cleanup_timeout
        while True:
            try:
                directory.cleanup()
                break
            except PermissionError:
                # Termination is asynchronous on Windows, including pending I/O.
                # Retry cleanup only; never retry assertions or ignore a stuck lock.
                if time.monotonic() >= deadline:
                    raise
                time.sleep(0.1)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--browser", help="Path to Chromium or Google Chrome")
    parser.add_argument("--page", default="test_editing.html", help="Test HTML filename in tools/")
    parser.add_argument("--output-dir", type=pathlib.Path, help="Retain the page HTML and driver diagnostics")
    args = parser.parse_args()
    browser = args.browser or next(
        (path for name in ("chromium", "google-chrome", "chromium-browser") if (path := shutil.which(name))), None
    )
    if not browser:
        parser.error("Chromium or Google Chrome is required; use --browser to select it")
    node = shutil.which("node")
    if not node:
        parser.error("Node.js 22 or newer is required")
    root = pathlib.Path(__file__).resolve().parent.parent
    if pathlib.Path(args.page).name != args.page or not (root / "tools" / args.page).is_file():
        parser.error("--page must name an existing HTML file in tools/")

    class QuietHandler(http.server.SimpleHTTPRequestHandler):
        # Windows' MIME registry can classify .mjs as a non-JavaScript file.
        # ES modules require a JavaScript MIME type in every browser.
        extensions_map = {
            **http.server.SimpleHTTPRequestHandler.extensions_map,
            ".js": "text/javascript",
            ".mjs": "text/javascript",
        }

        def log_message(self, *args: object) -> None:
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(QuietHandler, directory=str(root)))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    # Snap Chromium sees a private home/cache; its shared directory is also
    # reachable by the host process that creates and cleans up this profile.
    executable = pathlib.Path(shutil.which(browser) or browser)
    cache = pathlib.Path.home() / ("snap/chromium/common" if executable.parent == pathlib.Path("/snap/bin") else ".cache")
    cache.mkdir(parents=True, exist_ok=True)
    try:
        with browser_profile(cache) as profile:
            result = run_harness(
                [node, str(root / "tools" / "browser_harness.cjs"), browser,
                 f"http://127.0.0.1:{server.server_port}/tools/{args.page}", profile],
            )
            if args.output_dir:
                args.output_dir.mkdir(parents=True, exist_ok=True)
                (args.output_dir / args.page).write_text(result.stdout, encoding="utf-8")
                (args.output_dir / f"{args.page}.log").write_text(result.stderr, encoding="utf-8")
            report = re.search(r'<pre id="result">(.*?)</pre>', result.stdout, re.S)
            print(html.unescape(report.group(1)) if report else "Browser did not finish the test harness.")
            if not passed(result):
                print(result.stderr[-4000:])
                raise SystemExit(1)
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
