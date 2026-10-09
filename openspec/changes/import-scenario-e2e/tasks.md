## 1. Independent runner

- [x] 1.1 Add the test-only workspace package, a readable single-file import tracer case and automatic independent test registration; demonstrate a failing expectation before making it pass.
- [x] 1.2 Implement strict case loading, isolated fixture trees, public-interface outcomes and progress/source preservation checks, with negative harness tests.

## 2. Import corpus

- [x] 2.1 Add golden fixture variants and readable cases for selection/layouts, duplicates, persistence, damaged input, metadata evolution, attachments and cancellation/retry.
- [x] 2.2 Migrate equivalent high-level library tests after verifying scenario coverage; retain focused storage, race and UI tests.

## 3. Integration and verification

- [x] 3.1 Add Make targets, contributor guidance and context; verify case discovery/filtering, full workspace CI, runtime dependency separation and strict OpenSpec validation.

## Verification

- Tracer: the single-NCM case initially failed with its filename and `step 1: import scenario execution is not implemented`, then passed through real source files and SQLite.
- `make e2e`: all 39 import workflows and five runner regression tests passed. Wrong counters, same-format wrong audio bytes, wrong metadata/artwork, malformed expectations, unknown operations, empty workflows, path escape and recursive/empty discovery are checked.
- `make e2e-list`: exactly 39 cases listed independently. `make e2e E2E_FILTER=import/metadata/` selected the four metadata compatibility cases.
- Temporary negative case files proved automatic registration without Rust edits, duplicate-name failure and a wrong expectation failing with file/step/count and nonzero exit. Both temporary cases were removed.
- `cargo metadata --locked --offline --format-version 1`: the package is a workspace member with only test targets and dev-dependencies; runtime packages do not depend on it.
- Six overlapping high-level tests migrated to the corpus; all 16 remaining focused library integration tests passed. Transaction failure injection, source mutation races, ownership, storage checks and UI tests remain at their existing seams.
- Latest `make ci`: formatting, the default-free playback interface check, Clippy with denied warnings, 105 workspace tests, two documentation tests and the release build passed. The total includes two terminal tests added concurrently elsewhere in the workspace; the transient hangup failure observed during that concurrent edit was resolved before the final successful run. Three existing physical-device/personal-source tests remain ignored.
- `openspec validate --all --strict`: all ten changes passed; `openspec validate import-scenario-e2e --strict` also passed.
- No personal library or source directory was used. The independent fixture corpus consists only of generated audio and synthetic metadata.
