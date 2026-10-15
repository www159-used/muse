## ADDED Requirements

Unless a scenario shows a full prompt, bare function expressions below denote Rhai source executed after `:=` in the TUI or `=` in the REPL, or by a registered callback.

### Requirement: Explicit command entry and shared Rhai execution

The TUI SHALL retain ordinary Vim-style commands, explicit aliases, and numeric line navigation. A leading `=` in editor input SHALL select standard Rhai source (`:=` including the TUI prompt); the REPL SHALL use the same routing without the UI colon. Empty input or an empty script body SHALL have no effect. Otherwise an entire ASCII decimal integer SHALL invoke the navigation callback, and other input SHALL resolve an exact command name/alias with declared subcommands and arguments. Unknown commands and invalid arguments SHALL report English command errors without attempting script evaluation. Script syntax/runtime errors SHALL NOT retry input as commands. Unprefixed function syntax SHALL NOT implicitly select script mode.

Both frontends SHALL share a command/API registry and runtime implementation. Ordinary commands and key bindings SHALL invoke retained Rhai callbacks through the serialized script worker; callbacks and direct scripts SHALL use the same registered Rust application APIs and typed operation layer. Command arguments SHALL be passed as values, not interpolated into script source or reconstructed for an old argv dispatcher. Execution SHALL preserve host-call ordering and leave input responsive. An unhandled failure SHALL abort remaining statements without repeating or claiming rollback of earlier effects. Independent emergency stop/quit controls SHALL remain responsive even when the script worker is busy. UI-only APIs/commands SHALL declare capabilities, be omitted from unsupported completion, and return clear context errors if invoked. One-shot CLI argument syntax SHALL remain unchanged.

#### Scenario: Short script entry
- **WHEN** the user enters `:=seek(90, unit::SEC)` and presses Enter
- **THEN** the source after `=` is evaluated by Rhai and calls the registered seek API

#### Scenario: Ordinary command and script equivalence
- **WHEN** the user invokes `:play` or `:=play()` in equivalent playback states
- **THEN** each reaches the same registered playback operation with matching effects and errors

#### Scenario: Numeric navigation and numeric expression
- **WHEN** the TUI user enters `:23` and then `:=23`
- **THEN** the first invokes `goto_line(23)` through the runtime, while the second evaluates a numeric value without navigating

#### Scenario: Invalid navigation number
- **WHEN** a numeric command is zero or overflows the supported row type
- **THEN** an English argument error is reported without navigation or script fallback

#### Scenario: No heuristic script mode
- **WHEN** the user submits `:play()` or an unknown command name
- **THEN** a command error is reported without executing a script, and feedback may suggest an appropriate command or `:=` entry

#### Scenario: Ordered practice startup
- **WHEN** the user submits `:=practice(80, 95, unit::SEC); play();`
- **THEN** play executes after successful practice establishment, and an unhandled establishment error prevents play

#### Scenario: Shared REPL entry
- **WHEN** equivalent playback states receive TUI `:=seek(50, unit::PERCENT)` and REPL `=seek(50, unit::PERCENT)`
- **THEN** both apply the same validation, operation ordering, and effects

#### Scenario: Frontend capability boundary
- **WHEN** the user invokes numeric navigation or `=goto_line(23)` in the REPL
- **THEN** an unavailable-context error is returned without changing playback

#### Scenario: Failure does not retry input
- **WHEN** a script or command callback fails after a completed host operation
- **THEN** the error is displayed without repeating the effect or trying another dispatcher

### Requirement: Registered ordinary commands and aliases

All existing interactive operations SHALL have Rhai APIs and registered ordinary command mappings, including playback, import, playlist, volume, queries, mode, next-once, maintenance, help, quit/detach, and navigation. Descriptors SHALL declare names, explicit aliases, callbacks, argument schemas, documentation, completion roles, and required capabilities. A shared tokenizer SHALL support whitespace-separated arguments, quoted text, and escaping, with schema-driven arity, primitive conversions, defaults, flags, and subcommands. Ordinary command arguments SHALL NOT evaluate expressions or implement a time/percentage mini-language. Supported selection, confirmation, progress, cancellation, and exit behavior SHALL be retained through the shared operation layer. Automatic prefix abbreviations, Vim substitution syntax, Ex ranges, and pipelines are outside this change.

