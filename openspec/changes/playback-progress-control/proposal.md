## Why

muse needs precise progress control and repeated passage playback for memorizing melody and lyrics. As these controls grow, separate string grammars for time, percentages, ranges, and bindings become difficult to maintain; the command prompt should use a shared scripting language and discoverable application APIs.

## What Changes

- Evaluate standard Rhai in the TUI colon prompt without an extra language prefix. Expose playback operations as registered Rust APIs and execute key bindings as deferred script callbacks.
- Replace the earlier proposed seek/loop mini-grammar with numeric values and explicit unit arguments: `seek(90, unit::SEC)`, `seek(50, unit::PERCENT)`, `seek_by(-10, unit::SEC)`, and `practice(80, 95, unit::SEC)`.
- Export units under the `unit` namespace; use host-defined enum values in the proposed implementation rather than stringly typed units. No custom time literals or time-string grammar is required.
- Extend the existing command editor and popup with function, namespace, unit-argument, and path completion plus signature hints. Derive function metadata from Rhai registration and keep unit/path annotations alongside the same API definitions.
- Provide session-local `bind`/`unbind` functions and initialize default playback bindings through the same Rhai API. Retain existing input-context precedence.
- Add the shared bottom progress bar with click positioning, drag preview, commit on release, cancellation, and confirmed lyric following.
- Add continuous A–B practice playback with execution-time marks, precise interval arguments, range display, and on/off/clear/status APIs. The shared service owns looping, including while detached.
- Preserve existing shipped colon commands through explicit compatibility adapters. Do not introduce the previously proposed textual seek/loop forms or fall back to legacy parsing after a Rhai error.

Status: the user requested progress control, key-to-command bindings, continuous practice looping, a scripting-based interface beyond an auxiliary language command, explicit unit arguments, namespaces, and integrated completion. Rhai is the selected planning baseline from the discussion. Host enum representation, exact API names, compatibility details, and completion mechanics below are proposed implementation decisions. Persistent user configuration loading remains a follow-up; default bindings already use the same language/API surface.

## Capabilities

### New Capabilities

- `playback-progress-control`: Rhai playback APIs and deferred bindings, contextual command completion, progress/range display, continuous practice, and lyric synchronization.

### Modified Capabilities

None. This repository has no published `openspec/specs/` tree; prior requirements remain in earlier change artifacts. This revision extends the existing capability file rather than creating another planning artifact.

## Impact

- Add Rhai with metadata support to the application crate; keep the music library and native audio crates independent of scripting.
- `crates/muse/src/command.rs` and a dedicated scripting/API module: shared API registration, runtime state, completion catalog, incomplete-input analysis, and range-based text edits.
- `crates/muse/src/ui.rs`: completion/signature display, deferred key dispatch, progress gestures, and confirmed feedback.
- `crates/muse/src/service/client.rs`, playback worker, and protocol: ordered operation completion, session checks, percentage resolution, practice state/scheduling, and protocol versioning.
- No music-library migration or new audio dependency is planned. Full LSP integration, replacement line editors, saved practice presets, audio export, and one-shot CLI/REPL language migration are outside this change.
