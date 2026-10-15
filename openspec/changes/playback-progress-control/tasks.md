Implementation checkpoints are grouped by dependency. Complete each item with its focused behavioral verification; the final phase covers cross-feature regression and manual acceptance. All items are pending. This breakdown refines the existing scope and does not authorize code implementation by itself.

## 1. First Rhai command through the TUI

Dependencies: none. Outcome: the colon prompt evaluates a registered function without blocking drawing.

- [ ] 1.1 Add Rhai with metadata support only to the application crate and establish the scripting module boundary; verify the playback crate still checks with no default features and no scripting dependency.
- [ ] 1.2 Introduce a shared API descriptor/registration entry containing names, parameter/return metadata, documentation, constants, and completion annotations; verify a registered function is callable and appears in the descriptor catalog.
- [ ] 1.3 Create the serialized script worker and bounded request/result messages; route Rhai input through it and surface an evaluated value or English error without blocking the TUI event loop.
- [ ] 1.4 Retain session scope and compiled script functions across prompt submissions; verify a function defined in one submission remains callable in a later one, while a parse failure publishes no new definitions.
- [ ] 1.5 Introduce explicit input classification for shipped legacy forms versus Rhai calls before evaluation; verify `:play`, `:play()`, numeric navigation, and malformed script input take only their intended path with no error-triggered fallback.

## 2. Ordered host operations and responsive exit

Dependencies: phase 1. Outcome: scripts can perform ordered operations with reliable completion and failure feedback.

- [ ] 2.1 Connect host calls to the existing operation layer with request-correlated completion; verify the script worker observes completed execution rather than queue acceptance.
- [ ] 2.2 Preserve statement ordering across native calls and propagate failures; verify a failing first operation prevents an unhandled second operation, and earlier successful effects are never replayed or reported as rolled back.
- [ ] 2.3 Carry expected playback identity through relevant requests and completion messages; verify a request for an old session cannot operate on or update the replacement track.
- [ ] 2.4 Add script operation limits and bounded/cancellable waits for host calls; verify a runaway script, full queue, or unavailable service produces a useful error while rendering continues.
- [ ] 2.5 Preserve the independent stop/quit control path and orderly worker shutdown; verify exit remains responsive during a pending script and existing quit/detach playback semantics are retained.

## 3. Numeric seek APIs and unit namespace

Dependencies: phases 1–2. Outcome: time and percentage seeking work from Rhai through the service.

- [ ] 3.1 Register host Unit values under `unit::MS`, `SEC`, `MIN`, `HOUR`, and `PERCENT`; verify names resolve to the custom type and string units are rejected.
- [ ] 3.2 Implement one checked integer/float conversion path with documented round-to-nearest millisecond behavior; verify negative absolute values, non-finite numbers, overflow, fractional values, and conversion boundaries.
- [ ] 3.3 Register absolute time `seek(value, unit)` and signed relative time `seek_by(value, unit)`; verify execution-time relative positioning, missing-source errors, unknown-duration time seeks, and rejection of PERCENT for relative movement.
- [ ] 3.4 Add percentage seek handling in the matching service session and update protocol compatibility as required; verify 0/50/100 percent, invalid percentages, unavailable duration, stale identities, and explicit version mismatch without restarting a playing service.
- [ ] 3.5 Publish confirmed seek results to the TUI and restore lyric following only for a successful relevant manual seek; verify paused intent, backend bounds, failures, and stale completion handling.

## 4. Existing application API adapters

Dependencies: phases 1–2. Outcome: existing application behavior is available through registered Rhai APIs and its shipped entry points remain usable.

- [ ] 4.1 Inventory shipped colon operations and define their valid Rhai names, signatures, metadata, and compatibility adapters together; verify each supported operation has an explicit mapping and no newly invented textual seek/loop grammar is introduced.
- [ ] 4.2 Register play/pause/stop/next/previous and quit/detach adapters using the existing handlers; verify playback selection, end-of-playlist, and stop-versus-detach behavior remain consistent.
- [ ] 4.3 Register `import_audio` with a path-parameter annotation and retain legacy import handling; verify a real temporary import, quoted Unicode paths, background progress, and cancellation through both entry forms.
- [ ] 4.4 Register playlist, volume, and numeric-navigation adapters; verify existing selection/confirmation rules, persisted gain, and navigation behavior rather than duplicating those implementations in scripts.
- [ ] 4.5 Preserve explicit legacy completion/input mode and standalone CLI/REPL grammar isolation; verify script errors never dispatch a legacy operation and CLI/REPL do not advertise unavailable Rhai calls.

