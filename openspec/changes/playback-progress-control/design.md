## Context

At the start of this change, the TUI binds Left/Right directly to relative `SeekTarget` calls. `command.rs` implements completion locally: name matching only at the end of the first token, special handling for import paths, and replacements that rebuild the input. Ratatui already renders a candidate popup, and the standalone REPL shares the editor but keeps a separate command vocabulary. Colon commands are parsed separately by `BuiltinCommand`; several transport commands use a synchronous service call. The service client already queues seek requests with the expected current playback identity, and the native backend supports absolute and execution-time relative seeks.

Partial implementation includes the Rhai worker and explicit `=` script entry; ordinary commands still use the existing dispatch path pending phase 4. The target retains Vim-style entry while moving registered commands to Rhai callbacks. Completed runtime/catalog work remains useful. The existing REPL keeps a local history vector and handles Up/Down, but returning past the newest entry clears the buffer instead of restoring a draft. TUI command input has no history navigation. Both frontends already share the basic `CommandLine` buffer.

Lyrics select the last timestamp at or before the reported playback position. Manual lyric scrolling suspends centering for three seconds. A new seek does not require a different lyric clock or parser.

The user also wants to practice passages to memorize melody and lyrics and explicitly selected continuous A–B looping. The playback worker currently polls on a 40 ms receive timeout and advances playlists on Finished. Practice scheduling must live in that service, take precedence over natural advancement, and continue running when requests keep arriving.

## Goals / Non-Goals

**Goals:**
- Unify TUI and interactive REPL command execution under Rhai while retaining ordinary command names, aliases, and numeric navigation.
- Make Rhai calls, deferred playback bindings, and pointer seeking share registered application operations and completion feedback.
- Support readline-style editing, recoverable command history, and explicit dot repeat without interfering with ordinary browsing or playback.
- Reuse API definitions for execution, function signatures, namespace symbols, parameter suggestions, and path completion.
- Show one progress row in library and lyric views, without another keyboard focus mode.
- Keep input responsive while the service seeks, and render the actual reported position after completion.
- Mark a passage and repeat it continuously, with progress and lyrics following each return to A.

**Non-Goals:**
- General migration of all navigation and playback keys to a configurable command system.
- Persistent user keymap/configuration files or command history, command prefill, a full language server, full GNU Readline parity, or changes to one-shot CLI argument syntax. Session-local function bindings, history, and a focused readline-style editing subset are included.
- Custom duration literals, time-string parsing, expression evaluation in ordinary command arguments, and full Vim Ex syntax (ranges, substitution, pipelines, or inferred abbreviations). A thin registered-command tokenizer is included.
- Continuous audible scrubbing, a new playback clock, lyric format changes, or audio backend replacement.
- Audio export, saved ranges, lyric-line selection, playback speed controls, inserted silences, repeat counts, and sample-exact or gapless looping.

## Decisions

### 1. Standard Rhai and registered application APIs

Status: scripting and function bindings are the user's direction; runtime architecture is proposed.

Use standard Rhai syntax after the short `=` marker: `:=seek(90, unit::SEC)` in the TUI and `=seek(90, unit::SEC)` in the REPL. The marker accepts complete scripts, including statements and function definitions, not only expressions. Ordinary commands invoke retained callbacks through the same runtime rather than another business dispatcher. Both frontends share registration data and runtime implementation, with session-local scope and frontend capabilities. The `:=` spelling is a proposed response to the user finding `:rhai` too long; a long-form alias is not required. Do not add custom `10s` literals. Register native application functions once, with names, parameter/return metadata, descriptions, and any semantic completion annotations. Keep script execution outside the render/event loop and outside audio callbacks. A dedicated serialized script worker owns its engine, scope, compiled functions, and retained callbacks; bounded input queues and Rhai execution limits keep the UI responsive. Native calls also need bounded waits/cancellation because a script operation budget does not interrupt a blocking host function.

