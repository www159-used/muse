## Why

Large libraries need a way to find songs by their metadata without knowing a line number. The current terminal UI only supports browsing and numbered jumps.

## What Changes

- Add `/` forward and `?` backward incremental song search.
- Search titles, artists and albums using case-insensitive literal text; retain the complete library order and absolute line numbers.
- Preview matches while editing, confirm with Enter and restore the original selection/view with Esc.
- Remember confirmed searches for wrapping `n`/`N` navigation, including numeric counts and empty-prompt repetition.
- Highlight matching rows and show the query and match position/count in English UI labels.
- Keep search input in COMMAND mode, isolated from playback, import commands and mouse navigation; stay safe across Unicode edits, empty libraries and import refreshes.

## Capabilities

### New Capabilities

- `song-search`: Incremental metadata search and repeatable match navigation in the terminal library.

### Modified Capabilities

None.

## Impact

The application gains a small internal search module, UI prompt/navigation integration and rendering/terminal regression coverage. Storage, playback and dependency interfaces are unchanged; no migration or network access is needed.
