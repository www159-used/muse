Implementation checkpoints are grouped by dependency. Complete each item with its focused behavioral verification; the final phase covers cross-feature regression and manual acceptance. Completion marks preserve existing implementation progress. Task 1.5 uses the explicit `=` script marker; later phases unify registered command callbacks and mode-aware completion. This breakdown refines the existing scope and does not authorize code implementation by itself.

## 1. First Rhai command through the TUI

Dependencies: none. Outcome: `:=` evaluates a registered function without blocking drawing; ordinary input has an explicit separate entry route.

- [x] 1.1 Add Rhai with metadata support only to the application crate and establish the scripting module boundary; verify the playback crate still checks with no default features and no scripting dependency.
- [x] 1.2 Introduce a shared API descriptor/registration entry containing names, parameter/return metadata, documentation, constants, and completion annotations; verify a registered function is callable and appears in the descriptor catalog.
- [x] 1.3 Create the serialized script worker and bounded request/result messages; route Rhai input through it and surface an evaluated value or English error without blocking the TUI event loop. Verify on the rendered status row that preceding print/debug output cannot hide the final value or error.
- [x] 1.4 Retain session scope and compiled script functions across prompt submissions; verify a function defined in one submission remains callable in a later one, while a parse failure publishes no new definitions.
- [x] 1.5 Route only `:=` input to Rhai by stripping its leading marker; keep ordinary commands and numeric navigation on their current path until phase 4 registers their callbacks. Verify `:play`, `:=play()`, `:23`, `:=23`, and malformed input take only their selected path without error-triggered fallback.

## 2. Ordered host operations and responsive exit

Dependencies: phase 1. Outcome: scripts can perform ordered operations with reliable completion and failure feedback.

- [x] 2.1 Connect host calls to the existing operation layer with request-correlated completion; verify the script worker observes completed execution rather than queue acceptance.
- [x] 2.2 Preserve statement ordering across native calls and propagate failures; verify a failing first operation prevents an unhandled second operation, and earlier successful effects are never replayed or reported as rolled back.
- [x] 2.3 Carry expected playback identity through relevant requests and completion messages; verify a request for an old session cannot operate on or update the replacement track.
- [x] 2.4 Add script operation limits and bounded/cancellable waits for host calls; verify a runaway script, full queue, or unavailable service produces a useful error while rendering continues.
- [x] 2.5 Preserve the independent stop/quit control path and orderly worker shutdown; verify exit remains responsive during a pending script and existing quit/detach playback semantics are retained.

## 3. Numeric seek APIs and unit namespace

Dependencies: phases 1–2. Outcome: time and percentage seeking work from Rhai through the service.

- [x] 3.1 Register host Unit values under `unit::MS`, `SEC`, `MIN`, `HOUR`, and `PERCENT`; verify names resolve to the custom type and string units are rejected.
- [x] 3.2 Implement one checked integer/float conversion path with documented round-to-nearest millisecond behavior; verify negative absolute values, non-finite numbers, overflow, fractional values, and conversion boundaries.
- [x] 3.3 Register absolute time `seek(value, unit)` and signed relative time `seek_by(value, unit)`; verify execution-time relative positioning, missing-source errors, unknown-duration time seeks, and rejection of PERCENT for relative movement.
- [x] 3.4 Add percentage seek handling in the matching service session and update protocol compatibility as required; verify 0/50/100 percent, invalid percentages, unavailable duration, stale identities, and explicit version mismatch without restarting a playing service.
- [x] 3.5 Publish confirmed seek results to the TUI and restore lyric following only for a successful relevant manual seek; verify paused intent, backend bounds, failures, and stale completion handling.

## 4. Unify existing interactive operations under Rhai

Dependencies: phases 1–2. Outcome: ordinary commands and explicit `=` scripts use one Rhai runtime/API layer in both frontends. One-shot CLI arguments remain a separate boundary.

