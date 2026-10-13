## Context

Status: implemented and locally verified following the confirmed source-type and collision-retention direction. Commands and schema choices below are engineering defaults, not additional user decisions. See verification.md for evidence and execution limits.

Schema version 1 has one `tracks` row per unique encoded audio hash, one primary attachment per track/kind, and `import_sources(source_hash, track_id)`. The download importer additionally matches trimmed, Unicode-lowercased tagged title/artist and points every matching source at the first retained track. On reimport, that source mapping bypasses NCM audio decoding and reuses the retained hash. Consequently, changing only the title/artist lookup would not recover previously skipped audio.

NCM decoding, NetEase metadata, generic audio tags, directory selection, and attachment association currently live together in `muse-library`. The playback service loads a stable integer track ID. TUI rows and searches are cached until its own import completes. Interactive command mode and library CLI queries have been discussed but are not implemented.

## Goals / Non-Goals

**Goals:** One explicitly typed library per file; NetEase local rules in a built-in extension; retain audio alternatives and their identities; choose an exact candidate before changing playback; share operations across CLI, command mode, and TUI; migrate without losing owned content.

**Non-Goals:** API integration, account login, online playback, downloading lyrics/artwork, other source implementations, cross-library aggregation, dynamic plugins, remembered preferred versions, automatic audio-quality ranking, perceptual audio matching, and a new deletion UI.

## Decisions

### 1. Typed library instances and a built-in NetEase module

Persist a singleton library descriptor containing name and type (`netease` initially). The file selected by `--library` remains the current library for the entire client session; the default path remains `./library.muse`. New default libraries use type `netease` and a filename-derived display name. Unknown types fail explicitly; their data is never reinterpreted as NetEase.

As the migration default, existing libraries become NetEase-type instances, preserving all valid MP3/FLAC and stored rows. This classifies their import behavior, not their proven origin: no NetEase song ID is invented for ordinary audio. Show name/type in TUI, `library info`, and the command-mode prompt. Changing type in place and switching files inside a running client are deferred.

Start with a compiled `muse-library::netease` module and explicit type dispatch, rather than a plugin ABI. Move NCM parsing, NetEase metadata decoding, local download selection/association, and song-group projection there. Storage owns transactions, blobs, provenance, and migration. Shared tag readers remain helpers. Preserve existing public content-only and NCM-only entry points as compatibility adapters; remove only the application policy that discards different audio on a name match. The playback crate never receives an NCM container or platform-specific command.

### 2. Separate audio content, candidates, and presentation groups

Keep `audio(content_hash, data)` as immutable, content-addressed encoded audio. NCM is unpacked first; identity uses the exact stored MP3/FLAC bytes, including tags. Equal sound with different encoded bytes is not treated as exact content equality.

Keep existing `tracks.id` values as stable playable candidate IDs. Permit multiple candidates to reference one blob when platform identities conflict, replacing the global unique constraint on `tracks.content_hash` with suitable identity/content indexes. Add provenance observations for source hash, observed path/container format, tagged metadata, platform ID, and actual decoded audio hash. Source metadata must survive even when audio is shared. Paths are descriptive provenance, never dependencies for playback.

Resolution rules inside one immediate transaction:

1. A previously verified source resolves idempotently to its candidate, with optional missing-attachment supplementation.
2. Same explicit NetEase ID and same content hash reuse the candidate. Same ID with different content creates another candidate for that platform song.
3. Different explicit NetEase IDs remain separate candidates even if the blob is identical. Store the audio once and retain both identities.
4. For missing IDs, exact content can reuse a uniquely compatible existing candidate. If an unbound candidate later gains one unambiguous explicit ID, enrich it while preserving its ID. Conflicting known identities must never be collapsed through an ID-less source; retain an unbound candidate if resolution is ambiguous.
5. Different content without a platform ID remains a separate candidate. Matching real title/artist tags only makes it eligible for a display collision group. Filename fallback is never identity evidence.

