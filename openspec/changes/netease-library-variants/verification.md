# Implementation verification

Implemented locally on macOS (Apple Silicon), 2026-10-12. Tests use temporary libraries and synthetic audio; no personal library was migrated or modified.

## Checks

- `make ci`: formatting, playback interface without native dependencies, workspace Clippy with warnings denied, all-target/all-feature tests, doctests and release build.
- `openspec validate netease-library-variants --strict`: proposal/design/spec/task consistency.
- `git diff --check`: whitespace validation.

The test suite includes 129 passing ordinary Rust test executions (some production-module tests are also included by process harnesses), two doctests, and 14 Cucumber scenarios with 128 passing steps. Ignored cases are physical-device/personal-source checks and child-process entry points invoked separately by their parent integration tests.

## Evidence by workflow

- `muse-library` storage tests cover persistent name/type, unknown types, platform identity conflicts sharing one audio blob, ID-less input in both import orders, representative grouping and invalidated anchors.
- Download tests retain alternate encodings and Unicode/case collisions, serialize concurrent imports, preserve source bytes, and isolate candidate artwork. Attachment observation tests retain conflicting lyrics without replacing the primary attachment.
- Upgrade tests preserve ID 42, audio and legacy attachment attribution; verify a WAL-inclusive version-1 backup and old-copy restore; recover suppressed NCM/MP3/FLAC idempotently; leave failed source verification retryable; reject concurrent ownership; and verify both failed-backup preservation and rollback after a migration has rebuilt the tracks table. Successful backups remain available after migration failure.
- Backup tests cover pinned committed snapshots, concurrent writes/deletions, no-overwrite destinations and sidecars, corruption and foreign-key validation.
- CLI process tests cover missing read-only queries, JSON group/candidate/source output, import without a service, active-service upgrade rejection, ambiguous group play without playback mutation, and exact candidate playback.
- Real PTY tests cover command history/completion, quoted Unicode import, group selection/cancellation, detach, EOF, Ctrl-C, terminal hangup and active-import cleanup. Another PTY workflow imports through CLI while TUI input is active, observes the new candidate, verifies unchanged playback while choosing/cancelling, and loads only the confirmed ID through the deterministic test backend.
- Renderer and background-reader tests cover preserved command input, alternate metadata search, tiny terminals, removed selection/anchor, stale chooser reconfirmation, read-error recovery and change-only snapshots. Existing navigation, search and playback regression tests remain enabled.

The process tests exposed inherited nonblocking accepted sockets on macOS. Service handlers now explicitly select blocking I/O with frame deadlines; the listener remains nonblocking. Command-mode output is rendered without toggling raw input, avoiding lost input during result printing.

## Execution limits

Linux and Windows native builds/runtime tests were not executed in this session. The repository has a three-host CI configuration, but no Git remote is configured, so no hosted run was triggered. Real sound-device output and personal NCM samples remain opt-in and were not exercised. Deterministic playback process tests validate command/state transitions, not audible output.

API integration, other source extensions, dynamic plugins, preferred candidates and cross-library aggregation remain deferred. Version-1 content never retained in the database still requires original files for recovery. New clients honor maintenance locks; old binaries must be closed before upgrade.
