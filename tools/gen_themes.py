#!/usr/bin/env python3
"""Generates Cadenza's reveal.js themes (ui/themes/cadenza-*.css).

The themes reuse the rule set of reveal.js' own `simple` theme (so every
reveal.js feature keeps working) and only swap the CSS variables, fonts and
backgrounds. Edit THEMES below and re-run:

    python3 tools/gen_themes.py

Keep the colors in sync with `src-tauri/src/themes.rs` (used for previews,
chart palettes and theme-aware defaults).
"""

import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
TEMPLATE = ROOT / "ui/vendor/reveal/theme/simple.css"
OUT = ROOT / "ui/themes"

SANS = "Inter, 'Segoe UI', Cantarell, Ubuntu, Roboto, 'Helvetica Neue', Arial, sans-serif"
SERIF = "'Source Serif 4', 'Source Serif Pro', Georgia, 'DejaVu Serif', 'Times New Roman', serif"
MONO = "'JetBrains Mono', 'Fira Code', 'Cascadia Code', 'DejaVu Sans Mono', Menlo, Consolas, monospace"

THEMES = {
    "cadenza-midnight": dict(
        dark=True, background="linear-gradient(160deg, #111827 0%, #0b1120 100%)", bg="#0b1120",
        main_font=SANS, main_color="#cbd5e1", heading_font=SANS, heading_color="#ffffff",
        heading_weight=700, letter_spacing="-0.02em", link="#a5b4fc", link_dark="#818cf8", selection="#6366f1",
    ),
    "cadenza-aurora": dict(
        dark=True,
        background="radial-gradient(circle at 15% 20%, rgba(91,33,182,0.55) 0%, rgba(91,33,182,0) 45%), "
                   "radial-gradient(circle at 85% 80%, rgba(14,116,144,0.6) 0%, rgba(14,116,144,0) 50%), #0a0f1f",
        bg="#0a0f1f", main_font=SANS, main_color="#dbe4f3", heading_font=SANS, heading_color="#ffffff",
        heading_weight=800, letter_spacing="-0.025em", link="#67e8f9", link_dark="#22d3ee", selection="#0891b2",
    ),
    "cadenza-paper": dict(
        dark=False, background="#faf7f2", bg="#faf7f2", main_font=SANS, main_color="#44403c",
        heading_font=SERIF, heading_color="#1c1917", heading_weight=600, letter_spacing="-0.01em",
        link="#c2410c", link_dark="#9a3412", selection="#fdba74",
    ),
    "cadenza-slate": dict(
        dark=False, background="#ffffff", bg="#ffffff", main_font=SANS, main_color="#334155",
        heading_font=SANS, heading_color="#0f172a", heading_weight=700, letter_spacing="-0.02em",
        link="#2563eb", link_dark="#1d4ed8", selection="#93c5fd",
    ),
    "cadenza-ember": dict(
        dark=True, background="linear-gradient(160deg, #1c1917 0%, #0c0a09 100%)", bg="#0c0a09",
        main_font=SANS, main_color="#d6d3d1", heading_font=SANS, heading_color="#fafaf9",
        heading_weight=800, letter_spacing="-0.02em", link="#fb923c", link_dark="#f97316", selection="#c2410c",
    ),
    "cadenza-forest": dict(
        dark=True, background="linear-gradient(160deg, #0f2a1f 0%, #071510 100%)", bg="#071510",
        main_font=SANS, main_color="#c7e3d4", heading_font=SERIF, heading_color="#ecfdf5",
        heading_weight=600, letter_spacing="-0.01em", link="#4ade80", link_dark="#22c55e", selection="#15803d",
    ),
    "cadenza-mono": dict(
        dark=False, background="#f5f5f4", bg="#f5f5f4", main_font=SANS, main_color="#262626",
        heading_font=MONO, heading_color="#0a0a0a", heading_weight=700, letter_spacing="-0.03em",
        link="#0a0a0a", link_dark="#000000", selection="#d4d4d4",
    ),
    "cadenza-sunrise": dict(
        dark=False, background="linear-gradient(135deg, #fff1eb 0%, #ace0f9 100%)", bg="#f4f1f8",
        main_font=SANS, main_color="#312e81", heading_font=SANS, heading_color="#1e1b4b",
        heading_weight=800, letter_spacing="-0.025em", link="#e11d48", link_dark="#be123c", selection="#fda4af",
    ),
}


def build(name: str, t: dict, template: str) -> str:
    css = re.sub(r"@import[^;]+;", "", template)
    root = (
        ":root{"
        f"--r-background:{t['bg']};--r-background-color:{t['bg']};"
        f"--r-main-font:{t['main_font']};--r-main-font-size:34px;--r-main-color:{t['main_color']};"
        "--r-block-margin:20px;--r-heading-margin:0 0 20px 0;"
        f"--r-heading-font:{t['heading_font']};--r-heading-color:{t['heading_color']};"
        f"--r-heading-line-height:1.12;--r-heading-letter-spacing:{t['letter_spacing']};"
        "--r-heading-text-transform:none;--r-heading-text-shadow:none;"
        f"--r-heading-font-weight:{t['heading_weight']};--r-heading1-text-shadow:none;"
        "--r-heading1-size:2.5em;--r-heading2-size:1.6em;--r-heading3-size:1.3em;--r-heading4-size:1em;"
        f"--r-code-font:{MONO};"
        f"--r-link-color:{t['link']};--r-link-color-dark:{t['link_dark']};--r-link-color-hover:{t['link']};"
        f"--r-selection-background-color:{t['selection']};--r-selection-color:#fff;"
        "--r-overlay-element-bg-color:240, 240, 240;--r-overlay-element-fg-color:0, 0, 0}"
    )
    css = re.sub(r":root\{[^}]*\}", root, css, count=1)
    css = re.sub(
        r"\.reveal-viewport\{[^}]*\}",
        f".reveal-viewport{{background:{t['background']};background-color:{t['bg']}}}",
        css,
        count=1,
    )
    # Light themes fix text on dark backgrounds and vice versa.
    css = re.sub(r"section\.has-(dark|light)-background[^{]*\{[^}]*\}\s*$", "", css.strip())
    if t["dark"]:
        css += (
            "\nsection.has-light-background,section.has-light-background h1,section.has-light-background h2,"
            "section.has-light-background h3,section.has-light-background h4,section.has-light-background h5,"
            "section.has-light-background h6{color:#1f2937}"
        )
    else:
        css += (
            "\nsection.has-dark-background,section.has-dark-background h1,section.has-dark-background h2,"
            "section.has-dark-background h3,section.has-dark-background h4,section.has-dark-background h5,"
            "section.has-dark-background h6{color:#fff}"
        )
    header = f"/* Cadenza theme: {name}. Generated by tools/gen_themes.py — edit there. */\n"
    return header + css + "\n"


def main() -> None:
    template = TEMPLATE.read_text(encoding="utf-8")
    OUT.mkdir(parents=True, exist_ok=True)
    for name, t in THEMES.items():
        (OUT / f"{name}.css").write_text(build(name, t, template), encoding="utf-8")
        print("wrote", OUT / f"{name}.css")


if __name__ == "__main__":
    main()