#### Scenario: Existing import command
- **WHEN** the user submits `:import "path with spaces"` or `:=import_audio("path with spaces")`
- **THEN** both call the same import operation and preserve progress, cancellation, and error handling

#### Scenario: Explicit alias
- **WHEN** the user submits the registered `:q` alias
- **THEN** it invokes the same quit callback as `:quit` without maintaining a second exit implementation

#### Scenario: Argument data remains data
- **WHEN** a quoted import path contains punctuation resembling script statements
- **THEN** its decoded value is passed as a path argument and no part of it is evaluated as code

#### Scenario: Complete operation coverage
- **WHEN** an existing playlist, volume, query, mode, next-once, or maintenance command is invoked
- **THEN** its registered callback preserves the supported options, defaults, and selection/confirmation behavior through the corresponding API

### Requirement: Session-local user commands

`command(name, callback)` SHALL register a deferred zero-argument user command, and `uncommand(name)` SHALL remove a user command. Names SHALL match `[A-Za-z][A-Za-z0-9_]*`; builtin command names/aliases SHALL be protected from replacement/removal. Registration SHALL replace an existing user command of the same name without executing it and retain the callback's compilation environment and captures. User command execution SHALL use the same worker, limits, and completion/error handling as key callbacks and direct scripts. Successful registration/removal SHALL update command completion. User commands SHALL be session-local; argument schemas for user-defined commands and persistence are deferred.

#### Scenario: Register a short command
- **WHEN** the user submits `:=command("s", || seek_by(10, unit::SEC))`
- **THEN** no seek occurs during registration, `s` becomes discoverable in command completion, and a later `:s` seeks forward from execution-time position

#### Scenario: Retained callback and removal
- **WHEN** a registered user command runs after its defining prompt closes and is then removed with `uncommand`
- **THEN** its captures remain valid for execution, and subsequent command lookup and completion no longer find it after removal

#### Scenario: Protected names and arity
- **WHEN** registration attempts to replace a builtin/alias, or a zero-argument user command receives arguments
- **THEN** a clear error is reported without changing the existing registration or executing a callback

### Requirement: Numeric seek arguments and unit values

Both interactive frontends SHALL expose the Rhai APIs `seek(value, unit)` for absolute positioning and `seek_by(value, unit)` for relative movement. It SHALL export host-defined unit values `unit::MS`, `unit::SEC`, `unit::MIN`, `unit::HOUR`, and `unit::PERCENT`. Unit arguments SHALL be registered Unit values, not strings. Numeric arguments SHALL accept integers and finite floating values with checked conversion and round-to-nearest millisecond normalization. Absolute values SHALL be nonnegative; percentages SHALL be within 0 through 100 and require known positive duration. Relative movement SHALL accept signed time values, reject PERCENT in this version, and use execution-time playback position. Percentage resolution SHALL use the matching playback session in the service. Unsupported types, non-finite numbers, overflow, and invalid units SHALL fail before mutation. New APIs SHALL NOT infer units or parse time/percentage strings.

#### Scenario: Absolute time with units
- **WHEN** the user executes `seek(90, unit::SEC)` or `seek(1.5, unit::MIN)`
- **THEN** the player requests an absolute position of 90 seconds and displays the backend-reported position after completion

#### Scenario: Relative movement
- **WHEN** the user executes `seek_by(-10, unit::SEC)`
- **THEN** the player requests movement ten seconds backward from the execution-time position

#### Scenario: Percentage position
- **WHEN** the user executes `seek(50, unit::PERCENT)` for a track with known positive duration
- **THEN** the service requests the midpoint of that same full track

#### Scenario: Invalid target or unit
- **WHEN** a seek receives a string unit, a time string, a missing argument, a negative absolute value, an invalid percentage, a non-finite/overflowing value, or PERCENT for relative movement
- **THEN** the TUI reports an English error and no seek mutation is submitted

#### Scenario: Unknown duration
- **WHEN** the loaded track has no known positive duration
- **THEN** percentage seeking reports unavailable duration while valid time-based requests remain available

