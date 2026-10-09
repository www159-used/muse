## Context

The terminal worker imports all supported sources through muse-library. Completion independently offers the same extensions. A read-only audit found 68 pairs of plain MP3/NCM imports with identical audio after excluding ID3 tags, but different complete-byte hashes. The user chose NCM-only application imports.

## Goals / Non-Goals

**Goals:** Make :import select only NCM sources, align completion and progress, retain metadata/attachments, stored MP3/FLAC, transaction semantics and cancellation.

**Non-Goals:** Audio-frame deduplication, source-based list filtering, deleting or merging existing records, changing playback formats or introducing import flags.

## Decisions

### Chosen: A source selection inside the library pipeline

Add Library::import_ncm_with_progress over the existing shared pipeline. A private source selection controls single-file validation and recursive discovery before audio reads/decryption. Folder totals include only selected NCM files; excluded MP3/FLAC do not become audio failures or duplicates. A directly requested non-NCM file returns an English source failure. Extension checks remain case-insensitive.

The general-purpose library entry points retain their existing behavior for direct consumers and existing backup/playback checks. The application always calls the NCM-only entry point. Changing every library caller was considered but would unnecessarily remove the reusable library's ordinary audio ingestion.

### Chosen: Keep storage and import outcomes

Selected NCM files use the existing immutable source snapshot, decoder, content hashes and song transaction. Sidecar discovery/association, error handling and cancellation remain shared. There is no fallback to plain files when a folder contains no NCM; that import completes with zero audio work.

### Chosen: Match completion and visible guidance

Offer directories and NCM files only, preserving case handling, quoted Unicode paths, common prefixes and cycling. Help and :import guidance state NCM source support. Scan labels identify NCM files, while song list formats continue to show the actual stored MP3/FLAC.

### Chosen: Compose Hangul in progress labels only

Actual Korean source filenames use decomposed Hangul jamo, while stored metadata contains composed syllables. Progress widgets output the decomposed sequence directly. Apply the [Unicode Hangul composition algorithm, section 3.12](https://www.unicode.org/versions/latest/ch03.pdf) to displayed current filenames and source labels; keep original paths in worker state, callbacks, file access and hashes. Only modern Hangul composition is needed, and other text remains unchanged. A small display helper avoids a dependency for this bounded algorithm.

The regression command cargo test -p muse --locked import_progress_displays_decomposed_korean_names_as_complete_syllables first failed: Current and Source contained decomposed jamo rather than the expected EXID - 덜덜덜.ncm. It also asserts that the original snapshot path survives rendering. Terminal-width mismatch and missing-font glyphs were considered; the first correction composes the actual path string before measuring/rendering without changing unrelated terminal settings.

## Risks / Trade-offs

- Plain-only songs are excluded → document NCM-only selection and zero-result behavior.
- Existing duplicate records remain → document the existing make reset-library route for a deliberate rebuild after exiting muse; implementation does not delete the current library.
- NCM files with different decoded bytes can still be distinct → preserve the chosen complete-byte deduplication policy and avoid claiming that source selection merges all song versions.

## Migration Plan

Add source selection, switch the application worker/completion, update help and documentation, then run regression checks and a real terminal mixed-folder import. No schema or dependency migration. Existing libraries stay readable. Reverting the worker/completion selection restores the earlier application source policy.

## Open Questions

None required.

## Verification

- Final make ci passed on macOS: formatting, the standard-library playback interface check, Clippy with warnings denied, 61 ordinary tests, two doctests and the release build. The three physical-device/personal-sample checks remain opt-in.
- NCM-only library regressions cover recursive mixed directories, uppercase extensions, exclusion from counters, malformed NCM, repeated imports, preserved unpacked audio/cover/lyrics, direct plain-file rejection, no fallback in plain-only folders and cancellation after a committed song.
- Updated completion and application checks cover NCM-only candidates, preserved Unicode/quotes, cycling and mode transitions, real 200-NCM/200-MP3 worker selection, mixed imports and direct MP3/FLAC errors.
- The decomposed-Hangul progress regression was observed failing before the fix and passing afterwards. It covers actual EXID and Trouble Maker filenames, an already-composed LV plus trailing jamo, preservation of compatibility jamo/other combining text, and unchanged worker/snapshot paths. No diagnostic instrumentation remains.
- A real terminal smoke check used the release binary and temporary sources: 5,001 NCM files, ordinary MP3/FLAC files, and decomposed Korean filename/directory text. Unique completion selected NCM only. Live progress showed the composed Korean filename/source and total 5,001; completion retained the one prior song and skipped 5,001 repeated NCM sources. Plain-only folder import did zero work, and directly requesting MP3/FLAC displayed the English rejection. NORMAL/COMMAND transitions, SQLite integrity, source hash identity, exact unpacked audio bytes, unchanged source files and terminal restoration passed.
- All eight OpenSpec changes pass strict validation. Linux/Windows execution and terminal font rasterization were not examined in this session.
