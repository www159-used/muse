## Context

The terminal library already supports numbered rows, counted navigation, separate selection/camera state and an editable colon-command prompt. Imports refresh the list asynchronously. Search must compose with these flows without starting playback or changing storage.

## Goals / Non-Goals

**Goals:** Find metadata across the full library, preview matches, cancel without losing the original view, repeat in either direction and display useful match feedback.

**Non-Goals:** Regular expressions, fuzzy ranking, filtering/reordering songs, search history persistence, configurable bindings, Unicode normalization or new storage indexes.

## Decisions

- **Confirmed direction:** Add song search. The following bindings and matching rules are implementation choices consistent with the application's Vim-style interaction.
- **Implementation choice:** `/` and `?` open forward/backward COMMAND prompts. Match literal substrings of titles, artists and albums after Unicode lowercasing. Missing metadata contributes no placeholder text. This keeps the MVP predictable and avoids a regex dependency or a separate result list.
- **Implementation choice:** An internal `search` module owns the prepared query, matching row indices, directional preview and counted wrapping repeat navigation. The UI owns prompt editing, selection and camera. Rebuild matching rows when the query or library changes, not during every draw.
- **Implementation choice:** Reuse CommandLine's text/cursor editing for search without calling completion. Digits, spaces, `n`, `N`, `:` and playback keys are search text while editing; Tab does not complete paths or change focus. The prompt uses the COMMAND badge and its `/` or `?` marker.
- **Implementation choice:** Capture the original selected track ID, selection/camera state and panel focus. Each text change previews the first matching row at or beyond the original row in the requested direction, wrapping when needed. An empty or unmatched preview restores the original view. Esc also restores focus and leaves the previous confirmed search intact. Restore by track ID after import refresh, with a bounded row fallback if it disappeared.
- **Implementation choice:** Enter confirms nonempty text even when it has no matches, reporting an English not-found message. An empty prompt repeats the previous query in the newly chosen direction; without previous text it reports `No previous search`. Confirmation remains browsing and does not play the target.
- **Implementation choice:** `n` follows the last confirmed direction and `N` reverses it, starting strictly after/before the current row. Counts such as `3n` advance that many matches using bounded modular arithmetic, including oversized counts and single matches. Search targets focus the library and use its existing reveal rule.
- **Implementation choice:** Style matching rows and show the query plus current match/total in the library title. Keep the complete list and its absolute numbers. Highlighting stays available for the confirmed query after editing ends.

## Risks / Trade-offs

- Preview can lose the original view or stale IDs after an import → retain a navigation snapshot, resolve its track ID and rebuild both confirmed and pending matches after refresh.
- Input can leak into commands/playback → route search editing before ordinary NORMAL bindings, disable completion and ignore wheel navigation during editing.
- Unicode cursor offsets can panic or counts can overflow → reuse the existing editor, use Unicode lowercasing and count modulo the match list; exercise real key and renderer paths.
- An album match is not necessarily visible as text in the existing song row → highlight the matching row and document all searchable fields.

## Migration Plan

Add the internal search module and UI integration, document controls and run workspace CI and strict OpenSpec validation. No library migration or new dependency is needed.

## Open Questions

None for this increment. The previously discussed wheel-model choice is separate from search.
