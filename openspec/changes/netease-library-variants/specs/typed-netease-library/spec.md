## ADDED Requirements

### Requirement: Library instances have explicit source types

Each library SHALL persist a name and source type. The selected library SHALL determine import and grouping rules, and its name/type SHALL be visible in TUI and command mode and queryable from CLI. NetEase SHALL be the first supported type; unsupported types SHALL fail without reinterpreting stored content.

#### Scenario: Reopen a named library
- **WHEN** the user reopens a NetEase-type library through another client
- **THEN** the same stored name/type and NetEase rules apply independently of its filename or the imported files' extensions

#### Scenario: Unknown library type
- **WHEN** a client encounters a stored type it does not support
- **THEN** it reports that type as unsupported and does not change the library

### Requirement: NetEase extension owns local source interpretation

The NetEase extension SHALL own NCM parsing, NetEase metadata/ID interpretation, and local attachment association while accepting valid NCM, MP3, and FLAC sources. It SHALL provide unpacked owned audio for storage and playback without invoking a remote API. Shared storage and playback interfaces SHALL remain independent of NCM container interpretation.

#### Scenario: Import a mixed download folder
- **WHEN** a NetEase library imports a folder containing supported NCM and ordinary MP3/FLAC files
- **THEN** the extension reads each supported source, preserves original files, stores playable MP3/FLAC, and associates locally available metadata and attachments under its rules

#### Scenario: Ordinary audio lacks a platform ID
- **WHEN** a valid MP3 without a NetEase ID is imported into a NetEase library
- **THEN** it remains usable without inventing an ID or requiring an account or network request
