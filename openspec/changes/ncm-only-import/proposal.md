## Why

The download folder contains ordinary MP3 files alongside NCM versions of the same songs. Their ID3 tags differ, so the existing complete-file hashes retain both versions. The user chose to import only NCM sources.

## What Changes

- **BREAKING**: The application's existing :import command accepts NCM files and recursively selects only NCM files from folders; ordinary MP3/FLAC sources are excluded.
- Limit path completion to directories and NCM files, including uppercase extensions.
- Count only selected NCM files in folder import progress while retaining existing transactions, cancellation, metadata, attachments and unpacked MP3/FLAC storage.
- Update English help, empty-library guidance and usage documentation.
- Compose decomposed Hangul in displayed progress paths so Korean filenames appear as complete syllables while file access uses the original path.

## Capabilities

### New Capabilities

- `ncm-import`: NCM-only application imports, matching completion candidates and selected-source progress.

### Modified Capabilities

None in the main specification directory. This change supersedes the application's earlier multi-format source selection recorded in muse-mvp and command-completion; general-purpose library entry points remain available.

## Impact

crates/muse-library gains an additive NCM-only progress entry point over its existing import pipeline. crates/muse uses it for its worker and adjusts completion/help. No dependencies or database schema changes are needed. Existing stored songs remain readable; this change does not automatically merge earlier imports.
