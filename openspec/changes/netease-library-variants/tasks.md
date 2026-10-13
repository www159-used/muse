## 1. Typed library boundary

- [x] 1.1 Introduce the library descriptor and explicit type dispatch, defining NetEase as the initial supported type; verify default creation, stored name/type, unknown-type errors, and preservation of ordinary audio without platform IDs.
- [x] 1.2 Move NCM parsing, NetEase metadata, local source selection, and attachment association behind the built-in NetEase module; keep existing library entry points as compatible adapters and verify mixed-format fixture imports without network access.

## 2. Candidate schema and safe upgrade

- [x] 2.1 Implement the version-2 candidate/provenance schema with shared audio blobs, stable existing candidate IDs, and legacy-source verification flags; verify constraints with conflicting platform IDs, ID-less sources, and both import orders.
- [x] 2.2 Add version-aware read-only validation and backup/restore for both supported schemas; verify WAL-inclusive snapshots, no-overwrite behavior, and old-backup restoration without source upgrade.
- [x] 2.3 Implement explicit offline library upgrade with maintenance ownership, active-service rejection, verified uniquely named pre-upgrade backup, atomic migration, and failure rollback; verify old IDs/audio/attachments survive and failed backup leaves schema untouched.
- [x] 2.4 Update client/service compatibility checks and normal-open diagnostics so old libraries require the explicit upgrade path and incompatible live services are not silently reused.

## 3. Retention and reimport

- [x] 3.1 Replace application title/artist suppression with the candidate-resolution rules, source observations, and separate retained-collision/duplicate reporting; verify same-ID alternate audio, different-ID identical audio, missing tags, concurrent imports, and exact repeats.
- [x] 3.2 Revalidate legacy source mappings using freshly read/decoded bytes and atomically repair candidate associations; verify recovery of previously suppressed NCM/MP3/FLAC, repeat idempotence, unavailable sources, and failures that remain retryable.
- [x] 3.3 Preserve candidate-specific attachment provenance and prevent cross-candidate supplementation by name alone; verify distinct recordings keep their own lyrics/artwork and legacy attachments are not redistributed.
- [x] 3.4 Add readable import stories for retained alternatives and migration/reimport recovery, and revise older stories that explicitly expected name-based suppression while retaining source-integrity and golden-audio assertions.

## 4. Shared library commands

- [x] 4.1 Implement deterministic NetEase song entries and collision-group projections, group tokens, candidate details, and all-candidate search; verify missing tags, representative metadata differences, stable ordering, and stale group anchors.
- [x] 4.2 Add shared application operations and one-shot list/show/import/library-info/upgrade entry points, including JSON query output and global --library handling; verify read commands never create databases or start services and explicit-ID play retains compatibility.
- [x] 4.3 Add persistent command mode with in-session history, help, completion, quoted paths, shared operations, and stop/detach/EOF semantics; verify multiple commands and import cleanup in a real terminal.
- [x] 4.4 Add group-play disambiguation in command mode and noninteractive candidate-list errors for ambiguous group playback; verify cancellation preserves playback and exact candidate selection reaches the service.

## 5. TUI selection and external updates

- [x] 5.1 Render library name/type and grouped rows with candidate counts, and adapt numbered navigation and search to groups; verify existing viewport, focus, search preview/cancellation, Unicode, and tiny-terminal behaviors.
- [x] 5.2 Add the candidate chooser with stable IDs and accurate available metadata; verify single-candidate play, multiple-candidate confirmation, Esc cancellation, load failure, and no interruption of current audio before confirmation.
- [x] 5.3 Add a background library-change reader and reusable refresh path that updates groups/search while preserving input and selection; verify committed CLI imports, selected-item removal, read failures, and chooser changes requiring fresh confirmation.
- [x] 5.4 Add end-to-end CLI/command-mode/TUI workflows using temporary typed libraries and the existing deterministic service test backend; verify an imported alternative appears in another client and only the confirmed candidate plays.

## 6. Documentation and verification

- [x] 6.1 Update README/help/project context with typed-library semantics, NetEase/NCM ownership, retained candidates, command-mode usage, group versus candidate IDs, migration/reimport limitations, and automatic TUI refresh; keep API and aggregation explicitly deferred.
- [x] 6.2 Run workspace CI and strict OpenSpec validation, record migration/backup/import/process/terminal evidence, and identify any platform or physical-device checks not executed.

## Verification record

See [verification.md](verification.md) for temporary-library migration, backup, import, process and terminal evidence. Existing completed change records remain historical; this implementation replaces their name-based suppression policy. No personal music library was modified.