## 5. Deferred playback bindings

Dependencies: phases 1–3. Outcome: custom and default arrow bindings invoke functions at keypress time.

- [ ] 5.1 Implement session-local `bind`/`unbind`, canonical key/context validation, replacement, and removal; verify registration and removal themselves cause no playback action.
- [ ] 5.2 Retain callback function pointers, compilation environments, and captures; verify a callback still executes after the originating prompt closes and uses playback state at invocation rather than binding time.
- [ ] 5.3 Initialize default Left/Right mappings through the binding API; verify equivalence with typed `seek_by` calls and custom step-size overrides. Add bracket defaults after phase 10 exposes mark APIs.
- [ ] 5.4 Preserve prompt/search/chooser precedence, details-panel arrow scrolling, numeric navigation, and direct emergency exit handling; verify those contexts cannot accidentally trigger playback callbacks.

## 6. Function and namespace completion

Dependencies: phases 1 and 3. Outcome: the existing popup discovers functions and units from the runtime's API catalog.

- [ ] 6.1 Generate immutable symbol snapshots from application registrations, Rhai metadata, exported constants, and available session functions; verify runtime/catalog parity without executing scripts or accessing the filesystem.
- [ ] 6.2 Define completion candidates with display text, insertion text, replacement range, resulting cursor, kind, and details; implement UTF-8-safe range application and verify middle-of-line edits preserve the suffix.
- [ ] 6.3 Add function/namespace prefix matching and empty-prompt discovery, grouping numeric overloads into readable signatures; verify `se`, `unit::`, and newly available registered functions are discoverable without another name list.
- [ ] 6.4 Implement stable candidate cycling from the original completion session and insertion/reuse of function parentheses; verify common prefixes, Tab/Shift-Tab wraparound, existing delimiters, and repeated cycling without text accumulation.
- [ ] 6.5 Wire candidates into the current Ratatui popup and invalidate sessions on editing/cursor movement; verify Enter submits explicitly, Esc cancels without effects, and narrow/short terminals preserve input.

## 7. Argument hints and path completion

Dependencies: phases 4 and 6. Outcome: incomplete calls receive argument-specific completion without evaluation.

- [ ] 7.1 Add conservative context scanning for identifiers/namespaces, delimiters, strings/escapes, comments, and active-call argument index; verify incomplete calls, nested calls, and quoted commas without requiring a complete AST.
- [ ] 7.2 Filter candidates by registered parameter roles and show active-parameter signature hints; verify qualified Unit candidates, PERCENT exclusion for `seek_by`, and hints rather than fabricated numbers for numeric arguments.
- [ ] 7.3 Reuse existing path discovery only for path-annotated string arguments; verify directory/audio filtering, `~/`, Unicode paths, and that ordinary strings/symbol contexts do not scan the filesystem.
- [ ] 7.4 Escape path insertions as Rhai double-quoted strings while preserving following syntax; verify unfinished quotes, embedded quotes/backslashes, spaces, closing delimiters, and a following statement.
- [ ] 7.5 Verify conservative fallback and catalog refresh across input edits and successful registrations; cover comments, unsupported contexts, legacy/Rhai candidate overlap, and no callback/getter/script execution during completion.

## 8. Shared progress bar and pointer seeking

Dependencies: phase 3. Outcome: both views display actual progress and support click/drag positioning.

- [ ] 8.1 Render one shared bottom progress row from reported status in library and lyrics views; verify elapsed/total labels, paused display, and view switching without changing playback.
- [ ] 8.2 Handle unknown duration and constrained terminal dimensions; verify no fabricated ratio, no overlapping rows, and disabled pointer seeking when the bar is unusable.
- [ ] 8.3 Implement bar hit testing, endpoint mapping, and preview state on left press/drag; verify horizontal clamping and target display while audio/lyric highlighting remain on actual progress.
- [ ] 8.4 Commit exactly one structured seek on release for a click or drag, through the same operation path as Rhai; verify no seek on initial press and equivalent results for pointer and typed targets.
- [ ] 8.5 Cancel gestures on Esc, resize, pending load, changed identity, lost usable duration, or prompt/chooser entry; verify release after cancellation is inert and existing wheel/input behavior remains intact.

