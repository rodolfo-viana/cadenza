#!/usr/bin/env python3
"""Refreshes the bundled Google Fonts catalog (ui/data/google-fonts.json).

The catalog lets the font picker list every Google font offline. Fonts are
only downloaded (and embedded in the presentation) when a user picks one.

    python3 tools/update_google_fonts.py            # download the catalog
    python3 tools/update_google_fonts.py meta.json  # use a saved copy

Output format: [[family, category, "400,400i,700,..."], ...] sorted by
popularity. Categories: s = sans-serif, f = serif, d = display,
h = handwriting, m = monospace.
"""

import json
import pathlib
import sys
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "ui/data/google-fonts.json"
URL = "https://fonts.google.com/metadata/fonts"
CATEGORIES = {"Sans Serif": "s", "Serif": "f", "Display": "d", "Handwriting": "h", "Monospace": "m"}


def load(source: str | None) -> dict:
    if source:
        return json.loads(pathlib.Path(source).read_text(encoding="utf-8"))
    req = urllib.request.Request(URL, headers={"User-Agent": "Mozilla/5.0 (Cadenza font catalog updater)"})
    with urllib.request.urlopen(req, timeout=60) as r:
        text = r.read().decode("utf-8")
    # The endpoint may prefix the JSON with an anti-XSSI guard.
    return json.loads(text[text.index("{"):])


def variant_key(v: str) -> tuple[int, int]:
    return (1 if v.endswith("i") else 0, int(v.rstrip("i")))


def main() -> None:
    meta = load(sys.argv[1] if len(sys.argv) > 1 else None)
    rows = []
    for f in sorted(meta["familyMetadataList"], key=lambda f: f.get("popularity", 99999)):
        if f.get("colorCapabilities") or "latin" not in f.get("subsets", []):
            continue
        variants = sorted((f.get("fonts") or {"400": {}}).keys(), key=variant_key)
        rows.append([f["family"], CATEGORIES.get(f.get("category"), "s"), ",".join(variants)])
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(rows, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
    print(f"wrote {len(rows)} families to {OUT}")


if __name__ == "__main__":
    main()
