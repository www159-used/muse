## Context

Imports already run on a worker with a final result channel. The library processes files while walking, so it has no accurate total. Long static status messages hide useful state. Progress must count file outcomes after the existing single-song transaction and must not confuse job stages with NORMAL/COMMAND modes.

## Goals / Non-Goals

**Goals:** Live scan discovery, accurate file progress, current path and outcome counters, visible cancellation/failure, bounded worker-to-UI updates.

**Non-Goals:** Byte-level or per-file decode progress, estimated completion time, persistent job history, changing deduplication or updating the song list after each file.

## Decisions

### Chosen: Additive callback and explicit phases

Add ImportPhase and ImportProgress plus Library::import_with_progress(path, cancel, callback). Existing methods delegate with a no-op callback. Snapshots contain discovered audio count, optional total, processed count, outcome/issue counters and current path. Scanning keeps total unknown; processing and attachment finalization use the discovered total. Final phases distinguish Completed, Cancelled and Stopped. A missing source retains an unknown total and reports its existing failure rather than inventing successful work.

Callbacks run on the caller's thread, before file processing and after its existing transaction outcome. They do not receive audio bytes or alter the source snapshot/deduplication logic. The callback must return promptly.

### Chosen: Discover paths once before importing

Collect supported audio paths and sidecar candidates using the existing ordered, symlink-safe walk, then process that list. This provides an accurate denominator with one directory walk. Cancellation can stop scanning without processing any songs. A changed or removed discovered file is still attempted and reported normally. Cancellation/storage failure during processing retains completed results and a processed count below total. Unmatched attachment finalization remains separate from audio failures.

### Chosen: Share only the latest snapshot

The worker replaces one mutex-protected snapshot; the UI reads it once per draw. There is no progress message backlog. The existing result channel and worker shutdown rules remain authoritative for completion. Before library opening completes, show Preparing import. Never hold the progress lock during filesystem reads or database transactions.

### Chosen: Visible panel and compact status

Use the existing import-results area for a live Import progress panel: stage, known progress bar, added/failed counters, current filename and source. Label the duplicate count Skipped duplicates to make clear that existing audio creates no extra tracks. While scanning, show discovered count without a percentage. Put the compact import count before playback status so a long path/title cannot obscure it. Completion restores the final results view. Small windows may clip secondary path details; counts have priority. Empty directories avoid division by zero and do not invent audio work.

## Risks / Trade-offs

- Path discovery increases time before the first song is imported; show scanning checkpoints and allow cancellation throughout.
- Discovered path storage grows with file count; store paths only, never audio snapshots for the whole folder.
- One very large file can keep file-level progress unchanged while it is read/decoded; show its current filename and do not fabricate byte progress.
- New files created after discovery belong to a later import; report failures for discovered files that disappear.

## Migration Plan

Add the callback types and discovery stage, connect the worker snapshot, render progress and verify existing reports/transactions. No database or dependency migration is needed. Existing library callers keep their signatures.

## Open Questions

None required. Byte-level progress and ETA remain separate future work.

## Verification

- make ci passed on macOS: formatting, standard-library playback interface check, Clippy with warnings denied, 56 ordinary tests, two doctests and the release build. The three physical-device/personal-sample checks remain opt-in.
- Library regressions observe committed rows from a separate SQLite connection, mixed additions/duplicates/failures, repeated imports without extra tracks, Unicode paths, separate attachment issues, cancellation during scanning and after a committed file, empty/single/invalid sources and storage failure with remaining files unprocessed.
- UI regressions cover known and unknown progress, long source/status text, NORMAL/COMMAND transitions, tiny terminals, empty totals, restoration of final reports and worker snapshots matching a real 200-file import.
- Real terminal smoke checks used the release binary and a temporary folder of 10,000 identical MP3 fixtures. Live counts, skipped duplicates, current filenames and the Unicode source path were visible. Command entry and cancellation returned the correct badges while importing. Completion restored the report with one addition and 9,999 skipped duplicates; a separate :q run cancelled the active worker and exited. Both runs restored the terminal, passed SQLite integrity_check and retained exactly one audio BLOB identical to the fixture. Source files were unchanged.
- All seven OpenSpec changes pass strict validation. Linux and Windows execution was not performed in this session.
