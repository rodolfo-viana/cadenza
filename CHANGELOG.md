# Changelog

Notable changes to Cadenza. Each version's section is also shown on its GitHub release page.

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
