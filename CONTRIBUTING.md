# Contributing to Cadenza

Thanks for helping! Bug reports, translations, documentation and code are all welcome.

## Ways to help

- **Report a bug.** Open an issue with what you did, what you expected and what happened, plus your
  operating system and Cadenza version (**Menu > About Cadenza**). A small `.cadenza` file that shows
  the problem helps a lot.
- **Suggest a feature.** Open an issue first for anything larger than a small fix, so we can agree on
  the approach before you spend time on it.
- **Improve a translation.** Interface texts live in [`ui/locales`](ui/locales); see
  [Interface text and translations](#interface-text-and-translations).
- **Send code.** Fork the repository, make your change on a branch and open a pull request.

## Getting started

Follow [Building from source](README.md#building-from-source) to install Rust and the system
libraries. Then, from `src-tauri/`:

```sh
cargo run          # run the app
cargo test --lib   # run the tests
```

The interface is embedded into the binary at compile time, so **rebuild after changing anything in
`ui/`** (`cargo run` does this for you).

Useful while developing:

- `CADENZA_DATA_DIR=/tmp/cadenza-dev cargo run` starts Cadenza with a separate profile: its own
  settings, recent files and recovery data, and API keys kept in a file in that folder instead of your
  system keychain. It also runs alongside your normal Cadenza instead of handing files over to it.
- Builds from source never update themselves: only copies installed from a release's packages do.
  To try an update, build two versions' packages with `--debug` and the release config (see
  [Releases](#releases)), serve the newer one with a `latest.json` like a release's, and start the
  older one with `CADENZA_UPDATE_URL=http://127.0.0.1:8000/latest.json` (debug builds only).

## How the project is organized

```
src-tauri/        Rust app (Tauri 2)
  src/model.rs      the document model (deck, slides, elements)
  src/ops.rs        every editing operation, validated and applied in Rust
  src/document.rs   an open document: undo history, assets, recovery, saving
  src/render.rs     slide HTML for the editor, the show and exports
  src/lib.rs        Tauri commands, windows, presenting
  src/…             charts, maps, shapes, themes, Markdown import, export, fonts,
                    media library, secrets, settings, updates
  maps/             map chart boundaries (built by tools/gen_maps.py)
  icons/source/     the logo the app icons are made from
ui/               the interface: HTML, CSS and JavaScript modules (no build step)
  js/               Preact + htm components, one module per area
  css/              app.css (interface), content.css (slide content)
  locales/          translation catalogs
  vendor/           third-party libraries (reveal.js, KaTeX, Mermaid, …)
tools/            maintenance scripts (icons, themes, fonts, translations, checks)
```

A few principles keep Cadenza predictable. Please follow them:

1. **Rust owns the document.** The interface never edits the presentation directly; it sends an `Op`
   (see `ops.rs`) that Rust validates, applies and records for undo (consecutive edits of the same kind,
   such as typing, merge into one step).
   New editing features usually start with a new operation or a new field in `model.rs`.
2. **One renderer.** `render.rs` produces the slide HTML used by the editor canvas, the presentation
   windows and the exported web pages, so what you edit is what you present. Element styling belongs in
   `render.rs` and `ui/css/content.css`.
3. **No build step for the interface.** Modules are loaded as they are. Third-party code goes in
   `ui/vendor` together with its license.
4. **Private by default.** Features that use the network contact a service only when the person uses
   them. The only exception is the check for updates at startup, which reads the latest release's
   `latest.json` from GitHub and can be turned off. API keys go through `secrets.rs` and are never
   written into presentations.
5. **Slides don't depend on the interface language.** A presentation looks the same in every language,
   and the canvas stays left-to-right when the interface is right-to-left.

## Code style

**Rust**
- Format with `cargo fmt` (see `src-tauri/rustfmt.toml`).
- Keep `cargo clippy --all-targets` free of warnings.
- Add tests for new logic. Most modules have a `tests` section at the bottom; run them with
  `cargo test --lib`.

**JavaScript**
- 2-space indentation, single quotes, trailing commas, lines up to about 160 characters, matching the
  surrounding code.
- Run `node tools/check_js.cjs` to check every module's syntax and that each imported name is
  actually exported. A typo there would otherwise only show up as a blank window.
- Run `node --experimental-vm-modules tools/test_ui.cjs` for the document, editing and clipboard
  regression tests. They use the real UI modules with stubbed DOM and Tauri boundaries.

**CSS**
- Use the color tokens defined at the top of `app.css` so both light and dark modes work.
- Prefer logical properties (`margin-inline-start`, `inset-inline-end`, `text-align: start`) so the
  interface mirrors correctly in right-to-left languages.

## Interface text and translations

Every text a person can read goes through the translation functions in
[`ui/js/i18n.js`](ui/js/i18n.js). English text is the key:

```js
import { t, tn, N_ } from './i18n.js';

t('Save')                                  // a translated string
t('Exported {name}', { name: fileName })  // placeholders in braces
tn('{n} slide', '{n} slides', count)      // count-dependent text
const TABS = [{ label: N_('Photos') }];    // constants: mark with N_, translate with t() where shown
t('line height::Line')                     // "context::" separates identical English words
```

Keep whole sentences in one call; never build a sentence from translated pieces, because word order
differs between languages. Strings shown by Rust (errors, undo labels, layout and shape names) are
collected too; an undo label not written straight into `outcome("…")` is marked with `n_("…")`.

After changing interface text, run from the repository root:

```sh
python3 tools/i18n.py extract   # updates ui/locales/_keys.json
python3 tools/i18n.py check     # lists missing or broken entries in each language
```

Missing translations fall back to English, so a pull request doesn't have to translate new strings
into every language, but please say so in the description. To add a language, add it to `LANGUAGES` in
`ui/js/i18n.js` and create `ui/locales/<code>.json`. Plural forms use the language's
[CLDR plural categories](https://cldr.unicode.org/index/cldr-spec/plural-rules) and must include
`other`.

## Other maintenance tasks

- **Icons** come from [Lucide](https://lucide.dev). Add the icon's name to `tools/icon-list.txt`,
  download the icons (`npm pack lucide-static`, then extract the archive) and run
  `python3 tools/gen_icons.py package/icons`.
- **App icons** are made from the SVG in `src-tauri/icons/source`. After changing it, run
  `python3 tools/gen_app_icons.py` (needs Inkscape and Node.js).
- **Cadenza's built-in themes** are generated by `tools/gen_themes.py`. Keep its colors in sync with
  `src-tauri/src/themes.rs`.
- **The Google Fonts catalog** is refreshed with `python3 tools/update_google_fonts.py`.
- **Map data** (`src-tauri/maps`) is built from [Natural Earth](https://www.naturalearthdata.com)
  by `tools/gen_maps.py`; its header says which files to download. It needs Node.js (mapshaper).

## Branches and release cycles

`main` holds reviewed work for the next release. Features, fixes and documentation are developed
on short-lived branches, committed and pushed there, then merged into `main` through pull requests
when ready. There is no long-lived `dev` branch. A version tag records the exact commit shipped in
each release, so `main` can move ahead of the latest published version.

Start each change from an up-to-date `main`, using a descriptive branch name such as
`feature/slide-transitions`, `fix/pdf-backgrounds` or `docs/release-workflow`:

```sh
git switch main
git pull --ff-only origin main
git switch -c feature/slide-transitions
# Make the change and run the relevant checks.
git add <changed-files>
git commit -m "Add slide transitions"
git push -u origin feature/slide-transitions
```

Open a pull request targeting `main`. Keep subsequent commits on the same branch and push them to
update the pull request. Merge after review and passing checks; merging a feature does not publish
a release. Once merged, the branch can be deleted and the next change starts from the updated
`main`.

Every change up to a release's tagged commit is included in that release. Work intended for a later
release stays on its branch until ready to merge. Several merged pull requests can accumulate on
`main` before maintainers prepare a release.

## Pull requests

Pull requests target `main` and contain one reviewable change. Use the description to record the
user-visible result and validation; maintainers use these descriptions when assembling release notes.

- Keep each pull request focused on one change, and explain what it changes and why.
- Include a screenshot or short recording for visible changes.
- Before opening it, check that these pass:

  ```sh
  cd src-tauri && cargo fmt --check && cargo clippy --all-targets && cargo test --lib && cd ..
  node tools/check_js.cjs
  node --experimental-vm-modules tools/test_ui.cjs
  python3 tools/i18n.py check
  ```

  GitHub Actions runs them on every push to main and every pull request (`.github/workflows/checks.yml`),
  where any Clippy warning fails.

## Releases

A release collects the work merged into `main` since the previous version tag. Feature and fix pull
requests leave version numbers and `CHANGELOG.md` alone; these are updated together in a release
preparation pull request. The preparation branch is temporary, like a feature branch.

For example, to prepare `0.3.5` after `v0.3.4`:

1. Create `release/0.3.5` from the current `main`. Review the merged pull requests and
   `git log --oneline v0.3.4..main` to identify the changes being released.
2. Update `version` in `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`, along with Cadenza's
   package version in `src-tauri/Cargo.lock`. Add `## 0.3.5 — YYYY-MM-DD` to `CHANGELOG.md` with the
   release date and the collected changes.
3. Commit and push the preparation branch, then open a pull request into `main`. Run the checks
   above and test the release's changes in the Linux and Windows apps before merging it.
4. After merging and checking the resulting `main` commit, tag that exact commit and push the tag:

   ```sh
   git fetch origin
   git tag -a v0.3.5 <tested-main-commit> -m "Cadenza 0.3.5"
   git push origin v0.3.5
   ```

   Use the tested commit's SHA, even if newer work has since reached `main`. The tag fixes the release
   contents; later commits belong to a subsequent release.
5. Wait for the Release workflow to build both platforms and validate `latest.json`. Review the
   draft release's notes and installers, test the packaged apps, and publish the draft when ready.
   Start the next cycle with new branches from `main`.

Pushing the version tag triggers `.github/workflows/release.yml`. It builds the Linux and Windows
installers and attaches them to a draft GitHub release, using the matching `CHANGELOG.md` section as
its notes. The workflow stops before building if that section is missing or the tag and the two
version files differ. A manual workflow run on a branch builds artifacts for testing without creating
a release. Published version tags are kept unchanged; corrections ship under a new version.

Installed copies update themselves from the latest published release. The workflow builds with
`src-tauri/tauri.updater.conf.json`, which signs every installer and adds `latest.json` to the
release (the file installed copies read to find it). Signing needs two repository secrets,
`TAURI_SIGNING_PRIVATE_KEY` (the private key file's contents) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
The public key in `src-tauri/tauri.conf.json` must match it: installed copies only accept updates
signed with that key, so keep a backup of the private key and its password. Local builds don't
sign anything and need neither.

## License

By contributing, you agree that your contributions are released under the project's
[MIT License](LICENSE).
