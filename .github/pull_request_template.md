<!--
Thanks for contributing! Keep the pull request to one change. Leave CHANGELOG.md and the version
numbers alone: they're updated when a release is made.
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
