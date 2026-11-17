#!/usr/bin/env python3
"""Run a browser regression harness to completion in a fresh Chromium profile."""
import argparse
import functools
import html
import http.server
import pathlib
import re
import shutil
import subprocess
import tempfile
import threading


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--browser", help="Path to Chromium or Google Chrome")
    parser.add_argument("--page", default="test_editing.html", help="Test HTML filename in tools/")
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
        with tempfile.TemporaryDirectory(prefix="cadenza-browser-test-", dir=cache) as profile:
            result = subprocess.run(
                [node, str(root / "tools" / "browser_harness.cjs"), browser,
                 f"http://127.0.0.1:{server.server_port}/tools/{args.page}", profile],
                capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=75, check=False,
            )
            report = re.search(r'<pre id="result">(.*?)</pre>', result.stdout, re.S)
            print(html.unescape(report.group(1)) if report else "Browser did not finish the test harness.")
            if result.returncode or not re.search(r'<body\b[^>]*data-result="pass"', result.stdout):
                print(result.stderr[-4000:])
                raise SystemExit(1)
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
