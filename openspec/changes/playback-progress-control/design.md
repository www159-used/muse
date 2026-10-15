## Context

The TUI currently binds Left/Right directly to relative `SeekTarget` calls. `command.rs` implements completion locally: name matching only at the end of the first token, special handling for import paths, and replacements that rebuild the input. Ratatui already renders a candidate popup, and the standalone REPL shares the editor but keeps a separate command vocabulary. Colon commands are parsed separately by `BuiltinCommand`; several transport commands use a synchronous service call. The service client already queues seek requests with the expected current playback identity, and the native backend supports absolute and execution-time relative seeks.

Lyrics select the last timestamp at or before the reported playback position. Manual lyric scrolling suspends centering for three seconds. A new seek does not require a different lyric clock or parser.

The user also wants to practice passages to memorize melody and lyrics and explicitly selected continuous A–B looping. The playback worker currently polls on a 40 ms receive timeout and advances playlists on Finished. Practice scheduling must live in that service, take precedence over natural advancement, and continue running when requests keep arriving.

## Goals / Non-Goals

**Goals:**
- Make Rhai calls, deferred playback bindings, and pointer seeking share registered application operations and completion feedback.
- Reuse API definitions for execution, function signatures, namespace symbols, parameter suggestions, and path completion.
- Show one progress row in library and lyric views, without another keyboard focus mode.
- Keep input responsive while the service seeks, and render the actual reported position after completion.
- Mark a passage and repeat it continuously, with progress and lyrics following each return to A.

**Non-Goals:**
- General migration of all navigation and playback keys to a configurable command system.
- Persistent user keymap/configuration files, command prefill, a full language server, or CLI/REPL language migration. Session-local function bindings are included.
- Custom duration literals, time-string parsing, or a new command language parser alongside Rhai.
- Continuous audible scrubbing, a new playback clock, lyric format changes, or audio backend replacement.
- Audio export, saved ranges, lyric-line selection, playback speed controls, inserted silences, repeat counts, and sample-exact or gapless looping.

## Decisions

### 1. Standard Rhai and registered application APIs

Status: scripting and function bindings are the user's direction; runtime architecture is proposed.

Use standard Rhai syntax in the colon prompt. Do not add a `:rhai` prefix or custom `10s` literals. Register native application functions once, with names, parameter/return metadata, descriptions, and any semantic completion annotations. Keep script execution outside the render/event loop and outside audio callbacks. A dedicated serialized script worker owns its engine, scope, compiled functions, and retained callbacks; bounded input queues and Rhai execution limits keep the UI responsive. Native calls also need bounded waits/cancellation because a script operation budget does not interrupt a blocking host function.

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

Status: proposed integration based on inspection of `command.rs` and `ui.rs`.

Keep `CommandLine` editing and the Ratatui popup. Introduce a completion provider accepting text, UTF-8 byte cursor, syntax mode, and an immutable symbol/catalog snapshot. Return candidates with label, insertion text, exact replacement range, kind, detail/signature, and resulting cursor. Replace only the active token/argument range and preserve surrounding expressions, delimiters, and Unicode text. Cycling must use a stable original edit range/session instead of repeatedly reconstructing the full line.

Build the catalog from the same API registration data used by Rhai, using its `metadata` feature for functions, namespaces, and script-defined function signatures. Parameter names/documentation need explicit registration or plugin metadata; ordinary function registration alone does not provide all of them. Register exported constant names/values and semantic roles such as `unit`, `path`, `context`, and `key` alongside their APIs: function metadata alone does not enumerate these choices. Keep one registration owner and verify catalog/runtime parity; do not maintain another hard-coded command-name list. Group numeric overloads under one user-facing signature, while keeping their actual registration accurate.

Use a tolerant lexical/context pass for incomplete input, tracking strings/escapes, comments, parentheses, brackets, braces, and argument separators. It need not evaluate or fully compile an unfinished expression such as `seek(90, unit::`. Count commas at the active call depth, not inside nested calls or strings. Recognize identifiers/namespaces and known direct call signatures. If context or argument types are uncertain, offer conservative name suggestions or no result rather than executing code to infer a value. Full type inference, arbitrary object-member inference, and incomplete local-scope analysis are deferred; already registered script functions and published session symbols can participate without running them.

| Cursor context (`▏` marks the cursor) | Candidates/help |
| --- | --- |
| `se▏` | `seek`, `seek_by` with signatures |
| `unit::▏` | Exported unit values |
| `seek(90, ▏` | Qualified unit values plus active-parameter hint |
| `seek_by(-10, unit::▏` | Time units only |
| `practice(80, 95, unit::S▏)` | Replace `S` with `SEC`, retaining `)` |
| `import_audio("~/Mu▏"); pause();` | Path candidates for the annotated argument; preserve the closing string/call and following statement |
| A comment or an ordinary string argument | No code-symbol suggestions |