Playback host calls submit structured operations and await their own completion on that worker. Calls in `practice(...); play();` therefore execute in order; a failed first call aborts the remaining statements unless explicitly handled by the script. Previously completed effects are not rolled back. Runtime error handling must not retry a script. UI callbacks and completion consume snapshots/messages rather than calling back into a locked engine. Stop/quit retain a responsive control path even when a script is busy. Expected playback identity accompanies each relevant operation.

Retain script-defined functions and callback ASTs for subsequent calls; dropping the compilation result after evaluation would invalidate later anonymous-function execution. Compilation failures publish no new binding state. A runtime failure can leave earlier successful definitions/effects in place; report that accurately rather than promising transactional scripts. Capture current playback state when an action executes, not when its key is bound.

### 2. Explicit units and API signatures

Status: separate numeric/unit arguments and namespace usage follow the discussion; concrete values and validation are proposed.

Export `unit::MS`, `unit::SEC`, `unit::MIN`, `unit::HOUR`, and `unit::PERCENT` as instances of a Rust-defined `Unit` enum registered as a Rhai custom type. `unit` is a static namespace; scripts do not declare an enum themselves. These values are not strings. Validation happens at runtime, so this is not a static type-checking guarantee.

| API | Semantics |
| --- | --- |
| `seek(value, unit)` | Absolute time or full-track percentage |
| `seek_by(value, unit)` | Signed relative time; percentage is not accepted in this first version |
| `practice(start, end, unit)` | A/B absolute endpoints using a shared time unit or full-track percentage |
| `mark_a()` / `mark_b()` | Execution-time marks |
| `practice_status()` | Read-only markers/enabled state |
| `practice_on()` / `practice_off()` / `practice_clear()` | Enable retained pair / disable / remove markers |

Register integer and floating numeric overloads, including mixed integer/float practice endpoints, with one shared checked conversion path. Absolute arguments must be nonnegative; percentages must be within 0 through 100. Reject non-finite values, overflow, unknown argument types, and unsupported units before submitting mutations. Normalize time values to milliseconds using a documented round-to-nearest policy; validate A < B again after conversion. Percentage resolution occurs in the service against the expected current track and requires known positive duration. Relative time remains relative until execution. An unknown duration still permits ordinary time seeks, while practice establishment requires a known positive duration.

Examples:

```rhai
seek(90, unit::SEC);
seek(1.5, unit::MIN);
seek(50, unit::PERCENT);
seek_by(-10, unit::SEC);
practice(80, 95, unit::SEC);
```

Keep ordinary bounds/state rules in the backend. Successful manual seeking outside active practice disables it before normal playlist handling. Percentage values always refer to the full track, not the selected interval. No automatic conversion from strings or implicit unit guessing is provided for the new APIs.

### 3. One bottom progress row, with explicit preview

Status: proposed baseline from the discussion.

Place the progress row above the existing status and command rows in both views; reuse existing title/metadata rows. Display elapsed time, the bar, and total time. The bar never takes keyboard focus. In narrow layouts prioritize time labels, and disable pointer seeking when a usable track cannot be drawn. Unknown duration is shown as `--:--`, with no fabricated ratio.

The bar's first and last usable cells map to zero and full duration. A left-button press starts a tentative gesture without seeking. Release without movement commits a click; movement updates the target marker/time, and release commits one seek. This reconciles click-to-seek with drag preview without sending an initial unwanted seek on mouse-down. Clamp horizontal movement outside the track while the gesture is active; never start a gesture outside the bar.

Audio keeps its existing playback state during preview. Show the preview as a target distinct from actual elapsed time; lyric highlighting stays on actual playback. Esc cancels the gesture before other Esc behavior. Resize, track/session replacement, pending track load, lost usable duration, or entry into an input/modal context cancels the gesture. Releasing after cancellation does nothing.

### 4. Respect existing input contexts

Status: proposed compatibility decision.

Command/search input and chooser handling take precedence over playback bindings and bar interaction. Details-focused Left/Right retain horizontal scrolling. Playback bindings apply in ordinary library browsing and the lyrics view without focusing the bar. Existing lyric/list wheel behavior and lowercase Vim navigation remain unchanged. Do not implicitly interpret numeric navigation prefixes as seek multipliers.

