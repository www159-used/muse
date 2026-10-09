## Why

During a folder import, the terminal currently shows a static source path that can be clipped. Users cannot tell whether work is advancing. Show observable import progress while preserving browsing, command entry and transactional import behavior.

## What Changes

- Add an optional library progress callback with scan, import, attachment finalization and terminal phases.
- Discover audio candidates first so the file total becomes accurate before processing; retain unknown totals during scanning.
- Display a compact count in the status line and a progress panel with a bar, current file, source and outcome counters.
- Publish counters only after a file has committed, rolled back or failed; distinguish cancellation and storage failure from completion.
- Preserve existing import methods, final reports, Unicode paths and NORMAL/COMMAND modes.

## Capabilities

### New Capabilities

- `import-progress`: Observable import stages, truthful file counts, terminal progress rendering and bounded background updates.

### Modified Capabilities

None. Existing import specifications remain in unarchived changes.

## Impact

Affects muse-library import discovery and an additive callback API, muse's import worker and rendering, tests and documentation. No dependencies or schema changes are required. Existing import and import_with_cancel callers remain supported.
