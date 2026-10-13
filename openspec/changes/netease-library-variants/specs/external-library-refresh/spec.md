## ADDED Requirements

### Requirement: Observe committed changes from other clients

An open TUI SHALL detect library changes committed by another application connection and refresh its group/candidate metadata automatically. It SHALL check approximately every 500 ms while active and perform potentially blocking reads off the UI event thread. It SHALL NOT require the importing CLI to start a playback service or broadcast a job.

#### Scenario: CLI imports while TUI is open
- **WHEN** CLI import commits a new candidate to the same library
- **THEN** the TUI incorporates it on a subsequent successful refresh without being restarted and without displaying uncommitted partial content

#### Scenario: Playback-independent library query
- **WHEN** another client only lists the library
- **THEN** that query does not cause playback operations or unnecessary replacement of the TUI's cached metadata

### Requirement: Refresh preserves interaction and reports failures

Refresh SHALL preserve surviving selection identities, scroll/focus, command input, and search text; it SHALL update search matches and clamp views when rows change. Candidate chooser changes SHALL require fresh confirmation. Refresh SHALL NOT initiate playback. On read failure, the TUI SHALL retain its last valid snapshot, show the failure, and retry with bounded waits.

#### Scenario: Import during command editing
- **WHEN** another client commits songs while the user edits a TUI command or search
- **THEN** the typed text remains intact, metadata and matches update, and current playback is unchanged

#### Scenario: Selected item disappears
- **WHEN** a supported library operation removes the selected candidate/group
- **THEN** normal browsing selects a valid nearby row or empty state, while a chooser reports the changed selection without playing a different candidate

#### Scenario: Temporary read failure
- **WHEN** a metadata refresh cannot complete
- **THEN** input and rendering remain responsive and the last valid list remains available until a later successful refresh