#### Scenario: No loaded source
- **WHEN** a seek is requested with no track loaded
- **THEN** an English error is shown without starting a selected track

### Requirement: Deferred function bindings

The TUI SHALL expose session-local `bind(context, key, callback)` and `unbind(context, key)`. The initial supported context SHALL be `normal`; unsupported context/key values SHALL report an error. Binding SHALL store a callable without executing it, replace an existing mapping for the same context/key, and retain the compilation environment needed for later invocation. Unbinding SHALL remove that mapping. Default Left/Right bindings SHALL invoke `seek_by(-10, unit::SEC)` and `seek_by(10, unit::SEC)`; `[` and `]` SHALL invoke `mark_a()` and `mark_b()`. Binding callbacks SHALL execute at keypress time. Command/search input, choosers, and existing details-focused arrow navigation SHALL retain precedence, and the bar SHALL require no keyboard focus.

#### Scenario: Register then invoke a callback
- **WHEN** the user submits `bind("normal", "Right", || seek_by(5, unit::SEC))`
- **THEN** playback does not move during registration, and a later Right press in normal song browsing moves five seconds from the then-current execution-time position

#### Scenario: Callback lifetime
- **WHEN** a binding created by an earlier evaluated script is invoked after that command prompt closes
- **THEN** its function and captured values remain callable without recompiling a command string at keypress time

#### Scenario: Remove a binding
- **WHEN** the user unbinds a custom normal-mode key and later presses it
- **THEN** the removed callback is not invoked

#### Scenario: Input takes precedence
- **WHEN** the user presses Left/Right or types brackets in an active prompt
- **THEN** the prompt handles cursor movement or text entry without invoking playback bindings

#### Scenario: Details navigation remains available
- **WHEN** the details panel has keyboard focus and the user presses Left or Right
- **THEN** the panel handles horizontal scrolling without submitting a seek

### Requirement: Registration-driven command completion

Both interactive frontends SHALL use a shared completion interface and registry with explicit ordinary-command and Rhai modes. The same leading `=` rule used by execution SHALL select script completion. Ordinary mode SHALL offer registered command names, explicit aliases, subcommands, and schema-declared arguments without inserting function parentheses. Script mode SHALL offer Rhai symbols/signatures and parameter roles. No mode SHALL be selected by trying to execute or compile the input. The Rhai completion catalog SHALL derive callable names, signatures, and namespaces from application registrations and available script metadata. Exported unit symbols and semantic parameter roles SHALL be declared alongside the same APIs. Numeric overloads SHALL be grouped for presentation without hiding valid callable forms. Completion SHALL use the current cursor context to suggest functions, namespaces, qualified unit values, and parameter-specific candidates; recognized calls SHALL display signature help indicating the active argument. Unit candidates for `seek_by` SHALL exclude PERCENT. Numeric parameters SHALL receive signature help rather than fabricated numeric suggestions. Available registered script functions SHALL become discoverable without invoking them. Completion SHALL NOT execute scripts, callbacks, or host getters, and non-path contexts SHALL NOT scan the filesystem.

#### Scenario: Ordinary command discovery
- **WHEN** the user requests completion at an empty prompt or after `:pl`
- **THEN** registered ordinary commands/aliases and the script entry are discoverable, prefix matches are offered, and command insertion adds no function parentheses

#### Scenario: Explicit completion mode
- **WHEN** the user adds or removes a leading `=` while completing input
- **THEN** the previous completion session is invalidated and candidates follow the new explicit mode without mixing command names with script-only functions

#### Scenario: Function discovery
- **WHEN** the user presses Tab after `:=se`
- **THEN** `seek` and `seek_by` appear with signatures from the API catalog and nothing executes

#### Scenario: Namespace completion
- **WHEN** the user presses Tab after `:=unit::`
- **THEN** the exported unit names are offered without running any application operation

#### Scenario: Parameter-specific values
- **WHEN** the user presses Tab at `seek(90, ` or `seek_by(-10, `
- **THEN** qualified unit values and an active-unit-parameter hint are offered, with PERCENT excluded for the relative API

