<!--
Thanks for contributing! Open the pull request from your feature, fix or documentation branch into
main, and keep it to one change. Leave CHANGELOG.md and version numbers alone unless this is the
release preparation pull request. Explain the user-visible result below so it can be included in
the next release's notes. See CONTRIBUTING.md for the branch and release cycle.
-->

## What this changes

<!-- What a person using Cadenza will notice, or what changes in the code. -->

## Why

<!-- The problem it solves. Link the issue, e.g. "Fixes #123". -->

## How it was tested

- [ ] In `src-tauri/`: `cargo fmt --check`, `cargo clippy --all-targets` (no warnings) and `cargo test --lib`
- [ ] `node tools/check_js.cjs` and `node --experimental-vm-modules tools/test_ui.cjs`
- [ ] `python3 tools/i18n.py check`
- [ ] Tried in the app on: <!-- Linux (which desktop and distribution) / Windows -->

<!-- Tests added for the change, if any, and anything you couldn't test. -->

## Interface text

- [ ] No new or changed text
- [ ] New text goes through `t()`, `tn()` or `N_()` (in Rust: a static error message, or `n_()` for undo labels), and I ran `python3 tools/i18n.py extract`
- [ ] Translated into: <!-- the languages, or "none yet": untranslated text shows in English -->

## Screenshots

<!-- For visible changes: a screenshot or a short recording, before and after if it helps. -->
