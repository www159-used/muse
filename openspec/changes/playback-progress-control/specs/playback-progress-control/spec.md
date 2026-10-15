## ADDED Requirements

### Requirement: Rhai command entry and application APIs

The TUI colon prompt SHALL accept standard Rhai function calls, expressions, and script functions without an additional language prefix. Registered application APIs SHALL invoke the existing Rust operation layer. Script execution SHALL leave rendering and input responsive, report syntax/runtime errors in English, and preserve execution order for sequential host calls. A failed call SHALL abort remaining statements unless handled by the script; earlier successful effects SHALL NOT be reported as rolled back. Existing shipped colon commands and numeric navigation SHALL remain available through explicit compatibility recognition. Rhai errors SHALL NOT trigger a second execution attempt through legacy parsing. The standalone CLI/REPL SHALL retain their existing grammar and SHALL NOT advertise unsupported Rhai-only completions.

#### Scenario: Direct function execution
- **WHEN** the user enters `:seek(90, unit::SEC)` and presses Enter
- **THEN** the registered seek operation executes without requiring a language prefix

#### Scenario: Ordered practice startup
- **WHEN** the user submits `practice(80, 95, unit::SEC); play();`
- **THEN** play executes after successful practice establishment, and an unhandled establishment error prevents play

#### Scenario: Legacy import remains available
- **WHEN** the user submits an existing `:import` command with a quoted path
- **THEN** the existing import behavior is preserved through its explicit compatibility adapter
- **AND** `:import_audio("path")` uses the registered Rhai API for the same underlying operation

#### Scenario: Failure does not retry input
- **WHEN** Rhai evaluation fails after a completed host operation
- **THEN** the error is displayed, the completed effect is not repeated, and the text is not reinterpreted as a legacy command

### Requirement: Numeric seek arguments and unit values

The TUI SHALL expose `seek(value, unit)` for absolute positioning and `seek_by(value, unit)` for relative movement. It SHALL export host-defined unit values `unit::MS`, `unit::SEC`, `unit::MIN`, `unit::HOUR`, and `unit::PERCENT`. Unit arguments SHALL be registered Unit values, not strings. Numeric arguments SHALL accept integers and finite floating values with checked conversion and round-to-nearest millisecond normalization. Absolute values SHALL be nonnegative; percentages SHALL be within 0 through 100 and require known positive duration. Relative movement SHALL accept signed time values, reject PERCENT in this version, and use execution-time playback position. Percentage resolution SHALL use the matching playback session in the service. Unsupported types, non-finite numbers, overflow, and invalid units SHALL fail before mutation. New APIs SHALL NOT infer units or parse time/percentage strings.

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

The Rhai completion catalog SHALL derive callable names, signatures, and namespaces from application registrations and available script metadata. Exported unit symbols and semantic parameter roles SHALL be declared alongside the same APIs. Numeric overloads SHALL be grouped for presentation without hiding valid callable forms. Completion SHALL use the current cursor context to suggest functions, namespaces, qualified unit values, and parameter-specific candidates; recognized calls SHALL display signature help indicating the active argument. Unit candidates for `seek_by` SHALL exclude PERCENT. Numeric parameters SHALL receive signature help rather than fabricated numeric suggestions. Available registered script functions SHALL become discoverable without invoking them. Completion SHALL NOT execute scripts, callbacks, or host getters, and non-path contexts SHALL NOT scan the filesystem.

#### Scenario: Function discovery
- **WHEN** the user presses Tab after `:se`
- **THEN** `seek` and `seek_by` appear with signatures from the API catalog and nothing executes

#### Scenario: Namespace completion
- **WHEN** the user presses Tab after `:unit::`
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

The editor SHALL provide conservative completion for incomplete Rhai input, including unclosed calls and strings. Context recognition SHALL account for nested delimiters, quoted/escaped text, comments, and commas at the active call depth. Candidate application SHALL replace only the relevant UTF-8-safe range, preserve surrounding text, and set the cursor appropriately. Unique function completion SHALL add missing parentheses and position the cursor inside without duplicating existing delimiters. First Tab on ambiguous input SHALL expand the common prefix and show candidates; subsequent Tab/Shift-Tab SHALL cycle with wraparound. Edits or cursor movement SHALL invalidate the completion session. Enter SHALL explicitly submit the resulting input and Esc SHALL cancel the prompt. Unsupported/ambiguous contexts SHALL leave input intact or offer conservative name suggestions without evaluation. Small terminals SHALL degrade hints/popups without corrupting input.

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

A Rhai string argument annotated as a filesystem path, including the argument to `import_audio`, SHALL reuse directory/audio path discovery and `~/` expansion. Completion SHALL support incomplete double-quoted strings, spaces, Unicode, and correct Rhai escaping while preserving later arguments and delimiters. It SHALL NOT treat every string as a path. Legacy command completion SHALL retain its existing quoting rules in compatibility mode; standalone REPL completion SHALL remain in its own grammar mode.

#### Scenario: Complete a Rhai import path
- **WHEN** the user presses Tab in the unfinished path string `import_audio("~/Mu`
- **THEN** matching directories/audio files are offered with appropriate string escaping, and choosing a candidate does not start an import

#### Scenario: Preserve the suffix
- **WHEN** a path is completed inside an existing `import_audio("prefix"); pause();` expression
- **THEN** the closing quote, closing parenthesis, and following statement remain intact

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

The TUI SHALL expose `mark_a()` to mark the execution-time position as A, `mark_b()` to mark B and enable continuous looping, and `practice(start, end, unit)` to set both endpoints. Explicit endpoints SHALL accept finite integer or floating numeric values and one shared Unit value, using time units or full-track percentages. Current-position markers SHALL retain millisecond precision. Converted intervals SHALL satisfy `0 <= A < B <= duration` for a loaded track with known positive duration. Setting A SHALL clear B and disable the old loop without seeking. Completing or enabling a valid pair SHALL seek to A, continue if Playing, and otherwise remain non-playing until play is requested. Success SHALL be reported only after the seek succeeds and lands before B. Functions and their signatures SHALL be discoverable through command completion.

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