## 9. Practice state, validation, and lifecycle

Dependencies: phases 2–3. Outcome: the shared service can retain and validate an interval safely before automatic looping is enabled.

- [ ] 9.1 Add current-session A/B/enabled state and snapshot fields to the service, updating protocol compatibility for the final wire shape; verify round trips, multiple-reader visibility, and explicit version mismatch.
- [ ] 9.2 Implement execution-time A/B marking and shared explicit-endpoint conversion, including mixed integer/float values and shared time/percentage units; verify A < B again after rounding and 0 <= A < B <= duration.
- [ ] 9.3 Implement establishment at A and activation intent rules; verify Playing continues, Paused/Ready/Finished remain non-playing, and seek failure or landing at/after B does not publish successful activation. Exercise through the service test harness until phase 10 adds continuous scheduling.
- [ ] 9.4 Implement read-only status, off, on, and clear state transitions; verify retained markers, unchanged position on off/clear, and missing/incomplete-pair errors without modifying prior healthy state.
- [ ] 9.5 Apply pause/resume/detach retention and stop/shutdown/successful-replacement clearing; verify failed healthy replacement retains the old interval and stale requests cannot mutate a new session.

## 10. Continuous A–B service playback

Dependencies: phase 9. Outcome: an activated passage repeats continuously even without an attached TUI.

- [ ] 10.1 Add periodic boundary checking on a worker deadline independent of receive timeouts; verify successive wraps occur under sustained request traffic without changing public playback identity.
- [ ] 10.2 Give automatic return precedence over natural playlist advance and handle B at confirmed EOF; verify the same track resumes from A and the selected playlist mode is unchanged.
- [ ] 10.3 Serialize pause/stop intent with boundary handling and disable failed wraps; verify completed pause/stop cannot be undone, failure reports remain visible, and no repeated retry or concealed playlist advance occurs.
- [ ] 10.4 Resolve manual seek interaction atomically before boundary handling; verify successful landings inside `[A, B)` retain looping, outside landings disable it with markers retained, and cancelled previews change nothing.
- [ ] 10.5 Register the public `practice`, mark, status, on/off/clear APIs against the completed service behavior; verify `practice(...); play();` ordering, error propagation, detached looping, and reattachment snapshots.

## 11. Practice controls, display, and lyrics

Dependencies: phases 5–8 and 10. Outcome: the complete practice workflow is usable from keys, typed calls, and both player views.

- [ ] 11.1 Install `[` / `]` default callbacks through the shared binding API; verify marking while listening, preparing a range while paused, and prompt text-entry precedence.
- [ ] 11.2 Render A/B, selected interval, and active-versus-retained state on the full-track bar; verify narrow layouts, markers occupying the same cell, and updates made by another client.
- [ ] 11.3 Distinguish explicit activation/manual seeking from automatic wraps for lyric centering; verify explicit success restores following while an automatic wrap preserves the manual-scroll grace period.
- [ ] 11.4 Verify the integrated workflow of marking, looping, off/on/clear, clicking inside/outside, pausing, and replacing the track; confirm stale gesture/result handling does not change the replacement session.

## 12. Documentation and final acceptance

Dependencies: phases 1–11. Outcome: behavior is documented and the complete change passes repository and manual acceptance checks.

- [ ] 12.1 Update README/help with supported Rhai API signatures, unit values, session-local bindings, contextual completion, legacy compatibility, and unchanged CLI/REPL boundaries; verify every documented example names a registered API.
- [ ] 12.2 Document practice lifecycle, full-track percentages, manual-seek exit behavior, and seek-based overshoot/gap limitations; identify deferred configuration persistence and other out-of-scope features accurately.
- [ ] 12.3 Run the affected automated suites and `make ci`, including existing playback, import, terminal, and service regressions; record results and address failures before marking this checkpoint complete.
- [ ] 12.4 Record manual terminal/audio acceptance for Unicode/path completion, gestures/resizing, pause/exit responsiveness, detached practice, repeated-passage overshoot/gaps, and actual lyric alignment; use this evidence to assess the memorization workflow.
