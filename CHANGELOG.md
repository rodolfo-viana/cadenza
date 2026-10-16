# Changelog

Notable changes to Cadenza. Each version's section is also shown on its GitHub release page.

## 0.3.2 — 2026-10-15

Fixes for Windows: starting a show or printing no longer freezes Cadenza, and PDFs come out a slide per page. Plus fixes to charts, equations and editing.

### Changes

- On Windows, the print window's Save PDF… writes a PDF with one slide per page, without margins or browser headers. Print… still opens the print dialog.
- Printouts and PDFs show each video's first frame and the picture of YouTube videos, instead of black or empty boxes.
- On small screens or with large text scaling, Cadenza's windows open maximized, so nothing hides behind the taskbar.

### Fixes

- On Windows, Play, Print / Export PDF and New Window froze the whole app.
- Starting a show again, or printing again with the print window still open, failed with an error.
- On Windows with one screen, the presenter display covered the slides when a show started.
- After clicking a YouTube or Vimeo video during a show, the arrow keys and Escape stopped working.
- On Windows, the editor didn't get the keyboard back after the YouTube, Vimeo or GIPHY browser closed.
- Ctrl+S while typing in a chart's data table left out the cell being typed.
- Turning a Sankey diagram or a network graph into another chart type failed.
- Changing the chart type reset the colors picked for its series.
- A donut chart with a single value was drawn as a full disc instead of a ring.
- Escape in an inspector field applied what was typed instead of cancelling it.
- Clicking into a number field didn't select the number, so typing added to it.
- Text with inline math stayed as LaTeX source after clicking into it and out again without changes.
- Changing the slide size didn't scale motion paths or the font sizes of parts of a text.
- The default title of new presentations, the Theme Builder's preview and the name of a customized theme are now in the interface language.

## 0.3.1 — 2026-10-15

Ten new chart types plus maps of the world, continents and countries, charts that build one series at a time, motion paths, and the presenter's live camera on a slide.

### New

- New chart types: stacked column, stacked bar, streamgraph, treemap, parliament, scatterplot, heatmap, candlestick, Sankey diagram and network graph. Sankey diagrams and network graphs take a list of links (from, to, value).
- Map charts: the world, a continent, or a country's states or provinces, colored by value. Rows name regions in any of Cadenza's languages or by code, Add All Regions fills in the table, and faraway parts (Alaska, the Canary Islands…) get boxes of their own. Map data comes from Natural Earth.
- Charts can build one series at a time (pie and donut: one slice at a time; parliament: one party at a time).
- Motion paths: Animate ▸ Motion Path ▸ Draw Path, then click points on the slide. The object moves along the path at its build step, in straight lines or a smooth curve.
- Live camera (Media ▸ Live Camera): the presenter's camera on a slide, mirrored or not, as a rectangle or a circle. Only the audience window shows it, and only while you present in Cadenza; exported presentations and printouts leave it out.
- Four Wipe build effects.
- Images from a link: paste the address of an image, a Wikimedia Commons or Wikipedia file, or a web page in the photo search of the Media Library. Wikimedia files bring their author and license for the photo credit.
- Ctrl+B, Ctrl+I and Ctrl+U work on selected text boxes too, not only on text being edited.
- The About dialog shows who makes Cadenza and links to its repository.

### Changes

- With some text selected, Font, Size and Color change only that text, like Bold and Italic. Select the box to change all of its text.
- Stacked column, bar and area charts are chart types of their own instead of a switch. Stacked charts in older presentations stay stacked.

### Fixes

- Bold and Italic on a selected text box couldn't turn off the bold of titles and headings or the italic of quotes.
- Ctrl+U didn't underline text while editing it.
- After turning a list off, the text no longer followed font changes to its box.
- Saving or closing right after typing in an inspector field (an equation, a size) left out what was just typed, and closing didn't ask about it.
- Importing Markdown into a one-slide presentation with an image, chart or other content replaced it without asking.
- On Linux, PDF export put each slide at the top of an A4 portrait page, black below. Pages now have the size of a slide.
- Magic Move did nothing unless the slide before it also had Magic Move.
- Downloads from some websites (images, Google Fonts) waited 15 seconds or more before starting.
- Photo credits from Wikimedia Commons ran the author's lines together.

## 0.3.0 — 2026-10-14

Videos now play in presentations (YouTube, videos from your computer and slide backgrounds), and the Media Library has free photos from Wikimedia Commons, Pexels and Pixabay.

### New

