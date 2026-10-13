## Why

muse currently embeds NetEase-specific import rules inside a general library and discards different audio when tagged title and artist collide. A typed NetEase library should own NCM parsing and preserve those alternatives, letting the user choose what to play while keeping CLI and TUI views consistent.

## What Changes

- Persist each library instance's name and source type. Implement NetEase as the first built-in extension; retain MP3/FLAC import alongside NCM parsing and local attachment association.
- Retain different encoded audio and conflicting platform identities instead of merging by title/artist. Deduplicate audio bytes independently of song identity and retain source observations.
- Show collision groups with selectable candidates. A single candidate plays directly; multiple candidates require confirmation in TUI and interactive command mode. Explicit candidate IDs remain usable noninteractively.
- Add the previously discussed interactive `muse command` mode, library listing/search/details/import commands, and automatic refresh of externally committed library changes in an open TUI.
- **BREAKING**: Replace application's title/artist duplicate suppression with candidate retention, introduce a new storage schema, and require an explicit offline upgrade of existing libraries after a verified backup. Preserve existing stored audio and candidate IDs; recover discarded audio only by reimporting available source files.
- Keep API access, remote metadata retrieval, other source implementations, dynamic plugin loading, and cross-library aggregation out of scope.

## Capabilities

### New Capabilities

- `typed-netease-library`: Library instance metadata, built-in extension boundaries, and NetEase-specific local import rules.
- `retained-audio-candidates`: Content deduplication, platform identity, candidate provenance, and safe legacy migration/reimport.
- `candidate-playback-selection`: Grouped browsing and explicit candidate selection before playback.
- `library-command-mode`: Interactive command mode and one-shot library commands sharing application operations.
- `external-library-refresh`: Automatic TUI refresh after CLI or other application connections commit library changes.

### Modified Capabilities

None in main specs: `openspec/specs` is empty. This change supersedes the title/artist suppression policy in the completed `download-name-dedup` change. It extends the completed service, search, and navigation behaviors without replacing their stop/detach or playback contracts.

## Impact

Affected areas include `muse-library` schema/import/NCM/attachments/backup APIs; application command parsing and dispatch; TUI browsing/search and selection; playback service candidate validation; and migration, import, command, and terminal tests. The standard-library playback interface continues to receive owned audio sources, without knowing about NCM or NetEase rules.

Confirmed direction: source-based library types, NetEase first with NCM parsing, no API for now, retaining collisions with a second playback choice, interactive command mode, and CLI-to-TUI synchronization. Exact command spellings, tables, grouping presentation, and migration mechanics in the design are proposed engineering choices.
