## ADDED Requirements

### Requirement: Observable import stages

The library SHALL optionally publish progress for scanning, importing, attachment finalization and terminal outcomes without changing existing import entry points. Scanning SHALL report discovered supported audio files with an unknown total. Completed discovery SHALL establish a stable total for processing. Current paths SHALL preserve their original Unicode content.

#### Scenario: Discover and import a folder
- **WHEN** a recursive folder import scans and then processes supported audio
- **THEN** discovery advances without a fabricated percentage and processing reports current path, processed count and known total

### Requirement: Transaction-based counters and truthful termination

Processed SHALL equal added plus duplicates plus audio failures. Counters SHALL advance only after a file's transaction commits, rolls back or fails. Attachment/scan issues SHALL remain separate. Cancellation or storage failure SHALL not count remaining files as processed and SHALL have a terminal phase distinct from Completed. Final snapshots SHALL agree with the final report.

#### Scenario: Cancel after one file
- **WHEN** import is cancelled after one of several files finishes
- **THEN** the final phase is Cancelled with that file counted, remaining files unprocessed and prior commits preserved

#### Scenario: Storage failure
- **WHEN** a file transaction fails with a library error
- **THEN** progress counts that failed attempt and prior outcomes, reports Stopped and leaves remaining files unprocessed

### Requirement: Visible terminal progress

An active import SHALL display its stage, processed/total when known, outcome counters and current file in the existing lower panel, plus a compact status count. Duplicate outcomes SHALL be labeled as skipped duplicates, since existing audio does not create additional tracks. Long source paths SHALL not precede and obscure progress counts. Progress updates SHALL use bounded shared state and SHALL preserve browsing, colon commands, playback and NORMAL/COMMAND labels. Finished imports SHALL restore the final report view.

#### Scenario: Inspect a running import
- **WHEN** import is active while the user browses or enters a command
- **THEN** live progress remains visible and the mode badge describes keyboard interaction rather than the import stage

#### Scenario: Empty folder
- **WHEN** discovery finds no supported audio files
- **THEN** the UI reports no audio files without dividing by zero and then displays the normal final report
