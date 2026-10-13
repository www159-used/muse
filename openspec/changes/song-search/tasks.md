## 1. Search behavior

- [x] 1.1 Add prepared metadata matching and `/`/`?` incremental prompts, including direction, confirmation and counted wrapping n/N navigation.
- [x] 1.2 Restore original selection/camera/focus on cancellation and safely handle empty queries, no matches, Unicode editing, mode isolation and library refreshes.

## 2. Feedback and verification

- [x] 2.1 Render search prompts, matching rows and query/match feedback; verify narrow/tiny views and real terminal input.
- [x] 2.2 Document controls and scope, run workspace CI and strict OpenSpec validation, and record verification.

## Verification

- The initial search tracer failed with `/moon` leaving selection at row 1. It now previews an off-screen artist match, confirms without playing and repeats through artist, album and title matches with n/N/counts.
- Eight UI/rendering workflows cover preview, cancellation restoring camera/panel focus and the prior query, forward/backward wrapping, empty prompts, oversized counts, single/empty libraries, literal Unicode text and cursor edits, missing metadata placeholders, key/completion/wheel isolation, persistent highlights and narrow/tiny views.
- The import workflow uses real generated audio, SQLite connections and the production NCM background worker. It verifies newly added matches for both pending and confirmed queries and cancellation by original track ID after an earlier row is removed.
- The real pseudo-terminal search test verifies COMMAND/NORMAL transitions, metadata matches and n revealing the next off-screen match without starting playback. The existing wheel, connected-command and terminal-hangup tests also pass.
- `make ci` passed formatting, the standard-library playback interface check, Clippy with denied warnings, 84 ordinary Rust tests, eight import scenarios with 65 steps, two documentation tests and the release build. Three existing physical-device/personal-source tests remain ignored.
- `openspec validate --all --strict` passed all thirteen changes. Tests use temporary libraries and generated repository fixtures; no personal music or library was used.
