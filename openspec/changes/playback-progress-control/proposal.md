## Why

muse needs precise progress control and repeated passage playback for memorizing melody and lyrics. As these controls grow, separate string grammars for time, percentages, ranges, and bindings become difficult to maintain; ordinary commands and key bindings should use a shared scripting runtime and discoverable application APIs while retaining concise Vim-style interaction.

## What Changes

- Keep Vim-style ordinary commands, explicit aliases, and numeric navigation such as `:23`. Use `:=` for standard Rhai source in the TUI (`=` in the REPL); ordinary commands and key bindings invoke callbacks through the same Rhai runtime and registered Rust APIs.
- Replace the earlier proposed seek/loop mini-grammar with numeric values and explicit unit arguments: `seek(90, unit::SEC)`, `seek(50, unit::PERCENT)`, `seek_by(-10, unit::SEC)`, and `practice(80, 95, unit::SEC)`.
- Export units under the `unit` namespace; use host-defined enum values in the proposed implementation rather than stringly typed units. No custom time literals or time-string grammar is required.
- Extend the existing command editor and popup with ordinary-command completion and explicit script-mode function, namespace, unit-argument, and path completion plus signature hints. Derive both from shared registrations, including command aliases, argument schemas, and API parameter annotations.
- Add readline-style command editing and shared session-local Up/Down history, preserving unfinished drafts and the `=` script marker. In TUI normal mode, `.` repeats the latest successfully submitted command/script through the existing execution path.
- Provide session-local user command registration and `bind`/`unbind` functions and initialize default playback bindings through the same Rhai API. Retain existing input-context precedence.
- Add the shared bottom progress bar with click positioning, drag preview, commit on release, cancellation, and confirmed lyric following.
- Add continuous A–B practice playback with execution-time marks, precise interval arguments, range display, and on/off/clear/status APIs. The shared service owns looping, including while detached.
- Replace separate interactive business dispatchers with a thin command registry backed by Rhai callbacks. Preserve useful shipped command forms as first-class entries, including playback, import, playlist, volume, queries, and maintenance. Remove heuristic script detection and error-triggered fallback; no second legacy execution backend is retained.

Status: the user requested progress control, key-to-command bindings, continuous practice looping, a scripting-based interface beyond an auxiliary language command, explicit unit arguments, namespaces, and integrated completion. Rhai is the selected planning baseline from the discussion. The user wants unified execution while retaining Vim-style commands and found `:rhai` too long. The proposed concise script marker is `:=`. Readline-style editing, history navigation, and dot repeat are new user requests; the key subset, history policy, and success-based repeat semantics below are proposed defaults; command registration names, host enum representation, and completion mechanics below are implementation decisions. Persistent user configuration loading remains a follow-up; default bindings already use the same language/API surface.

## Capabilities

### New Capabilities

- `playback-progress-control`: Rhai playback APIs and deferred bindings, contextual command completion, readline-style editing, command history/repeat, progress/range display, continuous practice, and lyric synchronization.

### Modified Capabilities

None. This repository has no published `openspec/specs/` tree; prior requirements remain in earlier change artifacts. This revision extends the existing capability file rather than creating another planning artifact.

## Impact

- Add Rhai with metadata support to the application crate; keep the music library and native audio crates independent of scripting.
- `crates/muse/src/command.rs` and a dedicated scripting/API module: shared API registration, runtime state, completion catalog, incomplete-input analysis, range-based text edits, editor actions, bounded history/draft state, and request-correlated repeat records.
- `crates/muse/src/ui.rs` and `crates/muse/src/repl.rs`: explicit command/script routing, shared callback execution/completion, removal of duplicate interactive business dispatch, completion/signature display, deferred key dispatch, progress gestures, and confirmed feedback. Reuse typed operations beneath the one-shot CLI parser.
- `crates/muse/src/service/client.rs`, playback worker, and protocol: ordered operation completion, session checks, percentage resolution, practice state/scheduling, and protocol versioning.
- No music-library migration or new audio dependency is planned. Full Vim Ex syntax, full LSP integration, full GNU Readline parity, persistent command history, saved practice presets, audio export, and one-shot CLI argument syntax changes are outside this change.
