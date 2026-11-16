# PowerPoint and source data

Keep a `.cadenza` copy when you need to continue editing in Cadenza. It stores the original CSV,
Mermaid and LaTeX sources, editable objects and presentation behavior. PowerPoint conversion
supports a subset of those features; it does not preserve arbitrary PowerPoint XML for a later
round trip.

## Importing and exporting PowerPoint

Choose **Import PowerPoint…** or **Export PowerPoint…** from the app menu or command palette.
Import creates an unsaved Cadenza presentation and leaves the original `.pptx` unchanged. Review
the conversion report before using the result. Import warnings also remain in the saved
`.cadenza` document.

The supported representations differ in each direction:

| Content | Import into Cadenza | Export to PowerPoint |
| --- | --- | --- |
| Text and shapes | Simple paragraphs, runs, lists and supported preset shapes remain editable. Supported layout, master and theme properties are resolved. | Simple rich text and supported geometric shapes remain editable. Unsupported appearance is rendered as an image. |
| Images | Supported embedded raster images retain representable crop, flip, opacity and rotation. External images are not downloaded. | PNG/JPEG images with supported properties remain editable pictures, including crop and flip. Other image appearances use a rendered PNG. |
| Tables | Unmerged cells remain editable; Cadenza applies its grid formatting. | Supported tables remain editable. Rotation, decoration or other unsupported appearance uses an image. |
| Charts | Supported basic charts use their literal or cached values. Missing values stay missing; embedded spreadsheets are not evaluated. | Supported basic charts carry the displayed data and remain editable. More advanced charts or styling use an image. |
| Equations and diagrams | Unsupported OMML equations, SmartArt and custom geometry use an existing supported preview when available, otherwise a visible placeholder. | LaTeX equations and Mermaid diagrams become images; their original editable source stays in `.cadenza`. |
| Notes and links | Speaker notes and supported links are retained. | Speaker notes and supported web/slide links are retained where the output representation allows them. |
| Groups | Representable transforms are applied to individual objects; group membership is flattened. | Objects keep their placement; Cadenza group membership is not transferred. |
| Animation and media | Unsupported objects receive a preview or placeholder. PowerPoint transitions and timing are not recreated. | Slides are static. Cadenza transitions, builds and timing are omitted; media uses its rendered appearance and live camera content is omitted. |

The conversion report identifies omitted, approximated, rasterized and placeholder content. An
unsupported import cannot acquire a faithful preview unless the PowerPoint package actually
contains one that Cadenza can read. A rendered export preserves appearance at its capture
resolution, but its content is no longer independently editable in PowerPoint. Presentations
with custom CSS export as full-slide images.

Fonts are referenced by name in PowerPoint exports, not embedded. PowerPoint may substitute
missing fonts and lay out editable text, cells and chart axes differently. PowerPoint embedded
font programs are not imported. Review the converted file in the application that will present
it, particularly when exact line wrapping matters.

## Editing original sources

Select a chart or table to see its stored CSV in the inspector. This is provenance stored inside
the presentation, not a live connection to a spreadsheet application. Editing the current data
does not overwrite the original CSV. Reapplying an edited source is an explicit operation with
Undo support. Older objects and imported PowerPoint charts can have no source CSV; Cadenza does
not invent one. PowerPoint export carries the displayed chart/table values, not this original
source record.

Diagrams have Visual and Mermaid tabs. The visual editor supports a bounded set of flowchart
blocks and connections. Opening a tab or browsing templates does not rewrite the source; a
template replaces it only when chosen. Unsupported Mermaid remains editable in the source tab.
If the source is invalid, Cadenza retains it and marks the last rendered preview as outdated.

Equation objects have Visual and LaTeX tabs. MathLive provides visual entry and an on-screen
keyboard; KaTeX renders the slide. Switching tabs leaves the saved LaTeX unchanged. Source that
cannot be edited visually remains available in the LaTeX tab. Both libraries and their fonts are
bundled for offline use.

The transition panel configures and replays the selected slide's incoming transition. Its preview
uses the previous visible slide and keeps media paused. The first visible slide has no preceding
slide for this preview. Use a Cadenza presentation or HTML export when you need presentation
behavior that a static PowerPoint/PDF conversion cannot retain.

## Verification

The automated checks cover the document operations, real browser editors, conversion reports,
OOXML output and PDF rendering. See [CONTRIBUTING](../CONTRIBUTING.md#pull-requests) for commands
and [PowerPoint fixtures](../tools/fixtures/pptx/README.md) for the import corpus. Headless Chromium
checks and Windows compilation do not replace tests of the running native WebKitGTK and
WebView2 applications. No claim of full PowerPoint or Google Slides round-trip fidelity is made.
