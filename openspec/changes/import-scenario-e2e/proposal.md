## Why

Import spans source files, decoding, attachments, cancellation and persistent storage. Rust test bodies make the user scenarios hard to scan and extend when download layouts or metadata change.

## What Changes

- Add an independent `muse-e2e` workspace member under `crates`, with test-only dependencies on the public library interface.
- Describe import workflows in readable Given/When/Then TOML cases and register each file as a separate Cargo test.
- Run cases against isolated real files and SQLite libraries, checking reports, progress, stored audio/metadata/attachments and source preservation.
- Cover folder layouts, NCM selection, duplicate and repeat imports, persistence, attachments, cancellation, damaged inputs and metadata evolution.
- Migrate overlapping high-level import tests into scenarios while retaining focused storage/failure tests.
- Provide focused Make targets, listing/filtering, contributor guidance and workspace CI coverage.

## Capabilities

### New Capabilities

- `import-scenario-e2e`: Declarative, isolated import workflow regression tests maintained independently of application implementation.

### Modified Capabilities

None. Import behavior is unchanged.

## Impact

The workspace gains a test-only package, fixtures and scenario runner. Selected library integration tests move to that package. Runtime packages gain no dependency on the E2E package and expose no test-only interfaces. Make targets, contributor documentation, Cargo.lock and OpenSpec context are updated.