### 5. Completion, not submission, updates seek feedback

Status: proposed technical decision.

Keep the native clock and existing status polling. Add a small asynchronous completion mechanism for submitted seeks so the UI can distinguish queue acceptance from completion, failure, or a stale request. Associate completion with both the request and expected playback identity. Do not infer success from an unrelated status refresh or an empty error field. Existing stale-session handling must not let an old request move a newly loaded track or recenter its lyrics.

On confirmed successful seek for the relevant session, display the returned status and resume lyric following. On rejection, display the error and retain current playback/lyric-follow intent. Native decode/output failures continue to follow the existing backend failure contract; this design does not promise rollback after decoding has begun. Continue rendering and handling input while requests are pending; retain bounded queues and report submission failures.

### 6. A–B APIs for practice

Status: continuous automatic looping is user-confirmed; exact function names and lifecycle are proposed.

`mark_a()` records the execution-time position, clears B, and disables an existing loop without seeking. `mark_b()` records B, validates the pair, seeks A, and enables looping. `practice(80, 95, unit::SEC)` sets both endpoints and enables identically. `practice_status()` is read-only. `practice_off()` retains markers/position; `practice_on()` enables a retained valid pair and returns to A; `practice_clear()` removes markers without seeking.

Current-position marks retain backend millisecond precision. Require a loaded track with known positive duration and `0 <= A < B <= duration`; marking A alone requires A before the end. Reject missing A, invalid bounds, unavailable duration, pending replacement, or stale identity without modifying the prior healthy state. Validate a complete pair before publishing it. Establishment succeeds only after the seek to A succeeds and lands before B; native failures follow the existing failure contract and disable automatic retry.

Enabling from Playing continues playing from A. Enabling from Ready, Paused, or Finished positions at A without starting audio; a subsequent play begins practice. `[` and `]` execute `mark_a()` and `mark_b()` callbacks in the same normal browsing contexts as seek keys. Prompts treat them as text; choosers consume input first. No additional focus mode is introduced.

The bar keeps the full-track scale and displays A/B and a highlighted interval, with a distinct active versus retained/off state. Use an English status label such as `Loop A-B 1:20–1:35`; when space is limited retain readable status even if both marks occupy one cell. Ordinary bar dragging continues to seek and does not edit the markers. Lyric highlighting follows actual playback on each return; an automatic wrap preserves an existing manual-scroll grace period instead of forcibly cancelling it. Explicit activation or a manual seek restores following as described above.

### 7. The playback service owns looping

Status: proposed implementation decision.

Store optional A/B markers and an enabled flag against the current playback identity in the per-library session. Publish them in snapshots so multiple clients agree. Automatic seeks within the track do not replace its public identity or discard its markers. Pause/resume and detach preserve state; stop (including quit), shutdown, or successful source replacement clears it. A failed replacement that retains the healthy old source retains its practice state. Saved ranges across service restart are outside this change.

Evaluate an enabled interval on a periodic worker deadline, targeting the existing 40 ms cadence, including while handling commands. Do not depend exclusively on `recv_timeout` expiry, which can starve under traffic. While playback is advancing, reaching/passing B triggers an absolute seek to A before normal Finished/playlist handling. If B equals track duration, seek back and explicitly resume as necessary because native seek from Finished returns Paused. Recheck current intent and process pause/stop through the same serialized worker; a boundary event must never undo a completed pause or stop.

Practice looping temporarily overrides automatic playlist advance and single-track/list repeat without changing the selected playlist mode. Explicit next/previous/load still work and clear markers only after successful replacement. A manual seek whose actual landing is inside `[A, B)` retains looping; a successful landing outside disables it and retains markers. Apply that decision atomically in the worker before boundary handling, including requests from other clients. Cancelled drag previews never change the loop. Failure to return to A disables looping, reports an error, and suppresses normal playlist advance for that failed wrap rather than repeatedly retrying or hiding the failure by moving on.

