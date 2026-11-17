# Find, paint, crop and group

These editing tools are available in version 0.3.6. Each committed action uses the document's Undo/Redo history. They do not require online services.

## Find and replace

Open **Menu → Find and Replace** or press **Ctrl+F / Ctrl+H**. Search is literal, with an optional case-sensitive mode. Results show an excerpt and slide number; click a result or use the previous/next buttons to navigate.

The scope includes text boxes, text inside shapes and table cells. Speaker notes are excluded until **Include speaker notes** is checked. **Current slide** pins the search to the slide selected when that scope is chosen. Code, equations, chart labels and diagram source are not searched.

**Replace** changes the selected occurrence; **Replace All** changes all matches in the chosen scope with one Undo step. Matches can cross adjacent inline formatting spans. Replacements inherit the first matched character's markup; surrounding formatting and links remain. The panel displays up to 500 results, but Replace All includes every result. A stale replacement is rejected instead of overwriting newer text.

## Format painter

Select one object and click **Format Painter** in the toolbar, then click a destination object. **Keep applying** leaves the brush active for multiple destinations; **Escape** cancels it. **Ctrl+Alt+C** copies formatting and **Ctrl+Alt+V** applies it, including to selected characters while editing text.

The scope is explicit:

| Source | Copied appearance |
| --- | --- |
| Selected characters | Font, size, weight, italic, color, decoration and character spacing of the first selected character |
| Text box or shape text | Text/paragraph defaults and the first visible character's inline appearance |
| Any object | Fill, border, corners, shadow and padding |
| Image to image | The shared box appearance plus grayscale, blur, brightness, contrast and saturation |

Text selections do not copy box decoration or paragraph defaults. Incompatible destinations receive only supported properties. The brush stores a snapshot, so changing the source afterwards does not change the copied format. It preserves destination content, links, animations, position, dimensions, auto-height setting, media source, crop, fit and flips. Auto-height boxes can naturally become taller when their text formatting changes.

Default fills retain their appearance across object types: a text box without a background paints a transparent shape, and a shape using the theme accent paints that accent onto a text box.

## Visual image cropping

Select a loaded image and choose **Format → Image → Crop Image**, use its context menu, or double-click it. The dialog displays the unrotated, unflipped original image. Drag the selected area to pan or drag its corner handles to resize. Choose Free, 1:1, 4:3, 16:9 or Original Ratio; the ratio refers to source pixels, not normalized coordinates. Numerical percentage fields provide keyboard control.

**Apply / Enter** commits one Undo step. **Cancel / Escape** discards the preview. **Reset Crop** selects the original image; Apply removes the crop without changing the frame, flips, rotation or effects. Changing a numerical value returns to Free ratio. Replacing the source image clears its old crop.

The crop selects a source region; existing Fill, Fit and Stretch modes then fit that region into the slide's frame. Consequently, resizing the frame may hide additional source pixels in Fill mode or introduce empty space in Fit mode. The original asset is never rewritten. Save/reopen, presentations, single-file HTML and website exports use the shared renderer.

## Persistent groups

Select two or more unlocked objects and use **Menu / context menu → Group**, or **Ctrl+G**. **Ctrl+Shift+G** ungroups them. A click selects the whole group. Drag it to move, drag a corner to resize proportionally, and use its rotation handle to rotate. **Alt+click** selects a member for individual edits; double-click edits supported content directly.

Groups are flat: grouping existing groups merges their members into one group. Grouping and ungrouping leave each member's stacking order, appearance, action and animation intact, including objects interleaved with nonmembers. Reordering moves the members together while retaining their relative order. Motion paths rotate and scale with their member.

Copy/paste, duplication and slide duplication preserve groups with new identities. Cut and Delete act on the whole selected group, including when a member was selected individually; Cut captures every member before removing anything. A pending Cut removes exactly the IDs successfully copied. A group containing a locked member cannot move, resize, rotate or align until unlocked. Group-level animations and nested groups are not part of this model.

Uniform resizing stores a visual scale on each member, so fonts, borders, padding, shadows and image effects remain scaled after ungrouping. An individual geometry edit establishes a new enclosing group frame; an ordinary group move preserves its oriented pivot. Old documents default to no group, no crop and scale 1. Older Cadenza versions do not understand the new fields and should not be used to edit files relying on these features.

## Verification

The automated checks exercise the Rust operations, history, save/reopen, original-asset retention and both HTML export formats. The browser harness uses a real DOM for rich-text ranges, HTML preservation, crop geometry and scaled layout; its native transport is stubbed. Run from the repository root:

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --lib --locked
node tools/check_js.cjs
node --experimental-vm-modules tools/test_ui.cjs
python3 tools/test_browser.py
python3 tools/i18n.py check --allow-missing
python3 -m http.server 8876 --bind 127.0.0.1
```

The browser runner starts its own local server and isolated Chromium/Chrome profile and exits unsuccessfully if any check fails. CI runs it too. For interactive inspection, open `http://127.0.0.1:8876/tools/test_editing.html` with the manual server above running and require zero failures. Rust checks need the system libraries listed in CONTRIBUTING.md.

The November 2026 verification exercised the actual Linux/WebKitGTK and Windows/WebView2 apps with isolated profiles. The Windows follow-up covered multi-slide replacement with notes and a single Undo, one-time/repeated painting, group resize/rotation and individual selection, crop Apply/Undo/Redo on a flipped image, and save/reopen. It exposed and fixed the default-fill mismatch when painting between text boxes and shapes. See [Windows verification](windows-editing-verification.md) for the environment, results and reproduction notes. No release was created.
