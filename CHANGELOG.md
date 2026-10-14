# Changelog

Notable changes to Cadenza. Each version's section is also shown on its GitHub release page.

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