When a unique function name is completed, insert parentheses only if absent and place the cursor inside them; reuse existing delimiters. Empty prompts list callable application functions/namespaces and labeled legacy entry points. At an uncommitted first-token prefix, show the union of compatible function/legacy candidates; selecting or typing a complete form determines the provider mode. First Tab expands the common prefix and opens candidates when ambiguous; subsequent Tab/Shift-Tab cycles. Candidate edits never execute code; Enter explicitly submits the resulting input, and Esc cancels the command prompt as before. Show signature help while inside a recognized call and highlight the active parameter; numeric arguments receive hints rather than invented numeric candidates. Editing/cursor movement invalidates the completion session. Tiny terminals omit hints/popups without corrupting input or layout.

Reuse existing path scanning/filtering and `~/` expansion, adapting escaping to Rhai double-quoted strings rather than shell quoting. Completion must work before a closing quote is typed and preserve the rest of the call. Only a path-annotated parameter may trigger filesystem completion; symbol and unit completion must not access the filesystem. Completion never evaluates scripts, callbacks, or host getters. Snapshot catalog updates after a successful registration/definition without blocking rendering.

Rhai metadata supplies symbols, not a complete editor or language server. `rustyline` and `reedline` offer line editing and completion interfaces but would still need semantic providers and TUI integration. Reusing the current editor avoids replacing terminal ownership just to add this provider.

### 10. Existing commands and migration boundary

Status: proposed compatibility decision.

Provide Rhai adapters for the existing application operations, including playback, quit/detach, import, playlist management, volume, and numeric navigation. Reuse current Rust operation handlers so confirmations, import jobs, errors, and exit semantics are preserved. Use valid Rhai identifiers (`import_audio`, not reserved `import`; `practice`, not reserved `loop`). API names/signatures and their completion metadata are defined together during registration.

Keep the currently shipped colon forms through explicit compatibility recognition before evaluation, including numeric navigation and bare legacy commands. Recognition is limited to existing registered forms; it must distinguish a legacy token from a Rhai call such as `play()`. Once input is classified as Rhai, a parse/runtime error never retries it through another dispatcher. Legacy completion keeps its current rules and path quoting; Rhai completion uses the provider above. New seek/practice features have only function syntax, so no additional text mini-grammar is created. Standalone CLI/REPL keep their existing language and provider mode in this change, with no Rhai-only candidates advertised there.

References: [Rhai metadata](https://rhai.rs/book/engine/metadata/export_to_json.html), [signature metadata limits](https://rhai.rs/book/engine/metadata/gen_fn_sig.html), [retaining callable functions](https://rhai.rs/book/language/fn-ptr.html), [custom host types](https://rhai.rs/book/rust/custom-types.html).

## Risks / Trade-offs

- Runtime metadata and completion can drift → declare them with the same API registration and verify function/constant parity.
- Incomplete scripts cannot reliably form a full AST → use conservative cursor-context analysis and test nested calls, quoted commas, comments, and Unicode edits.
- Native effects and script callbacks can block or outlive their source → retain compilation environments and bound/cancel operations on a dedicated script worker.
- Terminal-cell precision limits mouse placement → provide exact `:seek` targets and label the preview time.
- Playback can change during a gesture or queued operation → cancel stale gestures and preserve expected playback identity through execution and completion.
- Holding an arrow key can fill the request queue → keep requests bounded and make failure visible; never convert relative requests using stale UI time.
- Bar layout reduces vertical content space → use one row and existing metadata; test short and narrow terminals.
- Native seek-to-end may advance the playlist → preserve it outside practice; active automatic wraps take precedence, including B at track duration.
- Polling may overshoot B and seeking may introduce a gap → schedule by deadline independently of command traffic and record real playback timing; do not advertise seamless looping.
- Pause, stop, or another client can race with a wrap → serialize intent, interval changes, and seeks in the shared worker and scope requests to playback identity.

## Migration Plan

Add the Rhai runtime/API catalog, explicit units, ordered host operations, and default bindings. Retain shipped colon behavior through explicit adapters while completing the script editor/provider. Then implement the progress bar and practice service state/scheduling against those APIs. Bump the service protocol for range state and percentage operations with existing explicit mismatch handling; never silently restart a playing older service. Validate default-feature-free native interfaces and keep scripting dependencies confined to the application crate. No library migration or native API expansion is planned. Reverting restores the previous command UI and requires a compatible service restart, without altering stored music.

## Open Questions

Optional extensions outside this implementation baseline:

- Where should persistent user configuration scripts live, and what should reload/override behavior be? The same evaluator and `bind` API are intended for them.
- When should legacy colon forms be deprecated, and should the standalone REPL adopt Rhai too?
- Should command prefill have a separate binding action?
- Should later completion include full local-variable/type analysis or an LSP for external configuration editors?
- Should practice add lyric-line selection, finer endpoint controls, saved passages, speed changes, or optional gaps?
