#!/usr/bin/env python3
"""Exercise mirror fallback and process deadlines without sudo or real APT."""
import json
import os
import pathlib
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

from ci_install_deps import InstallError, install_dependencies, run_command

FAKE_APT = r'''
import json, os, pathlib, signal, subprocess, sys, time
args = sys.argv[1:]
source = pathlib.Path(next(arg.split('=', 1)[1] for arg in args if arg.startswith('Dir::Etc::sourcelist=')))
text = source.read_text()
primary = 'https://archive.ubuntu.com/' in text
phase = 'update' if 'update' in args else 'download' if '--download-only' in args else 'install'
with open(os.environ['APT_TEST_LOG'], 'a') as log:
    log.write(json.dumps({'source': text, 'phase': phase, 'args': args}) + '\n')
mode = os.environ.get('APT_TEST_MODE', 'success')
if mode == 'slow' and primary and phase == 'download':
    child = subprocess.Popen([sys.executable, '-c', 'import signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(60)'])
    pathlib.Path(os.environ['APT_TEST_CHILD']).write_text(str(child.pid))
    # Ongoing output must not reset the total download deadline.
    while True:
        print('still downloading', flush=True)
        time.sleep(0.05)
if mode == 'slow_install' and phase == 'install':
    time.sleep(60)
if mode == 'both' or (mode == phase and (primary or phase == 'install')):
    sys.exit(100)
'''


@unittest.skipUnless(sys.platform == "linux", "The installer runs on Linux CI")
class InstallerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory(prefix="cadenza-fake-apt-")
        self.addCleanup(self.directory.cleanup)
        self.root = pathlib.Path(self.directory.name)
        self.log = self.root / "calls.jsonl"
        fake = self.root / "apt-get"
        fake.write_text(f"#!{sys.executable}\n" + FAKE_APT, encoding="utf-8")
        fake.chmod(0o755)
        self.environment = patch.dict(os.environ, {
            "PATH": str(self.root) + os.pathsep + os.environ["PATH"],
            "APT_TEST_LOG": str(self.log), "APT_TEST_MODE": "success",
            "APT_TEST_CHILD": str(self.root / "child.pid"),
        })
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def calls(self) -> list[dict]:
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def install(self, **kwargs) -> None:
        install_dependencies(["libwebkit2gtk-4.1-dev", "librsvg2-dev"], "noble", "amd64", **kwargs)

    def test_primary_downloads_before_installing_without_network(self) -> None:
        self.install()
        calls = self.calls()
        self.assertEqual([call["phase"] for call in calls], ["update", "download", "install"])
        for call in calls:
            self.assertIn("URIs: https://archive.ubuntu.com/ubuntu\n", call["source"])
            self.assertIn("noble-security", call["source"])
            self.assertIn("Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg", call["source"])
            self.assertIn("Dir::Etc::sourceparts=-", call["args"])
            self.assertIn("APT::Update::Error-Mode=any", call["args"])
        self.assertEqual(calls[1]["args"][-3:], ["--", "libwebkit2gtk-4.1-dev", "librsvg2-dev"])
        self.assertIn("--no-download", calls[2]["args"])

    def test_update_failure_selects_the_global_fallback(self) -> None:
        os.environ["APT_TEST_MODE"] = "update"
        self.install()
        calls = self.calls()
        self.assertEqual([call["phase"] for call in calls], ["update", "update", "download", "install"])
        self.assertIn("URIs: https://security.ubuntu.com/ubuntu\n", calls[1]["source"])

    def test_download_failure_retries_before_any_package_configuration(self) -> None:
        os.environ["APT_TEST_MODE"] = "download"
        self.install()
        calls = self.calls()
        self.assertEqual([call["phase"] for call in calls], ["update", "download", "update", "download", "install"])
        self.assertIn("URIs: https://security.ubuntu.com/ubuntu\n", calls[-1]["source"])

    def test_slow_progress_times_out_and_stops_workers_before_fallback(self) -> None:
        os.environ["APT_TEST_MODE"] = "slow"
        started = time.monotonic()
        self.install(attempt_seconds=1, total_seconds=10)
        self.assertLess(time.monotonic() - started, 8)
        pid = int((self.root / "child.pid").read_text())
        status = pathlib.Path(f"/proc/{pid}/stat")
        for _ in range(100):
            if not status.exists() or status.read_text().rsplit(")", 1)[1].split()[0] == "Z":
                break
            time.sleep(0.01)
        else:
            self.fail(f"Download worker {pid} survived the timeout")
        self.assertIn("https://security.ubuntu.com/ubuntu", self.calls()[-1]["source"])

    def test_both_mirrors_failing_is_a_hard_failure(self) -> None:
        os.environ["APT_TEST_MODE"] = "both"
        with self.assertRaisesRegex(InstallError, "Both global"):
            self.install()
        self.assertEqual([call["phase"] for call in self.calls()], ["update", "update"])

    def test_package_configuration_failure_is_not_retried(self) -> None:
        os.environ["APT_TEST_MODE"] = "install"
        with self.assertRaisesRegex(InstallError, "Offline package installation failed"):
            self.install()
        self.assertEqual([call["phase"] for call in self.calls()], ["update", "download", "install"])

    def test_expired_total_budget_starts_no_commands(self) -> None:
        with self.assertRaisesRegex(InstallError, "budget expired"):
            self.install(total_seconds=0)
        self.assertFalse(self.log.exists())

    def test_total_deadline_bounds_a_progressing_download(self) -> None:
        os.environ["APT_TEST_MODE"] = "slow"
        started = time.monotonic()
        with self.assertRaisesRegex(InstallError, "budget expired"):
            self.install(attempt_seconds=10, total_seconds=0.5)
        self.assertLess(time.monotonic() - started, 3)
        self.assertEqual([call["phase"] for call in self.calls()], ["update", "download"])

    def test_installation_deadline_fails_without_restarting_dpkg(self) -> None:
        os.environ["APT_TEST_MODE"] = "slow_install"
        with self.assertRaisesRegex(InstallError, "Offline package installation failed"):
            self.install(install_seconds=0.5)
        self.assertEqual([call["phase"] for call in self.calls()], ["update", "download", "install"])

    def test_deadline_prevents_starting_another_command(self) -> None:
        with patch("ci_install_deps.subprocess.Popen") as start:
            self.assertFalse(run_command(["apt-get", "update"], time.monotonic() - 1))
        start.assert_not_called()

    def test_release_runner_uses_jammy_suites(self) -> None:
        install_dependencies(["patchelf", "gstreamer1.0-libav"], "jammy", "amd64")
        self.assertIn("Suites: jammy jammy-updates jammy-backports jammy-security", self.calls()[0]["source"])


if __name__ == "__main__":
    unittest.main()