Report added candidates, exact duplicate sources, and collisions retained distinctly; do not count a newly retained alternative as a skipped duplicate. A collision is metadata ambiguity, not a cryptographic hash collision.

For browsing, first collect candidates with the same NetEase ID into a platform-song entry; an ID-less candidate remains its own entry. Entries whose representative real title/artist keys match share a presentation group. Use the oldest candidate's tagged metadata as the stable representative of a platform-song entry; show individual differences in the chooser. Missing-tag entries do not group merely because filenames match. Same-name groups can contain multiple NetEase IDs and do not assert those songs are equivalent.

Group order follows the minimum candidate ID. Expose a group token `g:<anchor-candidate-id>` in CLI output, with the lowest member ID as anchor. Resolve tokens against the current projection; a removed anchor makes its old token invalid. TUI line numbers remain row positions, now referring to groups. Exact candidate IDs never mean row numbers.

### 3. Candidate-owned metadata and attachments

Each candidate retains its format, album, duration, platform ID, and associated local attachments. Source observations retain filenames and container formats; the chooser can show “NCM source / FLAC audio” accurately. Show bitrate, sample rate, and bit depth only if actually parsed; format alone is not a quality ranking.

Never copy lyrics or artwork between different audio candidates based only on title/artist equality. Exact-candidate reimports may fill missing primary attachments. Keep conflicting observations/provenance so later attachment policies can be extended without pretending metadata was verified. Existing attachments remain on their legacy candidate, marked as legacy attribution where their contributing source cannot be proved; migration does not redistribute them to recovered candidates.

### 4. Selection is client-local; playback starts with an exact ID

Enter on a one-candidate group plays directly. Enter on multiple candidates opens a chooser showing exact ID, format, duration, album, platform ID when present, and observed source label. `j/k` or arrows select, Enter confirms, and Esc cancels. Ignore unrelated navigation/playback keys while the chooser is focused. Existing audio keeps playing throughout browsing and cancellation. Space retains its existing playback toggle behavior outside the chooser; when no track is loaded and it would play the selection, use the same resolution path as Enter.

Search matches metadata across all candidates and marks a group when any member matches; candidate details identify the matching versions. Group counts and numbered navigation refer to visible group rows. Retain viewport/focus and search cancellation behavior using stable group/candidate identities rather than raw row indexes.

The service still accepts exact candidate IDs. Before load, it verifies that the candidate exists and belongs to the selected library. The picker holds stable IDs, never an index to be reinterpreted after refresh. If its membership changes, redraw the choices and require a fresh confirmation; if the selected candidate disappears, report it without starting a replacement. Current playback state remains service-owned and independent of selection.

### 5. Shared commands and interactive mode

Proposed commands:

| Invocation | Meaning |
|---|---|
| `muse` | Existing full-screen TUI |
| `muse command` | Interactive line-oriented command mode |
| `muse list [--search <text>] [--candidates] [--json]` | Groups by default, exact candidates on request |
| `muse show <candidate-id or g:anchor> [--json]` | Candidate details or current group alternatives |
| `muse import <path>` | Current library type's local importer |
| `muse play <candidate-id>` | Existing exact-ID playback semantics |
| `muse play g:<anchor>` | Resolve group and play or request disambiguation |
| `muse library info` | Current instance name, type, and schema |
| `muse library upgrade` | Explicit offline schema upgrade with backup |

All entries use an application command dispatcher with typed results. The REPL is a persistent client, not a command-string wrapper and not a shell. Support help, in-session history, command/path completion, quoted Unicode paths, and the existing playback commands. Only prompt/selection is client-specific; operations and results are shared. `quit`/`q`, handled Ctrl-C, and EOF stop playback and exit; `detach` preserves playback. Import cancellation always finishes its current transaction.

