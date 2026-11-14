# Theme Builder

Open **Theme Builder** from the command palette or customize a theme in the Document inspector. Its live preview uses the same renderer as the presentation and HTML exports. The Paragraphs and Lists sections bring their respective sample slides to the top of the preview.

The builder groups settings by purpose:

- **Theme:** name, author, light/dark base, background and gradient.
- **Typography:** heading/body fonts and weights, heading line spacing, tracking and uppercase. Text box font sizes still come from the layout or Format panel; the reveal.js base size controls ordinary reveal.js slide content.
- **Paragraphs:** alignment, line spacing, space before and after, left/right indents and first-line indent. A negative first-line indent creates a hanging indent.
- **Lists:** nine independent levels for both bulleted and numbered lists, including mixed nesting. Choose a bullet, numbering style, indent, item spacing, marker color and marker size. Classic, Outline and Minimal presets change marker styles while preserving spacing and colors.
- **Colors:** heading/body/link colors, accent and chart/shape palette.

Distances in `em` follow the text size: `0.5em` is half the current font size. List indentation is relative to the parent level. An empty item-spacing field means automatic spacing (half the text box's paragraph spacing); zero means no gap. An empty marker color follows the text color.

## Existing presentations

Text boxes can inherit paragraph alignment, line spacing and paragraph spacing, or set their own values in **Format**. Newly created text follows the theme except where a layout deliberately sets an alignment, such as a centered title. Explicit values from older presentations—including left alignment and zero spacing—remain overrides.

To adopt the theme across an existing deck, enable **Use theme paragraph settings in existing text boxes** before applying. This resets those three overrides in every text box on every slide. Fonts, sizes, inline formatting, shape labels and text content stay. Applying the theme and resetting overrides form one Undo step. For a selected box, use **Use theme paragraph settings** in the Format panel.

Paragraph indents and before/after spacing apply to paragraph blocks. List indentation and markers use their own per-level settings. Explicit inline HTML formatting and numbered lists with an HTML `type` attribute retain their formatting.

## Saving and sharing

**Save to My Themes** adds the theme to the local gallery. **Apply to Presentation** saves it and applies it to the open deck. **Export** offers a portable `.cadenza-theme` file with fonts or a reveal.js CSS file. Importing an older theme supplies the new settings with compatible defaults.

Changes to a theme in the library do not silently change other saved presentations; apply the theme to each deck that should use it. New theme settings remain stored in the presentation and are included in standalone HTML and website exports. Older Cadenza versions can open the files but will not render customization they do not support.
