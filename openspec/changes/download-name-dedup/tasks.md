## 1. Download import policy

- [x] 1.1 Add mixed-format download selection and atomic tagged-title/artist deduplication, preserving hashes, retained audio, attachments and generic APIs.
- [x] 1.2 Integrate application imports, completion and English help/progress with all three source formats.

## 2. Verification and documentation

- [x] 2.1 Add readable mixed-download and metadata-duplicate stories plus focused checks for concurrency, missing metadata, deletion and failures.
- [x] 2.2 Document the rule and limitations, run full workspace CI and strict OpenSpec validation, and record results.

## Verification

- The mixed-download tracer first failed because only one NCM song was added instead of the expected three NCM/MP3/FLAC songs. It passes through the public download importer with exact golden audio and unchanged sources.
- Thirteen Cucumber scenarios with 109 steps pass. Five new stories cover alternate filenames/encodings, retained MP3 audio and missing-cover supplementation after reopening, different artists sharing title/filename, missing tags and case differences. Distinct-track and recursive stories now explicitly use different-artist metadata.
- Eight focused public-library workflows pass for Unicode lowercase/whitespace, missing title/artist, concurrent connections committing one retained song and two source records, backup/restore and deletion, attachment failure rolling back its source record and retrying, an invalid preferred NCM with valid ordinary audio, cancellation and compatibility with content-only/NCM-only APIs.
- Application tests pass for ordinary folder/direct imports and unsupported-file errors, all-format completion/cycling, English guidance and progress totals of 400 selected files with one added song and 399 duplicates. Search, navigation and real terminal regressions continue to pass.
- `make ci` passed formatting, the standard-library playback interface check, Clippy with denied warnings, 92 ordinary Rust tests, the 13 import scenarios, two documentation tests and the release build. Three existing physical-device/personal-source checks remain ignored.
- `openspec validate --all --strict` passed all fourteen changes. All import execution used temporary libraries and synthetic fixtures; the personal download directory was inspected read-only for extension/name counts and no personal library was changed.