In command mode, `play g:...` with multiple candidates presents numbered choices and permits cancellation. One-shot `play g:...` instead returns nonzero with candidate IDs and instructions to issue exact-ID play; it never prompts or picks the first. `play 42` remains unambiguous and backward compatible. `list --json` and `show --json` provide stable IDs and structured data; diagnostics/progress go to stderr. Library queries and imports need no playback service. Read commands never create missing libraries; import may create a default NetEase library as documented.

### 6. External refresh without API or service ownership of imports

Extract the existing TUI import-completion refresh into a shared refresh path. A background reader retains one SQLite connection and checks its data-version marker approximately every 500 ms; only a detected commit reloads metadata and the group projection. Compare versions only on that reader's own connection, and keep queries outside long-lived snapshots. Read the version before the metadata snapshot so a racing later commit remains detectable on the next check.

Publish successful snapshots to the UI thread. Preserve the selected group/candidate when possible, otherwise select a nearby row, clamp the viewport, and recompute pending/confirmed search matches. Do not clear command text, move to the playing song automatically, or issue playback commands. Candidate chooser changes require reconfirmation. On read failure, retain the last valid display, show an error, and retry with bounded waits.

This handles CLI imports and other committed library changes. Import progress stays with its initiating client; global job management is deferred. Each client still obtains playback snapshots from the service.

## Risks / Trade-offs

- [Title equality groups different recordings] → Use groups only for presentation; preserve individual IDs, audio, album, duration, and explicit selection.
- [Old source records claim discarded audio already exists] → Mark legacy mappings unverified and fully reread/redecode on reimport before trusting them.
- [Different encodings consume more storage] → Deduplicate exact blobs and show honest candidate counts; do not silently discard versions.
- [Ambiguous identity enrichment becomes import-order-dependent] → Record every observation, never merge conflicting explicit IDs, and test both import orders.
- [Grouping changes existing row numbering and search] → Keep exact candidate IDs stable and add rendered navigation/search regression workflows.
- [Schema change reaches a running old service] → Require offline upgrade and protocol compatibility checks, rather than upgrading during normal startup.

## Migration Plan

Introduce schema version 2 with explicit instance metadata, candidate/provenance support, and legacy-source verification flags. Keep old candidate IDs, owned blobs, attachments, and playable relationships. Preserve historical duplicate candidate rows instead of silently merging them during migration.

Normal new-version clients encountering version 1 give an actionable `library upgrade` instruction. The upgrade checks for a running service, acquires exclusive application maintenance ownership, and requires other clients to close. New clients honor the same maintenance guard; old clients must be closed before upgrading. Fail rather than automatically stopping an active session. Bump/validate service protocol compatibility where the new candidate contract requires it.

Before schema writes, create and validate a no-overwrite SQLite backup named `<library>.pre-v2.<unique-suffix>.muse`, including committed WAL data. Keep it until explicitly removed by the user; do not add an automatic retention policy. If backup cannot complete, leave the original schema untouched. Execute migration atomically and verify foreign keys and audio references. Backup/restore must explicitly validate both supported schema layouts, including old backups, without upgrading their source while copying.

Mark old source mappings unverified because the schema cannot distinguish exact duplicates from title/artist-suppressed variants. On reimport, use one owned input snapshot to parse/hash/decode anew. Atomically attach the verified source to the correct candidate, inserting missing audio as needed. Failure leaves the old library content intact and the mapping unverified for retry. Repeating successful reimport creates no further candidates. Missing source files remain unrecoverable; do not invent audio, paths, or recovered-version counts.

Rollback uses the verified pre-upgrade backup after closing clients/service; it cannot include changes made after migration. No source file is modified or removed.

## Open Questions

- Final SQL indexes and the compatibility adapter signatures should be settled against migration fixtures in the first implementation slice; the identity and preservation behavior above is the contract.
- Exact spellings of group tokens and CLI formatting are proposed defaults. Dynamic source switching, preferred-version persistence, additional library types, and future API metadata enrichment remain deferred.
