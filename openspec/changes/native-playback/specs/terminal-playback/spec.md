## ADDED Requirements

### Requirement: Background playback controls

The terminal SHALL read library audio and control playback in the background. Enter SHALL play the selection, Space SHALL pause/resume, Left/Right SHALL seek backward/forward ten seconds, and :stop SHALL stop at zero. The interface SHALL update current identity after successful loading and show actual state/position. Missing audio devices or playback errors SHALL not prevent browsing and import.

#### Scenario: Play the selection
- **WHEN** the user selects a track and presses Enter
- **THEN** background work opens its library audio, confirms the loaded identity, reports the actual playback outcome and leaves input processing available

### Requirement: Completion and shutdown

Natural completion SHALL retain the current track and stop without advancing. Seek requests from an old track generation SHALL not affect a new track. Quit SHALL stop/release playback before completing the existing import shutdown rules.

#### Scenario: Obsolete seek after a track change
- **WHEN** a prior track's seek request executes after a track change
- **THEN** it is rejected or discarded and the new track position is unchanged

#### Scenario: Quit during playback
- **WHEN** the user quits while playing
- **THEN** playback resources are released, the current import commits or rolls back according to existing rules, and the terminal is restored
