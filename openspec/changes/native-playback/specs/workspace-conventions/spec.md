## ADDED Requirements

### Requirement: All packages under crates

The repository root SHALL be a virtual Cargo workspace. The muse binary, library and playback packages SHALL live under crates. Default cargo run and explicit package selection SHALL run muse. Application version checks and packaging SHALL read crates/muse/Cargo.toml; internal libraries SHALL not create independent release tags.

#### Scenario: Build and run from the workspace root
- **WHEN** a developer checks the workspace or requests application help from the root
- **THEN** all packages are discovered under crates and the application runs without requiring a root package

#### Scenario: Package the application
- **WHEN** the release tag matches the application manifest version
- **THEN** packaging uses the binary in the shared target directory and includes root licenses and documentation

### Requirement: English application messages

User-facing help, interface labels, import reports and application/library error messages SHALL use English. Imported metadata, source paths and Unicode regression fixtures SHALL retain their original content.

#### Scenario: Invalid command or input
- **WHEN** a user supplies an invalid argument or malformed local audio input
- **THEN** the program reports the failure in English, retaining the original path where needed