This first implementation reuses native seeking. It can play slightly beyond B before the worker observes the boundary, and decoding/output buffers can produce a short transition. There is no deliberate silence, sample-exact endpoint, or seamless-loop promise. Verify real audible behavior for memorization; a native output boundary mechanism is a later option if the measured result is inadequate.

### 8. Deferred bindings and shared registration

Status: proposed implementation of the user's key-to-command direction.

Provide `bind(context, key, callback)` and `unbind(context, key)` as application functions. Initially accept the `normal` context with canonical names such as `Left`, `Right`, `[`, and `]`; unsupported contexts/keys report an error. Binding replaces only that context/key entry, and unbinding removes it. Neither operation invokes the callback. Defaults use the same mechanism:

```rhai
bind("normal", "Left",  || seek_by(-10, unit::SEC));
bind("normal", "Right", || seek_by(10, unit::SEC));
bind("normal", "[", || mark_a());
bind("normal", "]", || mark_b());
```

Retain `FnPtr`, its compilation environment/AST, and captured values. Prompt/search/chooser handling and existing details-panel arrow navigation take precedence. Generic navigation, emergency quit, and editing controls are not all migrated into user bindings. Preserve existing numeric-prefix behavior rather than inventing seek multipliers. Bindings are local to the TUI runtime; practice state remains shared in the playback service. A future configuration file can reuse the same evaluator and registrations, but startup file discovery/persistence is outside this baseline.

### 9. Contextual completion in the existing editor

Status: proposed integration based on the shared editor and explicit command/script entry modes.

Keep `CommandLine` editing and the Ratatui popup. Use one completion interface accepting text, UTF-8 byte cursor, frontend capabilities, and an immutable registry/catalog snapshot. Determine mode from the same entry router used for submission. Ordinary input completes registered command names, explicit aliases, subcommands, and schema-declared arguments. Only a leading `=` selects Rhai completion; do not infer mode from parentheses, valid syntax, or failed evaluation. Empty input lists commands and the `=` script entry. Ordinary command-name insertion does not add parentheses.

Build both views from shared registration records. A command descriptor contains its name, aliases, argument schema, documentation, required capabilities, and callback reference; API registration supplies Rhai signatures, namespaces, constants, and parameter roles. Rhai metadata does not automatically provide command aliases or all semantic roles, so declare those explicitly alongside registrations. Do not maintain independent execution and completion name lists. Group numeric overloads for readable signatures and publish successful user command/function registration updates without executing callbacks during completion.

Inside the script body, use a tolerant context scanner for identifiers, namespaces, nested delimiters, strings/escapes, comments, and active argument position. Count only commas at the active call depth. Incomplete code need not compile. Unsupported contexts receive conservative suggestions or none; full type inference and an LSP remain deferred. Completion must translate body-relative ranges back to full editor byte offsets, preserving the leading `=`, surrounding whitespace, suffix, and Unicode boundaries.

| TUI input (`▏` marks the cursor) | Candidates/help |
| --- | --- |
| `:pl▏` | Registered `play` / `playlist` command entries; no parentheses |
| `:s▏` | `s` only if explicitly registered, plus other matching registered names |
| `:=se▏` | `seek`, `seek_by` with signatures |
| `:=unit::▏` | Exported unit values |
| `:=seek(90, ▏` | Qualified units and active-parameter hint |
| `:=seek_by(-10, unit::▏` | Time units only |
| `:=practice(80, 95, unit::S▏)` | Replace only `S` with `SEC` |
| `:import "~/Mu▏` | Paths escaped for ordinary command quoting |
| `:=import_audio("~/Mu▏"); pause();` | Paths escaped as Rhai strings, preserving the rest of the script |

Candidates carry display/insertion text, exact replacement range, resulting cursor, kind, and signature/help. First Tab expands a common prefix and shows ambiguous candidates; later Tab/Shift-Tab cycles using the original completion session. Only script function completion inserts missing parentheses and reuses existing delimiters. Enter submits explicitly; Esc cancels without execution. Editing, moving the cursor, or changing/removing the script marker invalidates completion. Small terminals may omit hints/popups without corrupting input.

