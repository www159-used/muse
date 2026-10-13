## ADDED Requirements

### Requirement: Explicit terminal detach

The TUI SHALL support `:detach`, include it in builtin command completion, and exit with terminal modes restored without changing the shared playback state.

#### Scenario: Detach during playback
- **WHEN** the user executes `:detach` while a song is playing
- **THEN** the TUI process exits and restores raw mode, mouse reporting, and the alternate screen while playback continues in the service

#### Scenario: Detach while paused
- **WHEN** the user detaches from a paused or stopped session
- **THEN** detachment does not resume playback or discard the current track

#### Scenario: Completion does not execute detach
- **WHEN** the user completes the detach command using Tab
- **THEN** the prompt offers or inserts `detach` without exiting until the command is submitted

### Requirement: Ordinary quit still stops playback

The TUI SHALL stop the shared playback session on `:q`, `:quit`, handled Ctrl-C, or an observed terminal hangup. It SHALL use bounded waits and report failed or unconfirmed stops without claiming success, and SHALL restore terminal state on exit.

#### Scenario: Quit with another client attached
- **WHEN** one TUI successfully executes `:q` while another client observes the same library session
- **THEN** the first TUI exits and the other client observes stopped playback

#### Scenario: Quit while loading
- **WHEN** an ordinary quit follows an accepted load request
- **THEN** successful quit acknowledgement guarantees that the earlier load will not subsequently start playback

### Requirement: Import cleanup on either exit path

Both detach and ordinary quit SHALL cancel active import work between songs and wait for the current song transaction to commit or roll back before exiting.

#### Scenario: Detach during import
- **WHEN** the user detaches while an import is processing a song
- **THEN** the current song transaction finishes safely, further songs are not started, and playback is preserved
