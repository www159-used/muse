## Why

Import cases expressed as nested TOML still read like configuration. Contributors need ordered, readable stories that explain the source layout, user actions and expected library behavior without knowing the runner's internal model.

## What Changes

- Replace the 39 TOML workflows with English Gherkin `.feature` stories using Given, When and Then.
- Use fixture tables and literal text for source setup, followed by explicit report, audio, metadata and attachment expectations.
- Keep independent discovery, filtering, real temporary files and SQLite, public library interfaces and the test-only workspace package.
- Reject unsupported or incomplete stories and show the relevant source line on failure.

## Capabilities

### New Capabilities

- `readable-import-stories`: Executable, ordered import stories and strict diagnostics with preserved regression coverage.

### Modified Capabilities

None. Production import requirements and interfaces are unchanged.

## Impact

Changes are limited to `crates/muse-e2e`, development guidance, Cargo's dependency lock and OpenSpec context. A standard Gherkin parser replaces the E2E package's TOML/Serde dependencies; production dependencies and user libraries are unaffected.