- [ ] 4.1 Inventory every TUI/REPL operation, option, default, and selection rule; declare Rhai APIs and ordinary command descriptors with exact names, aliases, callbacks, schemas, and capability/completion metadata, including queries, maintenance, help, modes, and next-once.
- [ ] 4.2 Register playback and quit/detach APIs plus ordinary command callbacks/aliases using existing typed handlers; verify ordinary and script entry agree on selection, end-of-playlist, and stop-versus-detach behavior.
- [ ] 4.3 Register `import_audio` with a path annotation and an ordinary `import` callback; verify a temporary import, quoted Unicode paths, background progress, and cancellation from both frontends and entry modes.
- [ ] 4.4 Register playlist/volume namespaces and `goto_line` with ordinary subcommand/numeric mappings and capability metadata; verify selection/confirmation, persisted gain, equivalence of `:23` and `:=goto_line(23)`, zero/overflow rejection, and clear REPL rejection of UI-only operations.
- [ ] 4.5 Register the remaining query, status, mode, next-once, library-maintenance, and help APIs from the inventory; factor typed operations below argv parsing, verify options/defaults and results, and ensure Rhai does not construct command strings or reenter a token parser.
- [ ] 4.6 Route REPL ordinary commands and `=`-prefixed scripts through the shared entry router/runtime with session scope and asynchronous results; verify retained functions, ordered calls, import/chooser workflows, no evaluation fallback, and responsive stop/quit/detach.
- [ ] 4.7 Implement the shared ordinary-command tokenizer and schema conversion, then replace duplicate business dispatchers with typed callback invocation on the Rhai worker; verify quotes/escapes, flags/subcommands, defaults, invalid arity, script-like path data, and no source interpolation, argv reentry, or recursive worker deadlock. Preserve independent emergency controls.

- [ ] 4.8 Add session-local `command(name, callback)` / `uncommand(name)` for zero-argument user commands, retaining callback environments; verify deferred execution, replacement/removal, builtin/alias collision rejection, invalid names, callback lifetime, and extra-argument errors.
- [ ] 4.9 Verify the complete command inventory against previous operation behavior, including explicit aliases; preserve CLI argv and normal-mode/chooser interactions, and verify unknown prefixes or historical unprefixed scripts never trigger guessed execution.

## 5. Deferred playback bindings

Dependencies: phases 1–3. Outcome: custom and default arrow bindings invoke functions at keypress time.

- [ ] 5.1 Implement session-local `bind`/`unbind`, canonical key/context validation, replacement, and removal; verify registration and removal themselves cause no playback action.
- [ ] 5.2 Retain callback function pointers, compilation environments, and captures; verify a callback still executes after the originating prompt closes and uses playback state at invocation rather than binding time.
- [ ] 5.3 Initialize default Left/Right mappings through the binding API; verify equivalence with typed `seek_by` calls and custom step-size overrides. Add bracket defaults after phase 10 exposes mark APIs.
- [ ] 5.4 Preserve prompt/search/chooser precedence, details-panel arrow scrolling, numeric navigation, and direct emergency exit handling; verify those contexts cannot accidentally trigger playback callbacks.

## 6. Command and script completion

Dependencies: phases 1, 3, and 4. Outcome: both frontends complete ordinary commands or Rhai symbols according to explicit entry mode, using one registration source.

- [ ] 6.1 Generate immutable snapshots of command names, aliases, schemas, user commands, Rhai metadata, constants, and session functions from shared registrations; verify runtime/catalog parity and refresh on registration/removal without executing callbacks or accessing the filesystem.
- [ ] 6.2 Define candidates with display/insertion text, replacement range, resulting cursor, kind, and details; implement UTF-8-safe edits and translate script-body offsets into editor offsets, verifying preservation of the leading `=`, whitespace, and suffix.
- [ ] 6.3 Complete registered command names/aliases and subcommands in ordinary mode, and function/namespace prefixes after `=`; verify empty-prompt command/script-entry discovery, `:pl`, `:=se`, `:=unit::`, readable overloads, and user command discovery without mixed candidates or inferred execution aliases.
- [ ] 6.4 Implement stable candidate cycling from the original completion session; insert/reuse parentheses only for script functions, verifying ordinary names remain bare, common prefixes, Tab/Shift-Tab wraparound, existing delimiters, and repeated cycling without accumulation.
- [ ] 6.5 Wire the shared mode-aware provider into the Ratatui popup and REPL editor, removing independent hard-coded candidate lists; invalidate sessions on edits/cursor movement or script-marker changes, and verify capability filtering, explicit Enter submission, Esc cancellation, and small-terminal behavior.

