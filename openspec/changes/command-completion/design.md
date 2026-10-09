## Context

Before this change, the command row was empty outside colon input. Tab switched focus in browsing mode and had no function while typing. Commands were plain strings edited only at the end. User messages remain English; original Unicode paths must be preserved.

## Goals / Non-Goals

**Goals:** Import path completion, a visible candidate list, predictable cycling, editable quoted results, and a bottom-left mode badge.

**Non-Goals:** Command history, fuzzy search, shell execution, a full text editor or changes to importing/playback.

## Decisions

### Corrected: Vim-style NORMAL and COMMAND modes

The user clarified that modes must follow Vim semantics. NORMAL identifies navigation and playback controls; COMMAND identifies colon command entry. INSERT belongs to ordinary UI text fields, of which the current interface has none. Typing a colon command does not enter INSERT. The badge occupies the left end of the existing bottom row. The shortcut footer stays absent. Command text and a visible cursor use the remaining width.

### Chosen: A private command-line module

CommandLine owns text, a UTF-8 byte cursor and completion state. Basic cursor movement and deletion make quoted completed paths editable without inserting after their closing quote. The UI routes keys and renders state; no new dependencies or public crate APIs are needed.

### Chosen: Common prefix, then candidate cycling

Tab completes :import arguments at the end of their path, including just before a closing quote. A unique candidate fills immediately. Multiple candidates first expand their common prefix and display a list; subsequent Tab/Shift-Tab cycle forward/backward. Shift-Tab can start from the final candidate. Typing, deletion or cursor movement discards the candidate session. A uniquely completed directory ends in a platform separator, allowing another Tab to inspect its children.

Scan only the current parent directory when starting completion, then cache candidates for cycling. Sort directories first and names lexically; include directories and supported audio files. Hidden names appear only when the basename prefix starts with a dot. Relative paths resolve from process cwd; ~/ resolves from the same home lookup used by imports. Completion preserves that spelling rather than replacing it with an absolute path. Existing quotes are preserved and whitespace paths are quoted automatically, with the cursor before the closing quote. No match leaves text intact; scan failures produce an English message and keep the command editable.

### Chosen: A small candidate popup

Render a capped list above the command row, highlight the current candidate and scroll selection within it. Clamp popup and cursor to the terminal bounds and scroll long command text horizontally using rendered Unicode width. In NORMAL, Tab keeps switching results focus.

## Risks / Trade-offs

- A local directory scan can be slow on a remote mount; scan once per completion session and never recursively.
- Non-UTF-8 filesystem names cannot be represented faithfully by the UTF-8 command editor; omit them rather than insert lossy paths.
- This is a small command editor: completion applies at the end of the path, not within an arbitrary component; cursor movement itself remains available.

## Migration Plan

Add the private module, route editing/completion keys, draw mode/candidates and update usage documentation. The SQLite format and Cargo dependencies stay unchanged. Reverting the UI/module changes restores the previous command row.

## Open Questions

None required for this scope. Async filesystem completion and command history can be considered separately if needed.

## Verification

- make ci passed on macOS: formatting, standard-library playback interface, Clippy with warnings denied, 51 ordinary tests, two doctests and release build.
- Five filesystem/editor tests cover relative and absolute directories, supported extensions, common prefixes, cycling/wrapping, home expansion, Unicode/quotes, hidden names, no matches, scan errors and UTF-8 editing.
- Two additional UI regressions verify completion through actual key handling and import, mode transitions, quoted editing, candidate rendering, cancellation, preserved browsing focus and tiny terminals.
- A real terminal smoke check passed with the release binary. It completed a spaced directory and Unicode child, placed the visible cursor inside closing quotes, edited and imported a FLAC, selected an MP3 via Tab/Shift-Tab, completed a home-relative path, reported a scan failure in English, cancelled input and restored the terminal on exit. The two imported audio BLOBs matched the fixtures, and SQLite integrity_check passed.
- All six OpenSpec changes pass strict validation. Linux and Windows execution was not performed in this session.
- After correcting mode semantics, all five UI regressions passed, including NORMAL-to-COMMAND entry and submission/cancellation back to NORMAL. Formatting, a fresh release build and strict validation of this change also passed.
