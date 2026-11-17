# Code audit and Linux/Windows verification — 2026-11-17

`git pull --ff-only` completed on a clean `main`: the repository was already up to date at `162bc84c8ba714acde4c5705fcbd9c1bc01a3dfc`. This audit covers that revision plus the accompanying local fixes. No version, dependency, file format, commit, tag or release was changed.

The code does not currently justify a broad refactor. Document storage, operations, rendering, groups, formatting, charts, PowerPoint conversion and external services already have separate modules. The concrete maintenance problem found was duplicated document-closing logic and incomplete document-identity checks around asynchronous work. Window closing now uses the same checked operation as document closing. Native saving resolves the path and writes the validated document under one lock. This is a limited refactor tied to reproduced failures; splitting large modules further would add change without addressing a demonstrated defect.

The fixes address the following observed failures. Regression tests were run before and after the changes; the original red results remain in the local evidence directory.

| Failure | Change and verification |
| --- | --- |
| A Save As dialog could outlive its document and save the replacement document to the old destination. A late save response could also reopen an obsolete document in the UI. | Capture and check `docId` before and after asynchronous work; require the same identity in native `doc_save`. Tests cover a delayed dialog and delayed native response. |
| A stale save/discard confirmation or close response could affect a replacement document. Window closing duplicated the unsafe flow. | Validate identity throughout confirmation and closing, check it in native `doc_close`, and share `closeDocument()` with the window-close handler. Cancellation and successful closing retain their existing behavior. |
| Diagram preparation could finish after a document switch and start the replacement presentation. | Validate identity in the UI, before and after native window shutdown, and when audience/presenter windows request their data. |
| Starting a show marked unrelated editor windows as presenting. | Filter the start event by its owning editor. Verified with a regression test and two real editor windows on both systems. |
| The Last command stopped at the first slide of the final vertical stack. | Navigate to the final horizontal, vertical and fragment indices. A new browser harness exercises the bundled Reveal controller, including next, previous, first, last, goto, pause and overview. |

The final automated results below all passed. The Windows application and Rust test executable were cross-compiled for `x86_64-pc-windows-gnu` and **executed on Windows**, in a QEMU/KVM VM. This was not a Wine run. Linux used Ubuntu 26.04.1, WebKitGTK 2.52.6 and Chromium 157.0.8103.38. Windows used Windows 11 Enterprise Evaluation 10.0.26300 and Edge/WebView2 157.0.4341.60. Both used Node 24.21.0 and Python 3.14.4; Rust was 1.99.0. Native apps used isolated `CADENZA_DATA_DIR` profiles and synthetic documents.

| Check | Linux | Windows |
| --- | --- | --- |
| Rust unit tests | 273 passed | 270 passed |
| JavaScript unit tests | 168 passed | 168 passed |
| Browser editing harness | 23 passed | 23 passed |
| Browser diagrams harness | 11 passed | 11 passed |
| Browser math harness | 14 passed | 14 passed |
| Browser transition harness | 10 passed | 10 passed |
| Browser PPTX capture harness | 4 passed | 4 passed |
| Browser presentation harness | 5 passed | 5 passed |
| JavaScript syntax/import checks | 62 modules passed | 62 modules passed |
| Translation extraction tests | 4 passed | 4 passed |
| Translation catalog validation | Passed with existing missing translations allowed | Same |
| Formatting / Clippy / native build | Passed; Clippy warnings denied | Cross-target Clippy and build passed |

The three additional Linux Rust tests exercise Unix secret-file behavior and Linux media-browser compositing. They are excluded by platform configuration on Windows, rather than skipped after failure. Two artifact probes are ignored by the ordinary all-targets run because they need externally produced files; both were explicitly executed successfully against artifacts from each platform. Windows artifacts were independently reopened and reimported by those probes on Linux as well.

Additional integration checks exercised production UI modules and actual Tauri commands in both native applications. These are scripted native checks, not a claim that every mouse gesture or possible feature combination was manually exercised.

