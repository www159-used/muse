## 1. Story execution

- [x] 1.1 Prove a readable single-NCM story fails before support, then implement standard Gherkin loading and execution with action-line diagnostics.
- [x] 1.2 Extend the finite vocabulary through multistep, metadata, attachment, failure and cancellation cases; reject incomplete, misplaced and unsupported assertions with negative runner tests.

## 2. Corpus migration

- [x] 2.1 Replace all 39 TOML cases with equivalent stories, retaining setup, actions and expected reports/audio/metadata/attachments.
- [x] 2.2 Remove E2E TOML/Serde dependencies and update contributor examples and project context.

## 3. Verification

- [x] 3.1 Verify automatic registration, listing/filtering, dependency separation, full workspace CI and strict OpenSpec validation.

## Verification

- The single-NCM Gherkin tracer failed through the old TOML loader before support, then passed with real files, SQLite, golden audio, metadata and cover checks. Moved-directory, late-lyrics and cancellation workflows were extended through failing/passing checks.
- Before deleting TOML cases, a temporary migration check compared all 39 old/new names, complete Given setups and every operation/expected outcome for equality, then executed every new story. This caught and corrected docstring boundary newlines. The temporary comparison was removed together with the legacy format.
- All 39 independently registered `.feature` stories and eight runner regression tests passed. Negative checks cover wrong counters/audio/metadata/cover, unknown steps/fields, incomplete outcomes, unsupported Gherkin constructs, repeated/misplaced expectations, contradictory setup, attachment expectation ordering, path escape and empty/recursive discovery.
- `make e2e-list` listed exactly 39 stories. `make e2e E2E_FILTER=import/metadata/` passed four metadata stories and filtered the other 35.
- Temporary story files proved automatic discovery and execution without Rust edits, nonzero failure with file/action-line/count diagnostics, and duplicate-name rejection. They were removed; only the 39 permanent stories remain.
- `cargo metadata --locked --offline --no-deps --format-version 1` confirmed a workspace member with only test targets and dev-dependencies, and no production dependency on muse-e2e. E2E's TOML/Serde dependencies were removed.
- `make ci` passed formatting, the standard-library-only playback interface check, Clippy with denied warnings, 109 workspace tests, two documentation tests and the release build. Three existing physical-device/personal-source tests remain ignored.
- `openspec validate --all --strict` passed all eleven changes. No personal library or source tree was used.
