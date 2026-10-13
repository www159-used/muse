## ADDED Requirements

### Requirement: Noninteractive shared session commands

The CLI SHALL support `play [<track-id>]`, `pause`, `stop`, `status`, and `shutdown` with `--library`, without requiring a TTY. IDs SHALL refer to database track IDs rather than TUI row numbers. Bare `play` SHALL resume/start the current loaded track; `play <track-id>` SHALL load and play that track. Pause and stop SHALL be idempotent. Output and diagnostics SHALL be English, with failures returning a nonzero exit status. Mutating commands SHALL report success only after completion is confirmed.

#### Scenario: CLI controls a detached session
- **WHEN** `muse --library <file> pause` completes for a playing detached session
- **THEN** a subsequent status command reports that same track paused

#### Scenario: Start a track from CLI
- **WHEN** `play <track-id>` names an existing song in an existing library without a running service
- **THEN** the CLI starts the service, waits for the load-and-play result, and exits while successful playback continues

#### Scenario: Resume without a loaded track
- **WHEN** bare `play` is invoked without a loaded track
- **THEN** it returns a clear error without selecting an arbitrary song

### Requirement: Observation and maintenance avoid startup side effects

With no service, `status` SHALL successfully report its absence, while `stop` and `shutdown` SHALL succeed as no-ops. These commands SHALL NOT launch a service or initialize a library. Pause and bare play SHALL fail when no session exists. Invalid arguments SHALL fail before side effects. Existing help, version, backup, and restore behavior SHALL remain independent of service availability.

#### Scenario: Status for a missing library
- **WHEN** status is requested for a nonexistent library path
- **THEN** it reports no running session without creating a database or starting a service

#### Scenario: Backup without a service
- **WHEN** an existing library is backed up using the current backup CLI
- **THEN** backup completes using the existing validation and no-overwrite rules without starting a playback service

#### Scenario: Invalid command
- **WHEN** a command contains unsupported arguments
- **THEN** it exits unsuccessfully with usage guidance and does not create a library or start a service
