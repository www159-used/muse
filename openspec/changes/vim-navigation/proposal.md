## Why

Browsing a large library currently requires moving one song at a time. The list has no line numbers, so users cannot identify and jump directly to a song using familiar Vim controls.

## What Changes

- Add NORMAL-mode half-page and full-page navigation using Ctrl-d/u, Ctrl-f/b and PageDown/PageUp.
- Show aligned, absolute line numbers starting at 1 in the song list.
- Add gg/G, numbered G jumps and numeric colon commands such as :23.
- Apply paging to the focused song list or import results, with viewport-aware distances and bounded positions.
- Match agent board's cursor/camera rules: keyboard movement reveals the selection with minimal scrolling, full pages retain one row of overlap, and counts also apply to paging.
- Capture mouse wheel input over the song list or import results; song scrolling moves the camera and selection together by the actual bounded distance.
- Keep numeric prefixes scoped to NORMAL navigation and document the controls.

## Capabilities

### New Capabilities

- `vim-navigation`: Viewport-aware paging and numbered song navigation in the terminal interface.

### Modified Capabilities

None.

## Impact

Changes affect the application's UI, navigation/rendering tests and contributor documentation. The existing terminal handling, storage interfaces and dependencies continue to serve the feature.
