# Import stories

`muse-e2e` is a test-only workspace package with English Given / When / Then stories. Each scenario imports a real temporary source folder into its own SQLite library through public `muse-library` interfaces.

Start with [repeat-import.feature](scenarios/import/repeat-import.feature): import one NCM folder, then import it again and verify that the library still owns one track with the exact unpacked MP3 bytes.

## Scenarios

| Story | Observable result |
| --- | --- |
| [First import](scenarios/import/first-import.feature) | One MP3 track, exact audio, artist, album, NetEase ID and embedded cover. |
| [FLAC import](scenarios/import/flac-import.feature) | One FLAC track with exact unpacked audio, metadata and embedded cover. |
| [Multiple tracks](scenarios/import/multiple-tracks.feature) | Distinct MP3 and FLAC audio with different artists remain in the library, even with the same title. |
| [Recursive folders](scenarios/import/recursive-folders.feature) | Songs in separate artist/album folders and a nested disc folder are imported. |
| [Unicode paths](scenarios/import/unicode-paths.feature) | Chinese/Korean names, spaces and an uppercase `.NCM` extension work. |
| [Mixed downloads](scenarios/import/mixed-downloads.feature) | NCM and ordinary MP3/FLAC are all imported. |
| [Retained alternatives](scenarios/import/metadata-duplicates.feature) | Same title/artist across filenames, encodings, case differences and restarts retains playable alternatives in a browsing group; different artists and missing tags remain separate. |
| [Legacy recovery](scenarios/import/legacy-recovery.feature) | Explicit backed-up upgrade preserves the legacy ID; reimport recovers suppressed audio once. |
| [Local attachments](scenarios/import/local-attachments.feature) | Matching local lyrics and a NetEase `meta/track-321.jpg` cover retain their exact bytes. |
| [Repeat import](scenarios/import/repeat-import.feature) | The second import adds nothing, skips one duplicate and preserves the owned audio. |

Every import checks added/duplicate counts, rejects unexpected failures or issues, verifies library audio against independent fixtures and checks that all prepared source files remain unchanged. Track tables describe the complete library and match each row once without depending on import order.

## Running

```sh
make e2e
make e2e-list
make e2e E2E_FILTER=twice
make e2e E2E_FILTER=FLAC
cargo test -p muse-e2e --locked --test import -- --name twice
```

`make e2e` prints each Given / When / Then step and its result. `make e2e-list` lists stories without creating a library. Filters match scenario names using a regular expression. Workspace tests and CI include all scenarios.

## Feature and step definitions

The feature describes user actions and observable outcomes. [Cucumber's step attributes](https://cucumber-rs.github.io/cucumber/main/quickstart.html) connect each sentence to a Rust function; expressions extract typed parameters. One step definition can serve many future stories.

| Story sentence | Rust definition |
| --- | --- |
| `Given "downloads/song.ncm" is a copy of the NCM fixture "standard.ncm"` | [downloaded_ncm](tests/steps/given.rs) copies the synthetic fixture into the source tree. |
| `And "downloads/plain.mp3" is a copy of the fixture "tagged.mp3"` | [source_file](tests/steps/given.rs) prepares other audio or artwork files. |
| `And the text file "downloads/song.lrc" contains:` | [text_file](tests/steps/given.rs) writes the following literal docstring. |
| `When I import the folder "downloads"` | [import_folder](tests/steps/when.rs) calls the public download importer and checks unchanged source bytes. |
| `When I reopen the library` | [reopen_library](tests/steps/when.rs) replaces the connection with one opened from the same persisted library. |
| `Then 1 track is added` | [added_tracks](tests/steps/then.rs) checks the latest import report. |
| `And 1 duplicate is skipped` | [skipped_duplicates](tests/steps/then.rs) checks the duplicate count; singular/plural forms share a function. |
| `And the import finishes without errors` | [clean_import](tests/steps/then.rs) checks failures, scan/attachment issues, cancellation and stopped imports. |
| `And the library contains only "Fixture NCM" with audio "silence.mp3"` | [owned_track](tests/steps/then.rs) checks the sole title, format and exact owned audio bytes. |
| `And the library contains exactly these tracks:` | [owned_tracks](tests/steps/then.rs) checks the complete title/audio table, including byte identity and format. |
| `And the imported track has artist "Fixture Artist", album "Fixture Album" and NetEase ID 321` | [track_metadata](tests/steps/then.rs) checks explicit metadata values. |
| `And the imported track has a PNG cover matching "cover.png"` | [track_cover](tests/steps/then.rs) checks one cover, its MIME type and exact fixture bytes. |
| `And the imported track has these lyrics:` | [track_lyrics](tests/steps/then.rs) checks one lyrics attachment, its MIME type and the literal docstring bytes. |

The [World](tests/support/world.rs) holds one real temporary source directory, a separate SQLite library and the latest import report. Cucumber creates a new World for each scenario and registers the step definitions automatically. [The entry point](tests/import.rs) starts the runner and treats skipped/undefined steps as failures. An empty story directory also fails.

All dependencies are dev-dependencies. Production packages do not depend on this crate, and the stories need no terminal, audio device, NetEase installation, network service or personal music. There are no production test hooks, database mocks or SQL assertions. Source and fixture paths stay inside their respective roots.

The stories use fixed synthetic fixtures; tests never generate expected audio with the production decoder. See [fixture provenance](fixtures/README.md). Other fixture variants remain available for future stories. Focused library and application tests remain in their existing crates.

## Adding a story

Add one focused scenario with explicit source paths and observable outcomes. Reuse existing step definitions for the same behavior. Add a small, explicit definition for a new action or expectation; keep file/database setup in World and user outcomes in Then functions. Use tables for parallel track records and docstrings for literal lyrics.