Reuse path discovery/filtering and `~/` expansion only for registered path arguments. Ordinary commands use the common command tokenizer's quote/escape rules, while script strings use Rhai escaping. Neither mode may evaluate strings, callbacks, or getters to infer candidates; symbols and units never scan the filesystem. `rustyline`/`reedline` would still need this semantic provider, so retain the current editor and terminal ownership.

### 10. Thin command entry backed by Rhai

Status: retaining Vim-style entry and unified execution follows the latest discussion. The short script marker and user-command API below are proposed implementation choices.

Route trimmed interactive input deterministically before execution:

1. Empty input is a no-op.
2. Leading `=` selects a script: strip only that marker, retain the body/source offsets, and submit the remaining source to the Rhai worker. A bare marker is empty script input and has no effect.
3. An entire ASCII decimal integer is a navigation command invoking the same `goto_line` API through the runtime. Require a positive representable row number; zero/overflow produce an English argument error, and valid rows use the existing navigation bounds behavior. In the REPL, navigation reports unavailable context.
4. Otherwise resolve an exact registered command name/alias and declared subcommands, tokenize its arguments, and invoke its retained Rhai callback with typed values.

Unknown commands and invalid arguments report command errors. `:play()` is not implicitly a script; the error can suggest `:play` or `:=play()`. Script errors never retry input as commands. Explicit aliases such as `q` remain registry entries, not inferred prefixes. A short name such as `s` has no predefined meaning unless registered; this change does not implement Vim substitution or its broader Ex language.

Register Rhai APIs for every current TUI/REPL operation, including playback, quit/detach, import, playlist, volume, queries, maintenance, help, and navigation. Retain useful existing command forms as first-class command descriptors whose callbacks call those APIs. A command descriptor declares argument count, primitive conversion, optional/default values, supported flags/subcommands, and completion roles. Use one ordinary tokenizer with whitespace separation, quoted arguments, and documented escaping; do not evaluate argument expressions or create duration/percentage grammars. Pass values directly to the callback rather than interpolating input into Rhai source or reconstructing argv for the old dispatcher. Reuse typed Rust operations beneath the API so behavior is implemented once.

| Ordinary entry | Equivalent script entry |
| --- | --- |
| `:play`, `:pause`, `:stop` | `:=play()`, `:=pause()`, `:=stop()` |
| `:q` / `:quit`, `:detach` | `:=quit()`, `:=detach()` |
| `:import "path"` | `:=import_audio("path")` |
| `:23` | `:=goto_line(23)` |
| Playlist subcommands | Calls in `playlist::` |
| Volume subcommands | `volume::get/set/inc/dec` calls |
| REPL queries, modes, next-once, maintenance | Registered query/control functions and `library::` calls |

Do not automatically expose every script API as a positional command. New seek/practice argument expressions use `:=seek(...)` / `:=practice(...)` or callbacks bound to convenient commands/keys. This keeps numeric/unit semantics in one API and avoids another seek grammar.

Provide session-local `command(name, callback)` and `uncommand(name)` as the proposed minimal user-command API. Initially user commands take no command-line arguments; closures capture configuration or call script functions. Names follow `[A-Za-z][A-Za-z0-9_]*`. Registration does not run a callback, replaces only an existing user entry of the same name, and rejects collisions with builtin names/aliases. Removal affects only user entries. Numeric entry and `=` are reserved by routing. Retain callback ASTs/captures as for key bindings. Parameterized user-command schemas and persistent configuration loading are future extensions; builtin command schemas cover current operations.

For example, `:=command("s", || seek_by(10, unit::SEC))` makes `:s` a session-local forward-seek command; this is an example, not a default meaning for `s`. The same callback can be bound to a key. Dispatch callbacks directly on the serialized worker, with the same ordering, operation budget, and bounded waits as scripts; do not recursively submit a script and wait on the same worker. Direct emergency stop/quit controls retain their independent path to the shared operations when a worker is busy.