#### Scenario: New API registration
- **WHEN** an application function and its metadata are registered or a session script function becomes available
- **THEN** completion discovers it from the updated catalog without requiring another hard-coded command-name entry

#### Scenario: Numeric parameter hint
- **WHEN** the cursor is in the value argument of `seek(`
- **THEN** the signature identifies that argument and completion does not invent a list of playback positions

### Requirement: Incomplete-input completion and precise edits

The editor SHALL provide conservative completion for incomplete Rhai input, including unclosed calls and strings. Context recognition SHALL account for nested delimiters, quoted/escaped text, comments, and commas at the active call depth. Candidate application SHALL replace only the relevant UTF-8-safe range, preserve surrounding text, and set the cursor appropriately. Script-body ranges SHALL be mapped to editor offsets while preserving the leading `=` marker and whitespace; only script function completion SHALL insert parentheses. Unique function completion SHALL add missing parentheses and position the cursor inside without duplicating existing delimiters. First Tab on ambiguous input SHALL expand the common prefix and show candidates; subsequent Tab/Shift-Tab SHALL cycle with wraparound. Edits or cursor movement SHALL invalidate the completion session. Enter SHALL explicitly submit the resulting input and Esc SHALL cancel the prompt. Unsupported/ambiguous contexts SHALL leave input intact or offer conservative name suggestions without evaluation. Small terminals SHALL degrade hints/popups without corrupting input.

#### Scenario: Preserve script marker
- **WHEN** completion replaces a token in `=practice(80, 95, unit::S)`
- **THEN** the leading `=`, the closing parenthesis, and all text outside that token remain intact

#### Scenario: Complete in the middle of a call
- **WHEN** the cursor is after `S` in `practice(80, 95, unit::S)` and the user completes SEC
- **THEN** only that token is replaced, the closing parenthesis remains, and the result is `practice(80, 95, unit::SEC)`

#### Scenario: Nested arguments and strings
- **WHEN** the cursor is in the outer unit argument of `seek(helper("a,b", 90), unit::S)`
- **THEN** nested commas and the quoted comma do not change the identified outer argument, and helper is never evaluated for completion

#### Scenario: Reuse existing parentheses
- **WHEN** a function name is completed before an existing opening parenthesis
- **THEN** completion preserves the existing delimiter instead of inserting another pair

#### Scenario: Cancel before execution
- **WHEN** the user cycles completion candidates and then presses Esc
- **THEN** the prompt closes and no candidate has executed

#### Scenario: Ordinary strings and comments
- **WHEN** the cursor is inside a comment or a string not annotated for a specific completion provider
- **THEN** code-symbol completion does not rewrite that text

### Requirement: Path completion through parameter metadata

A Rhai string argument annotated as a filesystem path, including the argument to `import_audio`, SHALL reuse directory/audio path discovery and `~/` expansion. Completion SHALL support incomplete double-quoted strings, spaces, Unicode, and correct Rhai escaping while preserving later arguments and delimiters. It SHALL NOT treat every string as a path. Both interactive frontends SHALL share path discovery and select escaping by explicit entry mode: ordinary commands SHALL use the shared command tokenizer's quoting rules, while Rhai source SHALL use Rhai string escaping. Filesystem access SHALL occur only for schema/API parameters annotated as paths.

#### Scenario: Ordinary import path
- **WHEN** the user completes an unfinished quoted path in `:import "~/Mu`
- **THEN** candidates use command-argument quoting, do not insert a function call, and do not execute an import

#### Scenario: Complete a Rhai import path
- **WHEN** the user presses Tab in the unfinished path string `import_audio("~/Mu`
- **THEN** matching directories/audio files are offered with appropriate string escaping, and choosing a candidate does not start an import

#### Scenario: Preserve the suffix
- **WHEN** a path is completed inside an existing `import_audio("prefix"); pause();` expression
- **THEN** the closing quote, closing parenthesis, and following statement remain intact

### Requirement: Readline-style command editing

