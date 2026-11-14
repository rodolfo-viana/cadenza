# Windows editing verification — 2026-11-14

The four editing tools were tested in the native Cadenza application on Windows 11 Enterprise Evaluation, build 26300, using WebView2 157.0.4341.46. This was a local QEMU/KVM VM with an isolated document and `CADENZA_DATA_DIR`; no user presentations or installed Cadenza copy were replaced. The application was built from the current source for `x86_64-pc-windows-gnu` and run on Windows, rather than under Wine. No release, tag or version change was made.

## Results

The checks covered the document operations, real browser layout and native editing gestures. Each result below describes what was exercised, rather than inferring Windows compatibility from a Linux build.

| Check | Result |
| --- | --- |
| Native Rust unit suite | 231 passed, 0 failed, including history, saved-document reloading, original asset retention and single-file/folder HTML exports |
| Browser regression harness | 19 passed, 0 failed in Windows Edge 157.0.4341.46 and Linux Chromium; native document transport is stubbed in this harness |
| JavaScript unit suite | 73 passed, 0 failed on the Linux host; syntax check passed for all 41 modules |
| Find/replace in the Windows app | Four occurrences across two slides and speaker notes replaced together; one Undo restored all four and their rich-text links |
| Format painter in the Windows app | Individual group member used as source; one-time and Keep applying modes, Escape, application to shape text, Undo/Redo and saving verified |
| Groups in the Windows app | Proportional resizing, rotation, Undo/Redo, Alt-click member selection and persistence verified |
| Crop in the Windows app | Original source preview, Reset Crop, 1:1 source-pixel ratio, dragging the region, Apply, Undo/Redo and horizontal flip verified |
| Close/reopen | Group transform, crop and painted text retained after closing the application and reopening the saved presentation |
| Native Windows PDF export | Saved through WebView2; both pages rendered and inspected, with the same transformed text, transparent painted shape, cropped/flipped image and slide clipping as the editor; 960 × 540 pt per page |

The saved ZIP was inspected independently of the UI. Both group members retained the same group frame and visual scale; the crop's source-pixel width equalled its height; image flip and frame geometry were preserved. Painting retained the destination text, position, size, rotation, actions and builds. The embedded SVG was byte-identical to its original, and the existing hyperlink remained present.

## Fixes found during this follow-up

The native Windows test exposed a cross-type painter mismatch: an empty fill means no background on a text box, but the theme accent on a shape. Copying the empty value literally introduced a colored fill on the destination. The painter now resolves that source default before applying it. A regression test checks rendered SVG transparency and the reverse transfer of a theme accent to a text box. The rebuilt Windows application was checked visually and its saved document contains the explicit transparent fill.

GitHub code scanning also reported an unguarded recursive property merge in the browser test transport, alert [#14](https://github.com/rodolfo-viana/cadenza/security/code-scanning/14). Commit `a20f5ee` rejects prototype-related keys at every depth and avoids merging into inherited objects. The regression covers all three reserved keys, top-level and nested, plus an inherited destination. GitHub subsequently marked the alert **fixed**.

## Reproduction notes

Use the commands in [Editing tools](editing-tools.md#verification) for the host checks. For native Windows execution, build the application and unit-test executable for Windows, copy the executable and `WebView2Loader.dll` into an isolated test directory, and set `CADENZA_DATA_DIR` before launching the app. The Windows WebView2 runtime must be installed.

This run cross-compiled the application with `cargo build --target x86_64-pc-windows-gnu --locked`. The GNU unit-test executable needed an embedded `Microsoft.Windows.Common-Controls` version 6 manifest; this is normally present in the application's Tauri resources but was absent from the standalone test harness. The test binary was linked using `cargo rustc --target x86_64-pc-windows-gnu --lib --profile test --locked -- -C link-arg=/path/to/tests-resource.o`, where the resource object contains that manifest. The export test also needs the repository's `ui/` files at the source path baked into `CARGO_MANIFEST_DIR`; those resources were copied into the VM before running the complete suite.

For browser checks on Windows, serve the repository over HTTP and run Edge with a fresh profile, `--headless --disable-gpu --virtual-time-budget=10000 --dump-dom`, pointing to `tools/test_editing.html`. Require `19 passed; 0 failed` and `data-result="pass"`. This complements the native UI checks; it does not replace them.

For manual reproduction, use two slides containing rich text and a link, speaker notes, a shape and an image with four visibly distinct quadrants. Repeat the operations in the results table, save, close the application, reopen, and inspect the saved ZIP and exported presentation. The test fixture deliberately allows objects to overlap or extend beyond the slide when transforms are tested; export should preserve the same clipping as the editor.