## 7. Command editing, completion hints, history, and repeat

Dependencies: phases 2, 4, and 6. Outcome: incomplete calls receive contextual completion; both frontends share readline-style editing/history, and the TUI can repeat its latest successful command.

- [ ] 7.1 Add conservative context scanning for identifiers/namespaces, delimiters, strings/escapes, comments, and active-call argument index; verify incomplete calls, nested calls, and quoted commas without requiring a complete AST.
- [ ] 7.2 Filter candidates by registered parameter roles and show active-parameter signature hints; verify qualified Unit candidates, PERCENT exclusion for `seek_by`, and hints rather than fabricated numbers for numeric arguments.
- [ ] 7.3 Reuse path discovery only for command-schema or API parameters annotated as paths; verify directory/audio filtering, `~/`, Unicode paths, and no filesystem scanning for unannotated strings or symbol contexts.
- [ ] 7.4 Select ordinary-command quoting or Rhai double-quoted escaping from explicit entry mode; verify unfinished quotes, embedded quotes/backslashes, spaces, script-marker offsets, closing delimiters, and following statements without changing path values.
- [ ] 7.5 Verify conservative suggestions and catalog refresh across input edits and successful registrations/removal; cover comments, unsupported contexts, correct command/script candidate separation in both frontends, and no callback/getter/script execution during completion.

- [ ] 7.6 Add shared readline-style editor actions for character/word/line movement, deletion, kill-to-boundary, kill-word, and a session-local yank slot; verify Unicode boundaries, empty kills, middle-of-line edits, completion invalidation, and preservation of non-command key behavior.
- [ ] 7.7 Add a bounded session-local history model with exact-source storage, adjacent deduplication, failed-input retention, navigation index, and saved draft/cursor; verify capacity eviction, empty history, no wraparound, immutable recalled entries, and draft restoration.
- [ ] 7.8 Wire Up/Down and Ctrl-P/N plus editing shortcuts into both frontends, replacing REPL-local history logic; verify completion-popup precedence, `=` marker restoration, no execution on recall, and exclusion of cancelled input, search/chooser input, key callbacks, and replay from history.
- [ ] 7.9 Track the latest successful manually submitted command separately from history using source and submission ids; verify only completed workflows (including import/chooser outcomes) update it, queue acceptance does not, failed/cancelled/stale outcomes preserve it, and out-of-order success cannot regress it.
- [ ] 7.10 Add normal-mode TUI dot replay through the existing submission path with current-context resolution and visible source feedback; verify one fresh relative seek, ordinary commands and script source, no history growth or self-recording, no-target/busy feedback, and rejection of terminal autorepeat/count multiplication.
- [ ] 7.11 Verify repeat integration across pending submissions, failures, changed track/selection/variables/user commands, and prompt/search/chooser precedence; confirm input dots remain literal, REPL input is unaffected, and repeated scripts use normal errors/ordering without automatic retries.

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

- [ ] 12.1 Update README/help with Rhai APIs, units, session-local bindings/user commands, `:=` script entry, ordinary names/aliases and numeric navigation, mode-aware completion, readline editing keys, draft-preserving history, success-based dot replay and its current-context semantics, frontend limits, and the separate unchanged CLI boundary; verify examples match registered commands/APIs.
- [ ] 12.2 Document practice lifecycle, full-track percentages, manual-seek exit behavior, and seek-based overshoot/gap limitations; identify deferred configuration persistence and other out-of-scope features accurately.
- [ ] 12.3 Run the affected automated suites and `make ci`, including existing playback, import, terminal, and service regressions; record results and address failures before marking this checkpoint complete.
- [ ] 12.4 Record manual terminal/audio acceptance for Unicode/path completion, readline editing/yank, history draft restoration, dot replay under failure/busy/input contexts, gestures/resizing, pause/exit responsiveness, detached practice, repeated-passage overshoot/gaps, and actual lyric alignment; use this evidence to assess the memorization workflow.
