These synthetic fixtures exercise the actual presentation renderer and print
pipeline without external media or user data. `print-crop.svg` and `print-crop.png` have four solid
quadrants; cropping its left half must exclude blue/yellow pixels in the PDF.
`print.webm` is a 640 × 360 solid blue video that must print at its authored
768 × 432 size. It was generated with:

```sh
ffmpeg -f lavfi -i color=c=0x36a3da:s=640x360:r=10 -t 0.3 -c:v libvpx-vp9 -an print.webm
```

Run the fast failure-path tests and the full rendering regression:

```sh
node --test tools/test_print_media.cjs
cargo run --manifest-path src-tauri/Cargo.toml --example print_fixture > /tmp/print-data.json
node tools/test_print.cjs /tmp/print-data.json /tmp/cadenza-print-results
```

The full test needs the project's Rust build dependencies, Node 22 or newer,
Chromium and `pdftoppm` (Poppler). Pass a Chromium executable as the last
argument if it is not on PATH. It uses a fresh headless profile, checks real
WebM decoding and CSS geometry, then rasterizes the resulting PDF to verify
crop boundaries, Contain/Fill, flip, grayscale, rounded corners and video
placement. Crops appear on page 1 and intermediate pages (including page 8):
first/last-page-only tests hide Chromium’s pagination defect. `geometry.json`, `print.pdf` and page
rasters are retained in the result directory. Native WebView2/WebKit exports
still require verification because their print engines may differ.

The PPTX mapper test uses `pptx_fixture.rs` to serialize a complete Rust model
and the production editor HTML. It runs the browser bundle in an isolated
Chromium profile, then inspects the actual OOXML ZIP and its chart workbook:

```sh
cargo run --manifest-path src-tauri/Cargo.toml --example pptx_fixture > /tmp/pptx-snapshot.json
node tools/test_pptx_export.cjs /tmp/pptx-snapshot.json /tmp/cadenza-pptx-results
soffice --headless --convert-to pdf --outdir /tmp/cadenza-pptx-results /tmp/cadenza-pptx-results/export.pptx
```

Assertions cover editable rich text, lists, opacity, scaling, tables, charts,
numeric zero versus missing data, cropped pictures, safe hyperlinks and notes;
also full-slide CSS fallback, hidden slides, cancellation, failed captures and
disposal. Capture images are deterministic stubs of the native API contract.
This test verifies conversion structure and preserves a PPTX for opening in
an independent viewer; native screenshot fidelity is tested separately on
Windows WebView2 and Linux WebKit. No test reads the active document or starts
a camera.