Both interactive frontends SHALL map command-input keys to shared editor actions: Ctrl-A/E and Home/End for line boundaries, Ctrl-B/F and Left/Right for character movement, Alt-B/F for whitespace-delimited word movement, Backspace/Delete for adjacent character deletion, Ctrl-U/K for killing to line boundaries, Ctrl-W for killing whitespace immediately before the cursor followed by the preceding run of non-whitespace characters, and Ctrl-Y for yanking the latest nonempty killed text. Editing SHALL preserve UTF-8 character boundaries and invalidate completion. The yank slot SHALL be session-local; an empty kill SHALL NOT erase it. Enter/Tab/Shift-Tab/Esc and existing emergency/exit handling SHALL retain their roles. These shortcuts SHALL apply inside command input without changing normal-mode, search, or chooser controls. No particular readline library SHALL be required.

#### Scenario: Edit and yank a Unicode path
- **WHEN** the user kills and yanks part of a command containing a quoted Unicode path
- **THEN** the buffer and cursor remain valid UTF-8, the killed text is restored at the cursor, and no command executes

#### Scenario: Editor shortcuts respect context
- **WHEN** Ctrl-U is pressed inside command input and later in normal browsing
- **THEN** it kills to the beginning in command input and retains the existing normal-mode action outside it

#### Scenario: Editing changes entry mode
- **WHEN** an edit removes or restores the leading `=` marker
- **THEN** completion is invalidated and subsequent suggestions follow the resulting ordinary-command or script mode

### Requirement: Session-local command history

Both interactive frontends SHALL use the same history semantics with separate session-local stores bounded to 200 nonempty submissions. History SHALL preserve exact submitted editor text, including the `=` marker, exclude the TUI prompt colon, and retain failed submissions for correction. Adjacent identical submissions SHALL be deduplicated and oldest entries evicted at capacity. Cancelled/empty input, search terms, chooser answers, key callbacks, and dot replay SHALL NOT add entries. Up/Down and Ctrl-P/N inside command input SHALL recall older/newer entries without execution or wraparound, even while completion is visible. Tab/Shift-Tab SHALL retain candidate navigation.

On first movement from the newest position, the editor SHALL retain the draft and cursor. Recalled entries SHALL place the cursor at their end; moving past the newest entry SHALL restore the retained draft and cursor. Empty history SHALL leave input unchanged. Editing recalled text SHALL NOT mutate stored entries; subsequent history movement SHALL replace the working buffer, and returning to the draft SHALL restore the original draft. History replacement SHALL invalidate completion and select syntax mode from the restored text. Persistence across process exit is outside this change.

#### Scenario: Restore an unfinished draft
- **WHEN** the user has partially typed `=seek(50, `, moves Up through history, and moves Down past the newest item
- **THEN** the exact unfinished draft and its original cursor position return without executing any entry

#### Scenario: Correct failed source
- **WHEN** a command fails, the user recalls it, edits it, and presses Enter
- **THEN** the failed original remains unchanged in history and the edited text is submitted through the normal execution path

#### Scenario: Completion and history precedence
- **WHEN** a completion popup is open and the user presses Up
- **THEN** an older history entry is recalled, the popup session is invalidated, and no candidate executes

#### Scenario: Bounded history
- **WHEN** submissions exceed the history capacity or the same exact text is submitted consecutively
- **THEN** the oldest entries are evicted as needed and adjacent duplicates occupy only one entry

### Requirement: Repeat the latest successful command

A plain initial `.` keypress in normal TUI browsing SHALL repeat exactly one execution of the latest successfully completed command manually submitted from its command prompt, including ordinary commands, numeric navigation, and `=` scripts. It SHALL use the saved source and existing router/runtime with current execution context and fresh playback identity checks, rather than replaying cached service requests. The UI SHALL show the repeated source. Input/search/chooser contexts SHALL take precedence; dots there SHALL NOT trigger replay. The REPL SHALL continue to treat dots as input text and provide history recall rather than a normal-mode shortcut.

The repeat target SHALL update only after correlated successful completion of the full submitted workflow. Queue acceptance, failure, cancellation, empty input, stale results, key callbacks, gestures, and automatic practice wraps SHALL NOT replace it. Older successful submissions SHALL NOT overwrite newer successful ones. Replay SHALL NOT add history entries or replace its source with a repeat action. A replay failure SHALL report an error without automatic retry. No available target SHALL produce a notice without effects. Another manual submission or replay still pending SHALL cause dot requests to report busy rather than queue or replay an older target. Terminal autorepeat events and numeric counts SHALL NOT multiply dot execution in this version.