| Feature group | Native result on both systems |
| --- | --- |
| Document creation and rendering | All 22 themes, 12 layouts, 19 chart variants, 21 shape variants and 11 object kinds exercised. The combined fixture contained 64 slides and 76 objects. |
| Editing and history | Find/replace across text and notes, clipboard paste, persistent groups and transforms, cross-type format painting, crop/flip, Undo/Redo and original CSV preservation passed. Browser suites separately covered actual editable DOM, diagram authoring and MathLive behavior. |
| Design and settings | Custom theme application and theme/CSS export/import passed. Guides, settings persistence, zoom, command palette and all 16 interface languages were exercised. Changing language left the presentation unchanged. |
| Saving and reopening | Complete serialized deck equality verified after save, close and reopen, including downloaded fonts and guide positions. Native stale save/close/present requests were rejected. |
| Recovery | After writing a recovery checkpoint, only the isolated QA process was forcibly terminated. Restart and recovery restored the entire 64-slide deck exactly, including its unsaved title. Saving the recovered document cleared its dirty state. |
| Presenting | Audience and presenter windows opened with the correct document identity. Vertical navigation, Last, notes and teleprompter scrolling/settings passed. A second editor retained its own state. Saved editor windows subsequently closed through the normal close-request handler. |
| Markdown and PowerPoint import | Markdown append and Undo preserved the original deck. The independently authored LibreOffice fixture imported into a separate editor with two slides and editable text, shapes and a table, then saved successfully. |
| HTML export | Both single-file and folder exports loaded in Chromium/Edge, with 61 visible slides, loaded images and working navigation. Three intentionally skipped slides were omitted. |
| Native PDF export | WebKitGTK Print to File and WebView2 PDF export produced eight nonblank pages, each 960 × 540 pt. All pages were rendered with Poppler and visually inspected. |
| Native PPTX export | Eight mixed slides exported, reimported, saved as `.cadenza` and reopened. Text remained editable; supported tables/charts and captured appearances survived. Existing conversion warnings about layout, fonts and rasterized objects remain applicable. |
| Public online services | Openverse and Wikimedia returned search results. Roboto 400/700 downloaded and survived saving/reopening. Pexels, Pixabay and GIPHY correctly requested absent API keys. |

The dedicated PDF regression also passed its production print boot, decoded video, authored geometry and pixel checks. The PowerPoint browser/OOXML regression passed 135 assertions, and the production WebKitGTK measurement test passed 10 assertions. These complement the native artifact checks above. Initial native export fixtures contained empty title placeholders and a deliberately transparent painted shape; they were populated for the final visual checks so that blank pages could not count as sufficient export evidence.

There are explicit limits to this result. Physical camera capture, real projectors/multiple monitors, physical printers, every DPI/GPU combination, authenticated media downloads and interactive YouTube/Vimeo insertion were not exercised end to end in this run. Their applicable local parsing, rendering and state tests passed. Updater parsing, validation and recovery/reopen logic were covered by unit tests; installing an update or replacing an installed application was not performed. Installers, signing and an MSVC release build were not validated. These results do not prove an absence of bugs or exhaustive coverage of arbitrary combinations. Existing missing translations still fall back to English. PowerPoint interoperability retains the documented [conversion limits](interoperability.md).

The accompanying CI change runs JavaScript checks and all six browser harnesses on Windows, and adds the presentation harness on Linux. The corresponding local commands passed; the modified GitHub Actions workflow has not run remotely because these changes have not been pushed.

To reproduce the portable checks, run the following from the repository root after installing the prerequisites in [CONTRIBUTING.md](../CONTRIBUTING.md). Windows PowerShell can run the Node commands unchanged and replace the browser loop with the one in [checks.yml](../.github/workflows/checks.yml).

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets --locked
node tools/check_js.cjs
node --experimental-vm-modules --test tools/test_ui.cjs tools/test_data_source.cjs tools/test_diagram_visual.cjs tools/test_diagram_edits.cjs tools/test_math_edits.cjs tools/test_print_media.cjs tools/test_pptx_actions.cjs
for page in test_editing.html test_diagrams.html test_math_editor.html test_transition_preview.html test_pptx_capture.html test_present.html; do
  python3 tools/test_browser.py --page "$page" || exit 1
done
python3 tools/test_i18n.py
python3 tools/i18n.py check --allow-missing
```

The export checks write their artifacts to a chosen temporary directory. `chromium` below may be replaced with the absolute path to Chrome. The WebKit check needs the system Python GTK/WebKit bindings.

```sh
audit_dir=$(mktemp -d)
cargo run --manifest-path src-tauri/Cargo.toml --locked --example print_fixture > "$audit_dir/print-data.json"
node tools/test_print.cjs "$audit_dir/print-data.json" "$audit_dir/pdf" chromium
cargo run --manifest-path src-tauri/Cargo.toml --locked --example pptx_fixture > "$audit_dir/pptx-snapshot.json"
node tools/test_pptx_export.cjs "$audit_dir/pptx-snapshot.json" "$audit_dir/pptx" chromium
/usr/bin/python3 tools/test_pptx_webkit.py "$audit_dir/pptx-snapshot.json"
```

Local evidence is retained under `qa-results/audit-2026-11-17/` (ignored by Git). It includes baseline/red/final logs, native scenario scripts and JSON results, `environment.json`, executable hashes in `binaries.json`, artifact hashes in `artifacts-sha256.json`, saved presentations, PDFs, PPTX files and rendered contact sheets. `native-scenario.template.js` and `native-extra.template.js` contain the native assertions; platform-specific scripts provide the isolated paths. `reexport-*.js` creates the populated final export fixture. Native Windows setup and the GNU test-executable manifest follow [Windows editing verification](windows-editing-verification.md). The initial missing Linux development metadata and VM FUSE library were resolved in isolated SDK directories; neither required a product change. QA applications were closed and the initially stopped VM was shut down after verification.
