## ADDED Requirements

### Requirement: Incremental metadata search

In NORMAL mode, `/` and `?` SHALL open forward/backward search input focused on the song library. Search SHALL match case-insensitive literal substrings of stored titles, artists and albums across the full list, using Unicode lowercasing without filtering or reordering songs. As text changes, selection SHALL preview the first match at or beyond the original selected row in the chosen direction, wrapping if necessary. Empty or unmatched text SHALL retain the original selected song and view.

#### Scenario: Preview a metadata match beyond the current viewport
- **WHEN** the user opens `/` and types text found in a later song's artist or album
- **THEN** that song is selected and revealed while the full list and absolute line numbers remain intact

#### Scenario: Search Unicode text literally
- **WHEN** the user enters Chinese, Korean, spaces or regex punctuation
- **THEN** search matches literal metadata text without interpreting a regular expression or splitting characters during edits

### Requirement: Confirm or cancel search

Search editing SHALL display a COMMAND badge and the `/` or `?` prompt. Enter SHALL remember nonempty text and its direction without playing the selected song. Esc SHALL restore the original song, camera and panel focus while retaining the prior confirmed search. Empty Enter SHALL repeat the previous query in the new prompt's direction or report `No previous search`. A confirmed query with no matches SHALL report an English not-found message and remain safe in empty libraries.

#### Scenario: Cancel a preview from the results panel
- **WHEN** a user begins search while import results have focus, previews a song and presses Esc
- **THEN** the original song, viewport and results focus are restored and no playback is requested

#### Scenario: Repeat an existing query with an empty prompt
- **WHEN** a query was previously confirmed and the user opens an empty search and presses Enter
- **THEN** the next matching song in the prompt's direction is selected, wrapping when needed

### Requirement: Repeat match navigation

In NORMAL mode, `n` SHALL select the next match in the last confirmed search direction and `N` SHALL use the opposite direction. Repeats SHALL start strictly beyond the current row, wrap at both ends and support numeric counts safely, including oversized input and a single matching song. Match navigation SHALL focus the library and reveal the selected song without changing imports or playback.

#### Scenario: Repeat and reverse a search
- **WHEN** a search has multiple matches and the user presses n, N or 3n
- **THEN** selection follows the remembered direction, its reverse or three matching steps respectively and remains visible

### Requirement: Search feedback and input isolation

The library SHALL show the active query and current match/total count and highlight matching rows, including after confirmation. Search input SHALL support Unicode-safe cursor editing without acting as NORMAL navigation, command completion or playback controls. Wheel input SHALL not change selection during search editing. After an asynchronous import refresh, matches SHALL be rebuilt against the new list and cancellation SHALL restore the original track by identity when available. Rendering and input SHALL remain safe in empty, narrow and tiny views.

#### Scenario: Edit search text with navigation characters
- **WHEN** the user types digits, n, spaces or colon in a search prompt and presses Tab or wheel input
- **THEN** text remains search input, completion/import/playback do not run and the prompt remains active

#### Scenario: Import finishes during a search
- **WHEN** an import refresh adds a matching song while the search prompt is open
- **THEN** search feedback reflects the current list and Esc restores the original selected track and valid view