- Photos from Wikimedia Commons in the Media Library, with no key needed, and from Pexels and Pixabay with a free API key (Settings ▸ Integrations). Turn on "Add photo credit" to add the credit line each license asks for.

### Changes

- Unsplash is no longer in the Media Library: it now blocks downloads from apps. Photos you already inserted stay in your presentations.
- The Linux AppImage now includes GStreamer, so videos and audio play in it.

### Fixes

- YouTube videos showed "Error 153" instead of playing, during presentations and in exported presentations opened from disk.
- On Linux, videos from your computer didn't play during presentations, and the editor showed no preview frame for them.
- Slide background images and videos didn't appear during presentations.
- On Linux with NVIDIA graphics, Browse YouTube closed with "YouTube closed unexpectedly" before a video could be inserted.
- Many GIFs found through Openverse showed a broken thumbnail.
- The canvas flickered while dragging an element to the center of the slide.
- Updated two libraries (time and serde_with) to fix security advisories.

## 0.2.1 — 2026-10-14

Cadenza now has a teleprompter in the presenter display, and with one screen you can switch between the slides and the presenter display during a video call.

### New

- A teleprompter in the presenter display. It scrolls the current slide's speaker notes and moves on to the next slide's notes when you advance. Drag it anywhere, resize it, and set its speed, font, text size, line spacing, colors and transparency. It can also mirror the text for teleprompter glass and show a reading line. Turn it on in the Play menu or in Settings, or press T during the show. The audience window never shows it.
- With one screen, press X during the show to switch between the slides and the presenter display. In a video call, share only the presentation window: the audience sees the slides, and you read the presenter display and the teleprompter.

### Changes

- Changing the language or app theme in Settings updates every open window, not only the one where you changed it.

### Fixes

- In right-to-left languages, speaker notes written in a left-to-right language are shown in the right direction in the presenter display.

## 0.2.0 — 2026-10-13

Cadenza now runs on macOS, and the AI assistant has been removed. Presentations from 0.1.x open as before.

### New

- Cadenza for macOS 12 or newer, as one app for both Apple Silicon and Intel Macs. It has the usual menu bar, opens `.cadenza` files from Finder and the Dock, and keeps API keys in the macOS Keychain.
- On a Mac, quitting with ⌘Q asks about unsaved changes in each open presentation. Quitting from the Dock or by logging out keeps them for recovery at the next start.

### Changes

- Cadenza has its real icon on Linux and Windows, instead of a plain purple square.

### Fixes

- Options in segmented controls, such as Fast / Normal / Slow or the background types, now fit their labels instead of spilling past the highlight. When a translation is too long to fit, only the longest labels are shortened.
- With several presentations open, media picked on YouTube, Vimeo, GIPHY or Unsplash is inserted only into the presentation you picked it for, not into all of them.

### Removed

- The AI assistant is gone, along with its settings and the Create with AI button. Cadenza no longer connects to AI services.
- API keys saved for the assistant (Anthropic, OpenAI or another server) are removed from your system keychain and fallback file on startup. If cleanup fails, Cadenza retries on later launches.

## 0.1.1 — 2026-10-13

A maintenance release with security and reliability fixes. Presentations from 0.1.0 open as before.

### Security

- Opening a presentation from an untrusted source can no longer run scripts. Text, shapes and table cells are cleaned when a file is opened or recovered after a crash, and again whenever slides are drawn.
- A crafted theme can no longer inject scripts into exported HTML through its embedded fonts.

### Fixes

- Closing a presentation or its window no longer discards an unfinished edit in a table. When an edit can't be applied, Cadenza keeps it and tries again instead of discarding it.
- Copying slides or objects to another presentation now brings their images, videos, backgrounds and embedded fonts along, even after the original presentation is closed.
- Saving no longer drops missing media silently. If a presentation refers to media that is missing, saving stops and names the affected slides so you can replace or delete it.
- The AI assistant can no longer change a presentation other than the one you asked about: closing or replacing the presentation cancels the request.
- When the assistant rewrites a slide, links to that slide keep working and later steps of the same request still apply to it.
- If you press Stop after the assistant has already made its change, the change is now shown instead of hidden.
- Skipping the first slide of a vertical stack no longer merges its sub-slides into the previous stack.
- Responses that arrive out of order no longer show outdated slides.

### Changes

- European Portuguese is now listed as "Portuguese"; Brazilian Portuguese remains "Portuguese (Brazil)".

## 0.1.0 — 2026-10-13

First release.
