## ADDED Requirements

### Requirement: Built-in owned audio playback

The native feature SHALL expose NativePlayback and standard error types implementing the existing Playback interface. Loading a complete local MP3/FLAC SHALL enter Ready at zero without starting playback. Successful play SHALL output decoded audio to the default device. The source SHALL remain independent of library connections, and public inputs SHALL require only Send.

#### Scenario: Independent library playback
- **WHEN** an owned MP3/FLAC reader has been obtained and its database connection closed
- **THEN** the backend can load it and consume real decoded PCM without the original audio file

### Requirement: Bounded prefetch and basic controls

The backend SHALL bound prefetch; Paused and Ready SHALL not keep decoding the entire track. Pause SHALL retain music position, stop SHALL retain the source at zero, and play after Finished SHALL replay from the beginning. Starvation silence SHALL not advance music position. Confirmed EOF with a drained music queue SHALL report Finished.

An available complete PCM frame SHALL remain readable while another thread inspects playback status or publishes decoded audio. The output callback SHALL NOT take the status mutex or substitute silence solely because another thread holds that mutex.

#### Scenario: Pause, stop and replay
- **WHEN** playback is paused, stopped or restarted after Finished
- **THEN** pause retains position, stop returns Ready at zero, and replay begins at the start

#### Scenario: Status inspection during multichannel playback
- **WHEN** a complete six-channel PCM frame is queued and another thread holds the status mutex
- **THEN** playback outputs every queued sample in channel order, advances position once per complete frame and reports Finished only after confirmed EOF and queue drainage

#### Scenario: Pause races with a prepared output frame
- **WHEN** pause completes while the output callback is preparing a frame
- **THEN** the submitted music position remains fixed until playback resumes and any frame not authorized before pause remains queued

### Requirement: Time seeking and session isolation

The backend SHALL implement absolute and execution-time relative seeking, return the actual landing point and follow the existing range/state contract. Successful seeking SHALL clear obsolete PCM and completion results. An old source SHALL not affect a new session.

#### Scenario: Seek while paused
- **WHEN** a paused track is sought forward or backward
- **THEN** the returned position changes, the state remains Paused, and targets clamp to a reliable known duration

### Requirement: Failed replacements and persistent faults

An invalid candidate load SHALL return an error and preserve a healthy old source and playback intent. Runtime input, decoder, output-device and worker faults SHALL stop further music submission and remain observable through status rather than report Finished. A live backend SHALL clear a recoverable fault only after successful load or stop rebuilding. A terminated worker SHALL require a new backend.

#### Scenario: Invalid replacement
- **WHEN** invalid audio is loaded while a healthy source exists
- **THEN** loading fails, the candidate is released, and the original source and playback intent remain

#### Scenario: Runtime failure and recovery
- **WHEN** an input or decoder failure is followed by repeated status queries and then a successful stop rebuild or load
- **THEN** queries continue to report the failure until successful recovery returns Ready

### Requirement: Cleanup and builds

Dropping the backend SHALL stop further music submission, end its worker and release its source/device. The standard-library interface build SHALL not require audio dependencies. Native CI SHALL provide Linux audio development libraries.

#### Scenario: Drop during playback
- **WHEN** the backend is dropped while playing or paused
- **THEN** output is invalidated, its worker ends, and the source is released without detached playback
