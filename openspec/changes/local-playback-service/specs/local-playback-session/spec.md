## ADDED Requirements

### Requirement: Playback outlives the terminal client

A local service SHALL own playback independently of CLI and TUI lifetimes. Reattaching SHALL expose the existing current track, pending operation, state, position, duration, and errors without reloading audio.

#### Scenario: Reopen after detach
- **WHEN** a user reopens `muse` for a library whose detached session is playing
- **THEN** the TUI displays the service's current state without restarting the song or creating a second player

#### Scenario: Unannounced client loss
- **WHEN** a client process disappears without delivering a stop or shutdown request
- **THEN** the service preserves playback and remains accessible to a new client

### Requirement: Isolated service identity and startup

There SHALL be at most one service owner per user and canonical library path. Local control SHALL be restricted to authenticated access using user-private runtime credentials. Startup and connection attempts SHALL have bounded waits and reject incompatible protocol versions without replacing a live service.

#### Scenario: Concurrent starts through equivalent paths
- **WHEN** two clients start simultaneously using relative and symlink paths that resolve to the same library
- **THEN** both connect to one service and one playback session

#### Scenario: Different libraries
- **WHEN** a client stops the session for one library
- **THEN** a session belonging to a different canonical library path is unaffected

#### Scenario: Stale runtime record
- **WHEN** startup encounters a record whose owner is no longer running
- **THEN** it reclaims the record only after obtaining exclusive ownership and starts a usable session

#### Scenario: Incompatible or unauthenticated connection
- **WHEN** a connection has an incompatible protocol or invalid credential
- **THEN** no playback command from that connection is executed and the existing session is preserved

### Requirement: Authoritative shared playback controls

The service SHALL serialize accepted commands, retain track-generation protection for track-specific controls, and expose completion or failure to clients. A client SHALL NOT automatically replay a mutation whose outcome is unknown after disconnection. Device errors SHALL remain visible without disabling library browsing or imports.

#### Scenario: Pause observed from another client
- **WHEN** one client successfully pauses playback
- **THEN** another connected client observes the same paused track and position

#### Scenario: Stale seek
- **WHEN** a seek targeting an earlier track generation arrives after the current track changes
- **THEN** the service rejects the seek without moving the new track

#### Scenario: Lost response
- **WHEN** a client loses its connection after submitting a playback mutation but before receiving completion
- **THEN** it reports an unconfirmed outcome and does not automatically repeat that mutation

### Requirement: Explicit service shutdown

The service SHALL remain available after stop, detach, and natural track completion until explicitly shut down or terminated by the operating system. Successful shutdown SHALL stop output and release service ownership; loss of the service SHALL be shown to connected clients without automatic playback restart.

#### Scenario: Shutdown and subsequent startup
- **WHEN** shutdown completes and a later TUI starts for the same library
- **THEN** the previous service no longer owns the session and the new session starts without automatically playing the previous song
