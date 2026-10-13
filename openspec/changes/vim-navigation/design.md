## Context

The application already handles NORMAL and COMMAND modes, a stateful song list and a scrollable import-results panel. Each song occupies one display row. Ratatui keeps a selected row visible but does not provide application key bindings for paging or numbered jumps.

## Goals / Non-Goals

**Goals:** Fast keyboard browsing, absolute song numbers, direct jumps, viewport-aware paging and clear mode boundaries.

**Non-Goals:** A complete Vim command language, configurable bindings, search, relative numbering or storage changes.

## Decisions

- **Confirmed scope:** Add Vim-style paging and visible song numbers with numbered jumps. Keep the removed shortcut footer absent.
- **Confirmed refinement:** Follow the selection/camera logic in `zellij-agent-board/src/lib.rs` (`decide`, `reveal`, `clamp_view`, `scroll_view`, `half_page` and `page_step`). Its wheel scrolling moves selection and camera together; its keyboard paging moves the selection first and reveals it only as far as needed. It follows the reference's one-row page overlap rather than claiming complete Neovim compatibility.
- **Implementation choice:** Record each panel's inner height during drawing. For songs, use the number of rows that fit and exist. Half pages use half that span rounded down; full pages subtract one row for overlap. Both have a minimum distance of one row. Results use their own inner height. This adapts to resizing and excludes borders and status rows.
- **Implementation choice:** Keep selection and camera offset independent in ListState. Keyboard song movement, including paging and jumps, explicitly reconciles the camera: keep its existing offset while the selected row is visible, otherwise move it just enough to reveal that row. Clamp to the last full viewport after resizing or refreshing the library. Drawing uses the same reconciliation so rendering cannot establish a different navigation rule. Results paging changes only its vertical scroll.
- **Implementation choice:** Cache each panel's inner rectangle during rendering for both page sizing and mouse targeting. In NORMAL mode a vertical wheel event over the song body scrolls three rows, matching agent board's host `handle_home_mouse`. Move selection by the camera's actual delta so its screen row stays fixed, including partial moves at the boundaries. At a pinned camera, selection also stops. Each song paints one row, so selection itself serves as the reference's scroll anchor. Wheel events over results scroll only completed report text, without changing keyboard focus. Commands, import progress, borders and other screen regions ignore the wheel. Accepted wheel input clears unfinished navigation prefixes.
- **Implementation choice:** Enable Crossterm mouse capture when entering the alternate screen, dispatch both key and mouse events through the app, and disable capture in the existing terminal-restoration guard on every exit path.
- **Implementation choice:** Display aligned absolute numbers based on current list order, independent of database IDs. Numbered G and numeric colon commands select that song and focus the library. Unnumbered gg/G jump to the focused panel's beginning/end.
- **Implementation choice:** Keep a small numeric prefix and pending-g flag in the UI. Counts work with j/k and multiply half/full page distances, matching the reference. Counts of zero act as one movement; absolute zero targets still clamp to the first song. Esc, unrelated keys and mode/focus transitions clear unfinished prefixes; modified character keys cannot accidentally act as ordinary navigation. Arithmetic saturates for oversized input.
- **Implementation choice:** Test the production event dispatcher, key/mouse handlers and renderer using Ratatui's TestBackend. Generated track metadata is sufficient for navigation tests. A Unix pseudo-terminal test seeds temporary owned audio through the public library API, sends real SGR wheel reports to the running binary, verifies scrolling and observes capture enable/disable sequences. Physical audio output remains a separate flow.

## Risks / Trade-offs

- Borders or resizing can produce wrong page distances → derive heights from the drawn inner areas and test multiple sizes, including a one-cell terminal.
- A correct selection can remain off-screen → assert visible rows and viewport positions after paging and jumps.
- Prefixes can leak into commands or another panel → reset on transitions and verify mode/focus sequences.

## Migration Plan

Add controls and numbering, document them and run workspace CI. No library migration is required.

## Open Questions

None for this increment.
