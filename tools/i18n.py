#!/usr/bin/env python3
"""Interface translation tooling.

    python3 tools/i18n.py extract   # writes ui/locales/_keys.json from the sources
    python3 tools/i18n.py check     # validates every ui/locales/<code>.json against it

Keys are the English strings passed to t(), tn() and N_() in ui/js, data-i18n
attributes in ui/*.html, and static user-facing strings in src-tauri/src
(error messages, layout/shape/chart names, undo labels).
"""

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOCALES = ROOT / "ui/locales"
KEYS = LOCALES / "_keys.json"

# A JS string literal: '...', "..." or `...` (template literals without ${}).
STR = r"""('(?:[^'\\\n]|\\.)*'|"(?:[^"\\\n]|\\.)*"|`(?:[^`\\$]|\\.|\$(?!\{))*`)"""
JS_PATTERNS = [
    ("t", re.compile(r"(?<![\w.$])t\(\s*" + STR)),
    ("N_", re.compile(r"(?<![\w.$])N_\(\s*" + STR)),
]
TN = re.compile(r"(?<![\w.$])tn\(\s*" + STR + r"\s*,\s*" + STR)
HTML = re.compile(r'data-i18n(?:-[a-z]+)?="([^"]+)"')

# Rust: static strings that reach the interface.
RS_PATTERNS = [
    re.compile(r'(?:Err|ok_or|bail!|anyhow!|context)\(\s*"((?:[^"\\]|\\.)*)"\s*\)'),
    re.compile(r'Err\(\s*"((?:[^"\\]|\\.)*)"\.into\(\)\s*\)'),
    re.compile(r'outcome\(\s*"((?:[^"\\]|\\.)*)"'),
    re.compile(r'LayoutInfo \{\s*id: "[^"]*",\s*name: "((?:[^"\\]|\\.)*)"'),
    re.compile(r'label: Some\("((?:[^"\\]|\\.)*)"'),
    re.compile(r'tr\(&\w+,\s*"((?:[^"\\]|\\.)*)"'),
    # The media browser's Insert bar (browse.rs).
    re.compile(r'"((?:Insert this|Open an?) [^"]+)"'),
]


def unquote(lit: str) -> str:
    body = lit[1:-1]
    return re.sub(r"\\(.)", lambda m: {"n": "\n", "t": "\t"}.get(m.group(1), m.group(1)), body)


def rust_unescape(s: str) -> str:
    return re.sub(r"\\(.)", lambda m: {"n": "\n", "t": "\t"}.get(m.group(1), m.group(1)), s)


def extract():
    keys: dict[str, dict] = {}

    def add(key, where, one=None):
        # Skip blanks; from Rust also sentinel codes (NEEDS_KEY) and identifiers.
        if not key.strip() or (where.endswith(".rs") and re.fullmatch(r"[A-Z0-9_]+|[a-z0-9_.-]+", key)):
            return
        k = keys.setdefault(key, {"key": key, "files": []})
        if one is not None:
            k["one"] = one
        if where not in k["files"]:
            k["files"].append(where)

    for f in sorted((ROOT / "ui/js").glob("*.js")):
        if f.name in ("i18n.js", "icon-data.js"):
            continue
        src = f.read_text()
        for _, pat in JS_PATTERNS:
            for m in pat.finditer(src):
                add(unquote(m.group(1)), f.name)
        for m in TN.finditer(src):
            add(unquote(m.group(2)), f.name, one=unquote(m.group(1)))
    for f in sorted((ROOT / "ui").glob("*.html")):
        for m in HTML.finditer(f.read_text()):
            add(m.group(1), f.name)
    # Static chart and shape names are tuples ("id", "Name").
    for f in sorted((ROOT / "src-tauri/src").glob("*.rs")):
        src = f.read_text()
        src = src.split("#[cfg(test)]")[0]
        for pat in RS_PATTERNS:
            for m in pat.finditer(src):
                s = rust_unescape(m.group(1))
                # format!() arguments ({}, {:?}, {0}) are not keys; {name} placeholders are.
                if not re.search(r"\{\}|\{[:\d]|\{\{", s) and (("{" not in s) or "tr(" in m.group(0)):
                    add(s, f.name)
        if f.name in ("chart.rs", "shapes.rs"):
            for m in re.finditer(r'\(\s*"[a-z0-9-]+"\s*,\s*"([A-Z][^"]*)"\s*\)', src):
                add(m.group(1), f.name)
    out = sorted(keys.values(), key=lambda k: k["key"].lower())
    LOCALES.mkdir(parents=True, exist_ok=True)
    KEYS.write_text(json.dumps(out, ensure_ascii=False, indent=1) + "\n")
    plural = sum(1 for k in out if "one" in k)
    print(f"{len(out)} keys ({plural} plural) -> {KEYS.relative_to(ROOT)}")


PLACEHOLDER = re.compile(r"\{(\w+)\}")


def check():
    keys = json.loads(KEYS.read_text())
    ok = True
    for f in sorted(LOCALES.glob("*.json")):
        if f.name.startswith("_"):
            continue
        try:
            cat = json.loads(f.read_text())
        except json.JSONDecodeError as e:
            print(f"{f.name}: invalid JSON: {e}")
            ok = False
            continue
        missing, bad = [], []
        for k in keys:
            key = k["key"]
            v = cat.get(key)
            if v is None or v == "":
                missing.append(key)
                continue
            want = set(PLACEHOLDER.findall(key)) | (set(PLACEHOLDER.findall(k["one"])) if "one" in k else set())
            forms = v.values() if isinstance(v, dict) else [v]
            if isinstance(v, dict) and "other" not in v:
                bad.append(f"{key!r}: plural without 'other'")
            for s in forms:
                if not isinstance(s, str):
                    bad.append(f"{key!r}: non-string value")
                    continue
                got = set(PLACEHOLDER.findall(s))
                # A plural form may drop {n} (e.g. "one slide"), nothing else.
                if not got <= want or not (want - {"n"}) <= got:
                    bad.append(f"{key!r}: placeholders {sorted(got)} != {sorted(want)}")
        extra = sorted(set(cat) - {k["key"] for k in keys} - {"_language"})
        status = "OK" if not missing and not bad else "PROBLEMS"
        print(f"{f.name}: {len(cat)} entries, {len(missing)} missing, {len(bad)} bad, {len(extra)} unused — {status}")
        for b in bad[:15]:
            print("   ", b)
        for m in missing[:8]:
            print("    missing:", repr(m))
        ok = ok and not missing and not bad
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    {"extract": extract, "check": check}[sys.argv[1] if len(sys.argv) > 1 else "extract"]()
