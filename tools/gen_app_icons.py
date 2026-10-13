#!/usr/bin/env python3
"""Builds the app icons in src-tauri/icons from the logo in src-tauri/icons/source.

    python3 tools/gen_app_icons.py

Needs Inkscape (renders the SVGs) and Node (runs the Tauri CLI's `icon`
command). macOS gets its own artwork, drawn on Apple's icon grid.
"""

import pathlib
import shutil
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
ICONS = ROOT / "src-tauri/icons"
TAURI_ICON = ["npx", "--yes", "@tauri-apps/cli@2.12.1", "icon"]

# Icons the installers use ("bundle.icon" in tauri.conf.json), by source.
FILES = {
    "cadenza": ["32x32.png", "128x128.png", "128x128@2x.png", "icon.png", "icon.ico"],
    "cadenza-macos": ["icon.icns"],
}


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        for name, files in FILES.items():
            png = tmp / f"{name}.png"
            svg = ICONS / "source" / f"{name}.svg"
            subprocess.run(["inkscape", str(svg), "-o", str(png), "-w", "1024", "-h", "1024"], check=True, capture_output=True)
            out = tmp / name
            subprocess.run([*TAURI_ICON, str(png), "-o", str(out)], check=True, capture_output=True)
            for f in files:
                shutil.copyfile(out / f, ICONS / f)
                print(f"{f} <- {svg.name}")


if __name__ == "__main__":
    main()
