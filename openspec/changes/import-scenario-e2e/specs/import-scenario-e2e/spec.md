## ADDED Requirements

### Requirement: Independent import scenario package

The workspace SHALL provide a separate non-publishable `muse-e2e` package under `crates`. It SHALL exercise real source files and SQLite through public `muse-library` interfaces without requiring production test hooks or a runtime dependency on the scenario runner.

#### Scenario: Run independently
- **WHEN** a contributor runs the focused E2E Cargo or Make command
- **THEN** only the import scenario package runs, without needing an audio device, personal download directory, network service or interactive terminal

### Requirement: Readable and independently reported cases

Each scenario SHALL be a readable declarative file describing initial files, ordered operations and observable expected outcomes. Files SHALL be automatically discovered in deterministic order and individually listed/filterable. Invalid syntax, unknown fields, missing expectations, duplicate names and an empty suite SHALL fail visibly rather than silently skip coverage.

#### Scenario: Add a layout case
- **WHEN** a contributor adds a scenario using existing operations and fixtures
- **THEN** it becomes a separately reported Cargo test without modifying Rust registration code

### Requirement: Isolate and verify import side effects

Every case SHALL use its own temporary source tree and library. The runner SHALL check exact expected report counters, final progress, persistent track contents and golden owned audio through public interfaces, and SHALL verify source preservation during import. It SHALL support metadata and attachment assertions and deterministic cancellation at progress checkpoints. Scenario and fixture paths SHALL remain inside their respective roots.

#### Scenario: Repeat import after restart
- **WHEN** a case imports NCM audio, reopens the library and imports a renamed copy
- **THEN** its expected report records a duplicate, the library still has one playable track and original files remain unchanged

#### Scenario: Wrong expectation
- **WHEN** a case expects a wrong report count or wrong library content
- **THEN** the case fails with its filename, step and mismatched outcome

### Requirement: Maintain an import compatibility corpus

Cases SHALL cover NCM-only file/folder selection, recursive and changed directory layouts, duplicate/repeat imports, damaged inputs, persistence after source removal, metadata fallback/evolution, attachment association and deterministic cancellation. Equivalent high-level tests SHALL move into the corpus while focused storage and race tests remain at their existing seam. Workspace CI SHALL run the corpus.

#### Scenario: New metadata shape
- **WHEN** a fixture contains additional or missing NetEase metadata fields
- **THEN** a readable case asserts preserved audio and the intended supported metadata or fallback

#### Scenario: Cancellation followed by retry
- **WHEN** a case cancels after one committed file and then imports the folder again
- **THEN** committed audio survives, the retry deduplicates it and expected counters account for the remaining sources