The TUI colon opens the editor and is not part of its source. REPL input uses the same routing without that UI colon. UI-only APIs/commands declare capabilities, are omitted from unsupported completion, and return clear context errors if called. Chooser answers and normal-mode numeric prefixes remain separate UI interactions. One-shot process arguments such as `muse play` keep their existing CLI boundary and call shared typed operations. Generate interactive help from command/API registrations and keep CLI help specific to argv.


### 11. Readline-style editing, history, and dot repeat

Status: editing/history/repeat are user-requested. The following behavior is the proposed first-version contract; readline denotes the interaction style, not a mandatory dependency on GNU Readline or a replacement terminal event loop.

Keep editing actions separate from command routing and Rhai execution. Extend the shared `CommandLine` buffer with cursor movement, deletion/kill, yank, and history replacement. The TUI and REPL map keys to the same editor actions; Crossterm/Ratatui retain terminal ownership. A future library adapter can implement these actions if useful without changing completion or execution semantics. This change does not require selecting or adding an editor library.

| Key inside command input | Action |
| --- | --- |
| Left/Right, Ctrl-B/F | Move one Unicode character |
| Home/End, Ctrl-A/E | Move to beginning/end |
| Alt-B/F | Move backward/forward by whitespace-delimited word |
| Backspace/Delete | Delete previous/next character |
| Ctrl-U/K | Kill from cursor to beginning/end |
| Ctrl-W | Kill preceding whitespace and preceding word |
| Ctrl-Y | Insert the last killed text at the cursor |
| Up/Down, Ctrl-P/N | Older/newer submitted command |
| Tab/Shift-Tab | Existing completion cycle |
| Enter / Esc | Submit / cancel as before |

Use UTF-8-safe character boundaries, including for non-ASCII text. A nonempty kill replaces one session-local yank slot; a kill at an empty boundary preserves it. A full kill ring, incremental reverse search, vi editing mode inside the prompt, and multiline editing are deferred. Editing and history movement invalidate completion and recompute ordinary/script mode from the resulting text. These key meanings apply only inside command input; existing search, chooser, normal-mode controls, emergency Ctrl-C, and REPL empty-line Ctrl-D behavior retain precedence.

Share history behavior between frontends, but keep each frontend session's history local. Store at most 200 nonempty submissions in insertion order, preserving exact editable text including the `=` marker (not the TUI's displayed colon). Record failed submissions too, so they can be corrected. Deduplicate adjacent identical submissions only; evict oldest entries at capacity. Cancelled/empty input, chooser answers, search terms, ordinary key callbacks, and dot replay add no entries. Persistence across process exit is deferred.

On the first Up from the newest position, save the current draft and cursor, then show the newest history item with its cursor at the end. Further Up/Down traverses older/newer entries without wraparound; Down past the newest restores the saved draft and cursor. An empty history leaves the buffer unchanged. Editing a recalled entry never changes stored history; navigating again replaces the working buffer, and Down to the draft still restores the original draft. Enter submits the edited text as a new command, while Esc ends browsing without changing history. History navigation never executes anything. Up/Down always navigate history inside command input, even with a completion popup; Tab/Shift-Tab remain candidate navigation.

Define dot repeat as repeating the latest successfully completed command submitted manually from the TUI prompt, including an ordinary command, numeric navigation, or `=` script. It is not a replay of the last physical key, pointer gesture, or a Vim edit transaction. Save exact source with its mode marker and a monotonically increasing submission id; publish it as the repeat target only after correlated successful completion, including any chooser/import workflow. Queue acceptance is not success. Failed, cancelled, empty, or stale results do not overwrite the target, and an older success cannot overwrite a newer successful submission. History and repeat state are separate: failed source remains recallable but is not the repeat target.

A plain `.` keypress in ordinary TUI browsing (library/details/lyrics) submits one fresh execution of that saved source through the same router/runtime. Display what is being repeated. Resolve variables, user-command registrations, selection, relative positions, and expected playback identity anew at execution; do not cache host effects or old service requests. For example, after `:=seek_by(10, unit::SEC)`, each deliberate `.` moves ten seconds from the current position. Repeating a script repeats all its statements; there is no special handling for variable/function definitions or a guarantee of the original result. If repeat fails, report the error and do not retry automatically. Repeat itself never replaces its source record or appends a history entry.