#### Scenario: Repeat relative seek
- **WHEN** `:=seek_by(10, unit::SEC)` completes successfully and the user later presses `.` in normal mode
- **THEN** the same source executes once from the current position with a new request identity, and history gains no replay entry

#### Scenario: Failed submission does not replace target
- **WHEN** a successful command is followed by a failed submission and the user then presses `.`
- **THEN** the last successful command is repeated, while the failed source remains available through history

#### Scenario: In-flight command and repeat
- **WHEN** a manual command or dot replay is still awaiting execution or completion and the user presses `.`
- **THEN** busy feedback is shown without queueing another replay or executing an older command

#### Scenario: Correlated completion order
- **WHEN** a delayed successful result for an older submission arrives after a newer successful submission
- **THEN** the repeat target remains the newer successfully submitted source

#### Scenario: Dot inside a script or path
- **WHEN** the user types a decimal point or filename dot in command input
- **THEN** the character is inserted normally and no repeat is triggered

#### Scenario: Repeat uses current context
- **WHEN** the saved source is repeated after variables, command registrations, selected rows, or the playback track change
- **THEN** it resolves that current context through the usual router/runtime and validation, without replaying effects against an old session

### Requirement: Shared progress display

The TUI SHALL show a bottom progress row in both library and lyrics views with elapsed time, a position bar, and total duration when available. The displayed playback position SHALL come from the reported playback timeline. The row SHALL adapt to terminal size without overlapping the status or command rows. Unknown duration SHALL display `--:--` and SHALL disable percentage-based pointer seeking. An unusably small bar SHALL not accept pointer gestures.

#### Scenario: View switching
- **WHEN** a user opens or closes the lyrics view during playback
- **THEN** the progress row remains available and reflects the same playback session

#### Scenario: Paused display
- **WHEN** playback is paused without a new seek
- **THEN** the bar and elapsed time remain at the reported paused position

#### Scenario: Narrow terminal
- **WHEN** the terminal cannot accommodate both time labels and a usable bar
- **THEN** the display prioritizes time information, avoids overlapping adjacent rows, and ignores pointer seeking on the missing or unusable bar

### Requirement: Pointer positioning and drag preview

In a normal browsing context with a loaded track and known positive duration, a left-button gesture beginning on the bar SHALL select a target proportional to horizontal position. Pressing SHALL begin preview without submitting a seek. Release SHALL submit exactly one absolute seek for either a click or a drag. An active drag SHALL clamp targets to the bar endpoints. Preview SHALL be distinguishable from actual elapsed time and SHALL not move playback or lyric highlighting before release.

#### Scenario: Click positioning
- **WHEN** the user presses and releases at the midpoint of the bar without dragging
- **THEN** one seek to approximately half the track duration is submitted, within terminal-cell precision

#### Scenario: Dragging
- **WHEN** the user presses the bar and moves the pointer before releasing
- **THEN** the target time and marker update while actual playback and lyric highlighting continue normally, and only release submits the seek

#### Scenario: Cancelled gesture
- **WHEN** the user presses Esc during preview
- **THEN** preview is cancelled, actual progress remains displayed, and subsequent button release submits no seek

#### Scenario: Gesture invalidated by context
- **WHEN** the terminal resizes, a track load starts, the playback identity changes, usable duration disappears, or an input prompt or chooser opens during a gesture
- **THEN** the gesture is cancelled and a later release cannot seek the current track

### Requirement: Seeking preserves playback intent and follows confirmed progress

Seek controls SHALL use the existing backend bounds and state transitions. Successful seeks inside a track SHALL preserve Playing or Paused state. Valid out-of-range time targets SHALL use backend clamping. A successful manual seek outside an active practice interval SHALL disable looping before normal playlist handling; reaching the confirmed end then retains existing service playlist behavior. A successful manual seek for the relevant playback session SHALL restore automatic lyric following using reported playback position, including after manual lyric scrolling. Preview, queue acceptance, and failed or stale requests SHALL not be treated as successful seeks.

