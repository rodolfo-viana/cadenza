#!/usr/bin/env python3
"""Install Ubuntu CI dependencies from bounded, signed global-mirror downloads.

Run with sudo on Ubuntu 22.04/24.04 runners. Each mirror gets 120 seconds for
update + download; configuration gets 120 seconds within a 360-second total.
Fallback only happens before dpkg starts. No third-party Python packages needed.
"""
import argparse
import os
import pathlib
import platform
import signal
import subprocess
import tempfile
import time

MIRRORS = ("https://archive.ubuntu.com/ubuntu", "https://security.ubuntu.com/ubuntu")
ATTEMPT_SECONDS = 120
INSTALL_SECONDS = 120
TOTAL_SECONDS = 360


class InstallError(RuntimeError):
    """Dependency preparation failed; the CI job must not continue."""


def stop_process_group(process: subprocess.Popen) -> None:
    """Reap the command and stop its workers before attempting another mirror."""
    try:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            pass
    except ProcessLookupError:
        pass
    finally:
        # Workers can outlive a parent that accepted SIGTERM.
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()


def run_command(command: list[str], deadline: float) -> bool:
    """Run with a wall-clock deadline, including slowly progressing transfers."""
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        print("[apt] Deadline exhausted before starting command", flush=True)
        return False
    started = time.monotonic()
    process = subprocess.Popen(command, start_new_session=True)
    try:
        code = process.wait(timeout=remaining)
    except subprocess.TimeoutExpired:
        stop_process_group(process)
        print(f"[apt] Timed out after {time.monotonic() - started:.1f}s", flush=True)
        return False
    except BaseException:
        stop_process_group(process)
        raise
    print(f"[apt] Command finished in {time.monotonic() - started:.1f}s (exit {code})", flush=True)
    return code == 0


def install_dependencies(packages: list[str], codename: str, architecture: str, *,
                         attempt_seconds: float = ATTEMPT_SECONDS,
                         install_seconds: float = INSTALL_SECONDS,
                         total_seconds: float = TOTAL_SECONDS) -> None:
    """Fetch from one complete Ubuntu archive, then install offline exactly once."""
    if codename not in ("jammy", "noble") or architecture != "amd64":
        raise InstallError("This installer supports Ubuntu 22.04/24.04 amd64 CI runners")
    if not packages:
        raise InstallError("At least one dependency is required")
    started = time.monotonic()
    deadline = started + total_seconds
    with tempfile.TemporaryDirectory(prefix="cadenza-apt-") as directory:
        root = pathlib.Path(directory)
        # APT's unprivileged _apt worker must be able to read the source file.
        root.chmod(0o755)
        sources = root / "ubuntu.sources"
        apt = ["apt-get", "-o", f"Dir::Etc::sourcelist={sources}",
               "-o", "Dir::Etc::sourceparts=-", "-o", "APT::Update::Error-Mode=any",
               "-o", "Acquire::Retries=0", "-o", "Acquire::http::Timeout=15",
               "-o", "Acquire::https::Timeout=15", "-o", "DPkg::Lock::Timeout=15"]
        for mirror in MIRRORS:
            sources.write_text(
                f"Types: deb\nURIs: {mirror}\n"
                f"Suites: {codename} {codename}-updates {codename}-backports {codename}-security\n"
                "Components: main restricted universe multiverse\n"
                f"Architectures: {architecture}\n"
                "Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n",
                encoding="utf-8",
            )
            sources.chmod(0o644)
            attempt_deadline = min(deadline, time.monotonic() + attempt_seconds)
            print(f"[apt] Trying {mirror}; update + download budget {attempt_seconds:g}s", flush=True)
            if not run_command([*apt, "update"], attempt_deadline):
                print(f"[apt] Mirror update failed or timed out: {mirror}", flush=True)
                continue
            if not run_command([*apt, "install", "--yes", "--download-only", "--", *packages], attempt_deadline):
                print(f"[apt] Mirror download failed or timed out: {mirror}", flush=True)
                continue
            # Reuse downloaded .deb files, but never fetch during dpkg configuration.
            # A configuration failure is not retried against another mirror.
            install_deadline = min(deadline, time.monotonic() + install_seconds)
            print(f"[apt] Downloads complete from {mirror}; installing without network downloads", flush=True)
            if not run_command([*apt, "install", "--yes", "--no-download", "--", *packages], install_deadline):
                raise InstallError("Offline package installation failed or exceeded its deadline")
            print(f"[apt] Dependencies installed in {time.monotonic() - started:.1f}s using {mirror}", flush=True)
            return
    raise InstallError("Both global Ubuntu mirrors failed or the total download budget expired")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("packages", nargs="+")
    args = parser.parse_args()
    release = platform.freedesktop_os_release()
    if release.get("ID") != "ubuntu" or os.geteuid() != 0:
        parser.error("Run this CI installer with sudo on Ubuntu")
    architecture = subprocess.check_output(["dpkg", "--print-architecture"], text=True).strip()
    os.environ["DEBIAN_FRONTEND"] = "noninteractive"

    def terminate(signum: int, _frame: object) -> None:
        raise SystemExit(128 + signum)

    signal.signal(signal.SIGTERM, terminate)
    try:
        install_dependencies(args.packages, release.get("VERSION_CODENAME", ""), architecture)
    except InstallError as error:
        parser.exit(1, f"[apt] ERROR: {error}\n")


if __name__ == "__main__":
    main()