If no successful record exists, show an English notice without effects. While a manually submitted command or a dot replay is still pending, reject another dot invocation as busy rather than replaying an older command or creating an unbounded queue. Treat only initial keypress events as dot requests; ignore terminal autorepeat events. Numeric counts do not multiply dot execution in this version. Command/search input and choosers retain precedence, so a dot there is handled as input rather than replay. The REPL has history/editing but no normal-mode dot shortcut in this baseline; its input buffer always accepts `.` as text. These repeat controls are frontend behavior and do not introduce a new ordinary command grammar.

References: [Rhai metadata](https://rhai.rs/book/engine/metadata/export_to_json.html), [signature metadata limits](https://rhai.rs/book/engine/metadata/gen_fn_sig.html), [retaining callable functions](https://rhai.rs/book/language/fn-ptr.html), [custom host types](https://rhai.rs/book/rust/custom-types.html).

## Risks / Trade-offs

- Command history and execution results arrive at different times → keep history separate from success-based repeat state and correlate results by submission id.
- A replayed script may resolve different variables, selections, or registrations → rerun source explicitly, show it to the user, and document current-context behavior.
- Runtime metadata and completion can drift → declare them with the same API registration and verify function/constant parity.
- Incomplete scripts cannot reliably form a full AST → use conservative cursor-context analysis and test nested calls, quoted commas, comments, and Unicode edits.
- Native effects and script callbacks can block or outlive their source → retain compilation environments and bound/cancel operations on a dedicated script worker.
- Terminal-cell precision limits mouse placement → provide exact `seek(value, unit)` targets and label the preview time.
- Playback can change during a gesture or queued operation → cancel stale gestures and preserve expected playback identity through execution and completion.
- Holding an arrow key can fill the request queue → keep requests bounded and make failure visible; never convert relative requests using stale UI time.
- Bar layout reduces vertical content space → use one row and existing metadata; test short and narrow terminals.
- Native seek-to-end may advance the playlist → preserve it outside practice; active automatic wraps take precedence, including B at track duration.
- Polling may overshoot B and seeking may introduce a gap → schedule by deadline independently of command traffic and record real playback timing; do not advertise seamless looping.
- Pause, stop, or another client can race with a wrap → serialize intent, interval changes, and seeks in the shared worker and scope requests to playback identity.

## Migration Plan

Add the Rhai runtime/API catalog, explicit units, ordered host operations, and default bindings. Replace heuristic classification with explicit `=` script routing. Register existing commands and aliases as Rhai callbacks in both frontends, then remove duplicate business dispatchers. Preserve completed runtime/catalog and explicit-routing work; phase 4 completes callback-backed command execution. Complete mode-aware completion, shared editor/history actions, and success-correlated dot replay, then update help/history examples; do not auto-detect or rewrite historical unprefixed scripts. Then implement the progress bar and practice service state/scheduling against those APIs. Bump the service protocol for range state and percentage operations with existing explicit mismatch handling; never silently restart a playing older service. Validate default-feature-free native interfaces and keep scripting dependencies confined to the application crate. No library migration or native API expansion is planned. Reverting restores the previous command UI and requires a compatible service restart, without altering stored music.

## Open Questions

Optional extensions outside this implementation baseline:

- Where should persistent user configuration scripts live, and what should reload/override behavior be? The same evaluator and `bind` API are intended for them.
- Should user commands later accept declarative argument schemas or allow explicit builtin overrides?
- Should history later persist across sessions or support incremental reverse search, and should dot repeat support counts or REPL-specific invocation?
- Should command prefill have a separate binding action?
- Should later completion include full local-variable/type analysis or an LSP for external configuration editors?
- Should practice add lyric-line selection, finer endpoint controls, saved passages, speed changes, or optional gaps?