#### Scenario: Paused seek and lyrics
- **WHEN** the user has manually scrolled lyrics and successfully seeks inside a paused track
- **THEN** playback remains paused, the bar shows the reported landing position, and lyrics return to the corresponding active line

#### Scenario: Seek to the beginning
- **WHEN** a relative backward seek exceeds the elapsed time or the user executes `seek(0, unit::SEC)`
- **THEN** playback lands at zero according to the existing backend contract

#### Scenario: End of track
- **WHEN** the user seeks to 100% or to a time beyond a known track duration
- **THEN** the backend clamps to the confirmed end and the existing playlist end behavior applies

#### Scenario: Asynchronous completion
- **WHEN** a seek has been queued but the service has not completed it
- **THEN** the TUI remains responsive and does not claim success or recenter lyrics on that basis alone

#### Scenario: Stale playback identity
- **WHEN** a queued seek or its completion refers to a playback session replaced by a newly loaded track
- **THEN** it cannot seek the replacement track or apply stale lyric-follow feedback to it

#### Scenario: Rejection and failure feedback
- **WHEN** seek validation, submission, or service execution fails
- **THEN** the TUI displays the error and does not invent a successful target position; backend decode/output failures retain the existing backend failure contract

### Requirement: A–B practice commands and bindings

Both interactive frontends SHALL expose the Rhai API `mark_a()` to mark the execution-time position as A, `mark_b()` to mark B and enable continuous looping, and `practice(start, end, unit)` to set both endpoints. Explicit endpoints SHALL accept finite integer or floating numeric values and one shared Unit value, using time units or full-track percentages. Current-position markers SHALL retain millisecond precision. Converted intervals SHALL satisfy `0 <= A < B <= duration` for a loaded track with known positive duration. Setting A SHALL clear B and disable the old loop without seeking. Completing or enabling a valid pair SHALL seek to A, continue if Playing, and otherwise remain non-playing until play is requested. Success SHALL be reported only after the seek succeeds and lands before B. Functions and their signatures SHALL be discoverable through command completion.

#### Scenario: Mark a passage while listening
- **WHEN** the user presses `[` at approximately 80 seconds and `]` at approximately 95 seconds
- **THEN** the service stores the execution-time positions as A and B, returns to A, and repeats the selected passage continuously

#### Scenario: Specify an exact passage
- **WHEN** the user executes `practice(80, 95, unit::SEC)` on a playing track longer than 95 seconds
- **THEN** the interval is set to 80 through 95 seconds and playback continues from A with looping enabled

#### Scenario: Prepare practice while paused
- **WHEN** a valid interval is enabled while playback is Paused, Ready, or Finished
- **THEN** the player positions at A without starting audio, and subsequent play starts interval playback

#### Scenario: Begin a new selection
- **WHEN** the user marks a new A while another interval exists
- **THEN** the old B is cleared, looping is disabled, and playback continues at its current position and state

#### Scenario: Invalid interval
- **WHEN** B is marked without A, A is not before B, an endpoint is outside the track, syntax is unsupported, no track is loaded, duration is unknown, a replacement is pending, or the request is stale
- **THEN** the command reports an English error and preserves the prior healthy playback and interval state

#### Scenario: Brackets inside a prompt
- **WHEN** the user types `[` or `]` in a command or search prompt
- **THEN** the prompt accepts the character without changing practice markers

### Requirement: Continuous service-owned interval playback

The playback service SHALL own practice state for the current playback identity and publish it to attached clients. While enabled and playback is advancing, observing position at or beyond B SHALL cause a return to A and continued playback, repeatedly without a count limit or intentional delay. Boundary checks SHALL remain scheduled while commands arrive and SHALL not depend on a TUI rendering or polling. Automatic interval return SHALL take precedence over natural playlist advancement, including when B equals track duration, without changing the selected playlist repeat mode. Automatic looping SHALL NOT resume a completed user pause or stop. The behavior SHALL be documented as seek-based looping rather than sample-exact or gapless playback.

#### Scenario: Repeated practice
- **WHEN** playback reaches B on successive passes with the interval enabled
- **THEN** each pass returns to A on the same track and continues, and the selected playlist mode remains unchanged

