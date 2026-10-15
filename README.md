# Cadenza

![platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20macOS%20%7C%20Windows-blue?style=flat-square&color=%237c6cff) 
![release](https://img.shields.io/github/v/release/rodolfo-viana/cadenza?style=flat-square&label=latest%20release&color=%237c6cff) ![downloads](https://img.shields.io/github/downloads/rodolfo-viana/cadenza/total?style=flat-square&color=%237c6cff)
[![release build](https://img.shields.io/github/actions/workflow/status/rodolfo-viana/cadenza/release.yml?label=release%20build&style=flat-square&color=%237c6cff)](https://github.com/rodolfo-viana/cadenza/actions/workflows/release.yml) [![codeql](https://img.shields.io/github/actions/workflow/status/rodolfo-viana/cadenza/codeql.yml?branch=main&label=codeql&style=flat-square&color=%237c6cff)](https://github.com/rodolfo-viana/cadenza/actions/workflows/codeql.yml) ![license](https://img.shields.io/github/license/rodolfo-viana/cadenza?style=flat-square&color=%237c6cff)


**Presentations, powered by reveal.js.** Cadenza is a desktop presentation app in the spirit of
Keynote and PowerPoint, built on the [reveal.js](https://revealjs.com) engine, written in Rust
with [Tauri](https://tauri.app). It runs on Linux, macOS and Windows.

![Cadenza editing a slide with a Mermaid diagram](docs/screenshot.png)

## Features

**Editing**
- Slides built from 12 layouts (title, section, two columns, quote, big number, code…), with text,
  shapes, images, video, tables, charts, code, equations and diagrams.
- Charts (column, bar, line, area, pie, donut) edited in a data grid and rendered as crisp SVG.
- Code blocks with syntax highlighting, line numbers and step-by-step line highlights.
- LaTeX math with KaTeX, as equation objects or inline in any text: `$e^{i\pi}+1=0$`.
- [Mermaid](https://mermaid.js.org) diagrams: flowcharts, sequences, timelines, mind maps and more.
- Rulers, draggable guides, smart guides and a snapping grid.
- Undo/redo for everything, a command palette (Ctrl+K, or ⌘K on a Mac) and keyboard shortcuts for all
  common actions.

**Design**
- 22 themes: 8 made for Cadenza plus the classic reveal.js themes.
- A Theme Builder to make your own and share it as a `.cadenza-theme` file (fonts included) or export
  it as reveal.js CSS.
- Google Fonts: every family is searchable offline, and the fonts you pick are embedded in the
  presentation, so it looks the same on any computer.
- Builds (step-by-step animations), slide transitions and Magic Move (reveal.js auto-animate).
- Per-presentation custom CSS for anything else.

**Media**
- A Media Library with openly licensed photos and GIFs from Openverse and Wikimedia Commons, plus
  Pexels, Pixabay and GIPHY with a free API key.
- YouTube and Vimeo videos: browse the sites as usual and click **Insert**, or paste a link. No key
  needed.

**Presenting and sharing**
- Fullscreen show with a presenter display: speaker notes, timer, clock and the next slide.
- A teleprompter in the presenter display that scrolls each slide's speaker notes as you present. Move
  and resize it anywhere, and set its speed, font, size and colors. The audience never sees it.
- With one screen, press X during the show to switch between the slides and the presenter display.
  In a video call, share only the presentation window.
- Vertical sub-slides, skipped slides and auto-advance.
- Export to a web page, either as a folder for any web server or as a single self-contained `.html`
  file, or print to PDF.
- Import reveal.js-style Markdown.
- Crash recovery for unsaved work.

**Languages**

The interface is available in Arabic, Bengali, Chinese (Simplified), Dutch, English, French, German,
Hindi, Italian, Japanese, Polish, Portuguese, Portuguese (Brazil), Russian, Spanish and
Turkish. English is the default; change it in **Settings ▸ Language**.

## Download

Installers are published on the [Releases](https://github.com/rodolfo-viana/cadenza/releases) page.

| Platform | File | Notes |
| --- | --- | --- |
| Debian, Ubuntu and derivatives | `Cadenza_x.y.z_amd64.deb` | `sudo apt install ./Cadenza_x.y.z_amd64.deb` |
| Fedora, openSUSE | `Cadenza-x.y.z-1.x86_64.rpm` | `sudo dnf install ./Cadenza-x.y.z-1.x86_64.rpm` (Fedora) or `sudo zypper install ./…` (openSUSE) |
| Any Linux | `Cadenza_x.y.z_amd64.AppImage` | Make it executable and run it; it includes everything it needs |
| macOS 12 or newer (Apple Silicon and Intel) | `Cadenza_x.y.z_universal.dmg` | Open it and drag Cadenza to **Applications** |
| Windows 10 and 11 | `Cadenza_x.y.z_x64-setup.exe` or `.msi` | Installs Microsoft WebView2 if it's missing |

The `.deb` and `.rpm` packages need WebKitGTK 4.1 (Ubuntu 22.04, Debian 12, Fedora 37 or newer).

The installers aren't code-signed yet, so the first time you run Cadenza:
- **Windows:** SmartScreen may ask you to confirm (**More info ▸ Run anyway**).
- **macOS:** macOS says it can't verify the app. Open **System Settings ▸ Privacy & Security**, scroll
  down to the message about Cadenza and click **Open Anyway**. macOS remembers this, so you only do it
  once per version.

## Privacy

Cadenza works fully offline. Online features — Google Fonts downloads, the Media Library and video
previews — contact a service only when you use them. API keys are stored in your system's credential
store (Secret Service on Linux, Keychain on macOS, Credential Manager on Windows) and are never saved
inside presentations.

## Files

- **`.cadenza`**: a presentation, stored as a zip archive with `deck.json` and the media and fonts it
  uses.
- **`.cadenza-theme`**: a theme from the Theme Builder (JSON, with its fonts embedded).

## Building from source

You need Rust 1.90 or newer ([rustup](https://rustup.rs)) and the system libraries Tauri uses. On
Debian or Ubuntu:

```sh
sudo apt install build-essential libwebkit2gtk-4.1-dev librsvg2-dev
```

On macOS, install Apple's command line tools (`xcode-select --install`). For other systems, see
Tauri's [prerequisites](https://v2.tauri.app/start/prerequisites/). Then:

```sh
git clone git@github.com:rodolfo-viana/cadenza.git
cd cadenza/src-tauri
cargo run            # start Cadenza (debug build)
cargo test --lib     # run the tests
```

The interface (`ui/`) is plain HTML, CSS and JavaScript modules with no build step; it is embedded
into the app when Rust compiles it.

To build the installers for your platform (requires Node.js 18 or newer), run from the repository
root:

```sh
npx @tauri-apps/cli@2 build
```

The packages are written to `src-tauri/target/release/bundle/`. On a Mac, add
`--target universal-apple-darwin` (after `rustup target add aarch64-apple-darwin x86_64-apple-darwin`)
to build one app for both Apple Silicon and Intel; it is written to
`src-tauri/target/universal-apple-darwin/release/bundle/`. Pushing a version tag such as `v0.2.0` makes GitHub
Actions build the Linux, macOS and Windows installers and attach them to a draft release.

## Contributing

Bug reports, translations and code are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for how the
project is organized and how to send changes.

## License

Cadenza is released under the [MIT License](LICENSE). It includes reveal.js, KaTeX, Mermaid,
highlight.js, Preact, htm and Lucide icons, each under its own license; see the `LICENSE` files in
[`ui/vendor`](ui/vendor) and [`ui/vendor/licenses`](ui/vendor/licenses).
