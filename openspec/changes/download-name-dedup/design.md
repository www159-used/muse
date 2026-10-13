## Context

The application currently selects NCM only; the library already decodes MP3/FLAC and owns complete audio, metadata, source records and attachments. The user explicitly selected tagged title plus artist as the duplicate identity. Filename equality is not the requested rule.

## Goals / Non-Goals

**Goals:** Include ordinary downloads, skip alternate files of a known song, preserve the retained audio, avoid merging different artists or missing metadata, and verify the real import workflow.

**Non-Goals:** Removing existing duplicates, selecting higher quality automatically, fuzzy names, artist-order equivalence, Unicode normalization, supporting additional codecs, changing the database schema or changing playback/search.

## Decisions

- **Confirmed:** Application imports NCM, MP3 and FLAC; tagged title and artist together define additional duplicate matching.
- **Implementation choice:** Add `Library::import_downloads_with_progress` alongside content-only and NCM-only entry points. Share decoding, progress, cancellation and attachment handling. Only the download policy performs additional metadata lookup.
- **Implementation choice:** Trim surrounding whitespace and lowercase Unicode title and artist for comparison. Require both actual metadata fields to be nonempty before applying filename fallback. Keep exact interior text, punctuation, versions and artist order; no title-only or filename identity.
- **Implementation choice:** Check source hash, stored audio hash and then metadata identity inside the existing immediate SQLite transaction. Compare persisted metadata in Rust to support Unicode lowercasing. Choose the earliest existing match and retain its metadata/audio. Persist the new source record and missing attachments in the same transaction; a duplicate never writes a new audio BLOB.
- **Implementation choice:** Use persisted Track metadata directly. A separate identity table/index and schema migration would add derived state before scale requires it. Metadata-only lookup scans known-artist rows, runs only after hash lookup misses and can later be indexed without changing the public import policy.
- **Implementation choice:** Process NCM first within a fresh folder import to keep rich metadata/artwork when alternate versions coexist, retaining deterministic path order inside each group. An existing library entry always wins; invalid preferred sources do not reserve names and other valid sources continue.
- **Implementation choice:** Completion and English help/progress list all three formats. Independent Cucumber stories use synthetic fixed tagged audio and public download import APIs, including a restart step.

## Risks / Trade-offs

- Live/remastered versions with identical title and artist merge intentionally under this policy; distinct titles or artists remain distinct, and content-only imports can preserve versions.
- Metadata scans are linear in existing known-artist tracks; hash matches avoid the scan. Do not introduce a migration for an unmeasured scale concern.
- Incomplete tags can cause false identity → disable metadata matching when incoming tagged title or artist is missing and retain byte matching.
- Existing duplicate versions stay in place; this change affects future imports and does not delete or rewrite audio.

## Migration Plan

No schema upgrade is needed. Existing libraries supply stored title/artist metadata immediately. Backup/restore remains compatible; re-importing a download folder can add ordinary sources and supplement missing attachments.

## Open Questions

None for this increment.