#### Scenario: Practice to the track end
- **WHEN** B equals track duration and playing audio reaches the confirmed end
- **THEN** the service returns to A and resumes that track instead of advancing the playlist or remaining paused after the backward seek

#### Scenario: Background practice
- **WHEN** the user detaches the TUI while interval playback is active
- **THEN** the service keeps looping and a later attachment displays the same interval and state

#### Scenario: Incoming command traffic
- **WHEN** playback reaches B while status or control requests keep arriving
- **THEN** boundary handling still runs rather than waiting for an idle receive timeout

#### Scenario: Wrap failure
- **WHEN** an automatic return to A fails or cannot land before B
- **THEN** looping is disabled and an error is reported without an automatic retry loop or automatic playlist advancement that conceals the failure

### Requirement: Practice status and lifecycle

`practice_status()` SHALL report current markers and enabled state without changing playback. `practice_off()` SHALL disable looping while retaining markers and position; `practice_on()` SHALL enable a retained valid pair and return to A using the same playback-intent rules as initial activation. `practice_clear()` SHALL disable looping and remove the markers without seeking. Pause/resume SHALL retain the interval. Stop, service shutdown/restart, and successful track replacement SHALL clear it. Failed track replacement that preserves the healthy source SHALL preserve its interval. Practice state SHALL be session-local and SHALL NOT be saved in the music library.

#### Scenario: Disable and reuse a passage
- **WHEN** the user executes `practice_off()` and later `practice_on()` on the same loaded track
- **THEN** disabling retains the markers and current position, and enabling returns to A and uses the retained pair

#### Scenario: Missing retained interval
- **WHEN** `practice_on()` is executed without a complete valid pair
- **THEN** an English error is shown and playback remains unchanged

#### Scenario: Pause at a boundary
- **WHEN** the service completes a pause while an interval is enabled, including near B
- **THEN** the interval remains stored and no boundary action restarts audio until the user requests play

#### Scenario: Explicit next or previous
- **WHEN** an explicit next, previous, or load command successfully replaces the current track
- **THEN** the prior interval is cleared and the new track follows normal playback behavior

#### Scenario: Clear markers
- **WHEN** the user executes `practice_clear()`
- **THEN** markers are removed without changing playback position or state

#### Scenario: Stop practice
- **WHEN** the user stops playback, including through the existing quit behavior
- **THEN** the interval is cleared and normal stop semantics apply

### Requirement: Practice interaction with seeking and lyrics

A successful manual seek landing inside an enabled interval `[A, B)` SHALL retain looping. A successful manual seek landing outside it SHALL disable looping and retain markers. This decision SHALL occur in the shared service before subsequent boundary handling and SHALL apply to all clients. A preview or cancelled gesture SHALL NOT change interval state. The progress display SHALL mark A/B and the interval on the full-track scale, distinguish active from retained/off state, and expose readable range status when space is limited. Lyrics SHALL follow the reported playback position on automatic return; an automatic wrap SHALL preserve an existing manual-scroll grace period, while explicit interval activation SHALL restore automatic following.

#### Scenario: Seek within the passage
- **WHEN** a user seeks to 85 seconds inside an active interval from 80 to 95 seconds
- **THEN** playback moves there and continues looping that interval

#### Scenario: Seek out of the passage
- **WHEN** a key, typed command, or completed bar gesture successfully seeks to a position outside the active interval, including exactly B
- **THEN** playback goes to the requested position using ordinary seek semantics and looping becomes inactive with markers retained

#### Scenario: Cancel an outside preview
- **WHEN** the user previews a position outside the interval and cancels before release
- **THEN** the current interval remains enabled and playback does not seek

#### Scenario: Automatic lyric return
- **WHEN** an active interval returns to A while lyrics are following
- **THEN** the highlighted line and viewport follow the actual returned position

#### Scenario: Manual lyric browsing during a wrap
- **WHEN** an active interval returns to A during the manual-scroll grace period
- **THEN** highlighting updates but automatic centering waits for the usual resumption

#### Scenario: Shared interval display
- **WHEN** another attached client changes the current track's practice state
- **THEN** subsequent snapshots update the markers and active/off indication in both library and lyrics views
