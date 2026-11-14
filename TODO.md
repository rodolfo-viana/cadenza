# Next improvements

Planning checklist for the next working session, following the 0.3.5 Theme Builder work. These are the features selected for follow-up; the current implementation is noted where it already covers part of the request.

## Editable slide masters and layouts

Cadenza currently creates slides from 12 built-in layouts. A slide remembers its original layout as information, but does not inherit objects or geometry from an editable master.

- [ ] Create, edit, duplicate and reuse masters and custom layouts, with text/image placeholders and shared elements such as logos and footers.
- [ ] Define inheritance for position, dimensions and styles, with explicit per-slide overrides and a way to restore inherited values.
- [ ] Apply layout changes to existing slides while preserving their content and intentional overrides.
- [ ] Include reusable layouts in theme/presentation files and preserve compatibility with existing files.
- [ ] Verify that changing a shared logo or title position updates the intended slides, survives saving/reopening, works in exports and can be undone.

## Format painter

Copying a style should be faster than configuring each property again in the Format inspector.

- [ ] Copy and apply text or object formatting without replacing content, geometry, links or animations.
- [ ] Define clearly which properties are copied for text selections, text boxes and other object types.
- [ ] Support one-time application and repeated application, with a visible active state and Escape to cancel.
- [ ] Make each application undoable and accessible through the keyboard.

## Find and replace

The command palette can find commands and slides, but there is no presentation-wide text replacement workflow.

- [ ] Find text across slides, show matches and navigate to the matching slide and text box.
- [ ] Replace one match or all matches, preserving surrounding rich-text formatting.
- [ ] Provide case matching and clear scope controls; decide whether speaker notes are included.
- [ ] Show the number of replacements and make Replace All a single Undo step.

## Persistent object groups

Multiple selection already allows joint editing, but the document has no persistent group of objects.

- [ ] Group and ungroup objects while preserving their positions, styles and relative stacking order.
- [ ] Move, resize and rotate a group; define how to select and edit an individual member.
- [ ] Preserve groups through duplication, copy/paste, saving/reopening and exports.
- [ ] Specify interactions with locking, animation and stacking; start with a clear, bounded grouping model.

## Comments

Add a review workflow that works with Cadenza's local presentation files.

- [ ] Anchor comments to slides or objects, with author, timestamp, replies and resolve/reopen actions.
- [ ] Store comments in the presentation and preserve them when sharing the editable file.
- [ ] Keep comments out of the audience view and presentation exports.
- [ ] Handle deletion of a commented object explicitly so feedback is not silently lost.

## Persistent version history

Undo/Redo and crash recovery already exist. The missing capability is browsing and restoring earlier saved versions after reopening a presentation.

- [ ] Add named checkpoints and a history view with timestamps and previews.
- [ ] Restore an earlier version as a new current version while retaining the state it replaces.
- [ ] Preserve the media and fonts referenced by each retained version.
- [ ] Decide storage location, portability, retention and size limits before implementation.

## Visual image cropping and framing

Images currently have Fill, Fit and Stretch modes, plus numerical X/Y focus controls. The next step is direct manipulation on the slide.

- [ ] Drag crop handles and reposition the image inside its frame, with a visible preview of the original image.
- [ ] Support aspect-ratio constraints and resetting the crop without modifying the original asset.
- [ ] Keep existing image settings compatible and preserve the crop in saved files and exports.
- [ ] Verify the interaction on Linux and Windows, including Undo/Redo and keyboard access.

## Stacking order: bring forward and send backward

This feature already exists: **Bring to Front**, **Bring Forward**, **Send Backward** and **Send to Back** are in the command registry, canvas context menu and Format inspector. The follow-up is to make these controls easier to discover and use.

- [ ] Review placement, icons and labels so the difference between moving one layer and moving to the front/back is clear.
- [ ] Evaluate an object/layers list for selecting obscured elements and seeing their order.
- [ ] Verify multiple selections, relative order, locking and Undo/Redo, and integrate the behavior with persistent groups.
- [ ] Document the menu paths and keyboard shortcuts.

## Verification for each implementation

Use the existing Rust operations and shared renderer so editing, presenting and exporting remain consistent.

- [ ] Cover changed behavior with focused tests, including old-file compatibility where the document model changes.
- [ ] Exercise the actual app on Linux and the Windows VM, including save/reopen and Undo/Redo.
- [ ] Update interface translations, documentation and relevant screenshots.
