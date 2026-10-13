## Why

NetEase download folders also contain ordinary MP3 and FLAC files, which the current NCM-only application importer misses. Complete-byte deduplication also keeps separate encodings and tag versions of the same song; the user requests deduplication by tagged song title and artist.

## What Changes

- Accept NCM, MP3 and FLAC in application imports and path completion, with matching progress and English guidance.
- Add a public download import policy that also deduplicates known title/artist pairs, retaining the existing audio and supplementing missing attachments.
- Keep content deduplication for renamed sources and missing identity metadata; preserve the general-purpose content-only library API and the explicit NCM-only API.
- Use stored title/artist metadata across restarts without a schema migration, deleting tracks or merging existing versions.
- Add readable import stories for ordinary downloads, different files representing one song and different artists sharing a title.

## Capabilities

### New Capabilities
- `download-import`: Mixed-format download imports with persistent tagged-title/artist identity and transactional duplicate outcomes. This supersedes the application's NCM-only selection from the earlier change.

### Modified Capabilities
None; existing capabilities have not yet been archived into main specs.

## Impact

The library importer and transactional duplicate lookup, terminal import worker, completion, progress/help text, documentation and independent import stories change. Playback, search, storage schema and backup interfaces remain compatible. No source files or personal library are modified during development.
