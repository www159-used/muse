## ADDED Requirements

### Requirement: Import cases read as ordered stories

The E2E package SHALL express import workflows in English `.feature` files with Given setup, When actions and Then expectations in execution order. Files and tracks SHALL use readable tables, and literal multiline text SHALL use docstrings.

#### Scenario: Moved download directory

- **WHEN** a contributor reads a workflow that imports, moves a folder, reopens the library and imports again
- **THEN** the file presents those actions in order with explicit expected reports and library contents
- **AND** the reader does not need to interpret nested configuration fields

### Requirement: Story loading fails on incomplete or unsupported behavior

The runner SHALL reject unknown steps, invalid or duplicate table fields, unsupported Gherkin constructs and missing required outcomes. Each import SHALL declare added, duplicate and failure counts, a known or unknown source total and complete library contents. Action failures SHALL identify the story file and action line.

#### Scenario: Misspelled assertion

- **WHEN** a contributor misspells an assertion or omits required library contents
- **THEN** the case fails visibly before any import side effects
- **AND** the diagnostic identifies the relevant story line

### Requirement: Existing import coverage remains independent

All 39 existing import workflows SHALL retain their setup, actions and expected public-interface outcomes. The test-only workspace package SHALL retain real isolated files and SQLite, golden audio/attachment checks, source preservation, progress checks, independent discovery/filtering and production dependency separation.

#### Scenario: Added regression story

- **WHEN** a contributor adds a supported `.feature` file under the scenario tree
- **THEN** Cargo lists and executes that story independently without Rust registration edits
- **AND** each execution uses a private source tree and SQLite library
