## ADDED Requirements

### Requirement: NCM-only application sources

The application's :import command SHALL accept a single NCM file or recursively select only NCM files from a directory, matching extensions case-insensitively. Ordinary MP3/FLAC files in a folder SHALL be excluded before audio reads and SHALL not create songs, audio failures or duplicate outcomes. A directly requested non-NCM file SHALL report an English source failure. Scanning SHALL retain existing symlink boundaries.

#### Scenario: Mixed download folder
- **WHEN** a folder contains NCM sources alongside ordinary MP3/FLAC files
- **THEN** the application processes only NCM sources and retains their unpacked MP3/FLAC as stored audio

#### Scenario: Single ordinary file
- **WHEN** a user requests a single MP3 or FLAC source through :import
- **THEN** the application reports that only NCM source files can be imported and creates no new song

### Requirement: Progress for selected sources

Folder discovery and processing totals SHALL count selected NCM files only. Skipped ordinary files SHALL not advance processed outcomes. Selected-file errors, cancellation, storage failures, transaction-based counters and final reports SHALL retain their existing behavior. A folder without NCM files SHALL complete with zero processed audio and a zero total.

#### Scenario: Folder without NCM
- **WHEN** a folder contains only ordinary MP3/FLAC sources
- **THEN** the import reports zero audio work without falling back to those files

#### Scenario: Invalid NCM among valid sources
- **WHEN** one selected NCM fails to unpack while other selected NCM files are valid
- **THEN** the failure is counted, other files continue and progress excludes ordinary audio in the same folder

### Requirement: Completion and guidance match source selection

:import completion SHALL offer directories and NCM files only. It SHALL preserve quoted, spaced, home-relative and Unicode paths, common-prefix expansion and bidirectional candidate cycling. English help and empty-library guidance SHALL identify NCM import sources. NORMAL/COMMAND keyboard modes SHALL remain independent of import stages and stored audio formats.

#### Scenario: Complete from a mixed folder
- **WHEN** matching NCM, MP3 and FLAC names coexist and the user presses Tab
- **THEN** only directories and NCM files participate in completion

### Requirement: Complete Hangul syllables in progress paths

The displayed current filename and source label SHALL compose modern decomposed Hangul sequences into complete syllables. The progress snapshot and all filesystem access SHALL retain the original path. Other text SHALL be preserved.

#### Scenario: Decomposed Korean filename
- **WHEN** a selected NCM filename contains decomposed leading, vowel and trailing Hangul characters
- **THEN** import progress displays the complete Korean syllables and continues reading the original filesystem path

### Requirement: Preserve NCM content and associated data

Selected NCM imports SHALL retain the existing source snapshot, unpacking, complete audio hashes, atomic song/attachment/source commits, duplicate handling and attachment priorities. Original source files SHALL remain unchanged. This source selection SHALL not delete or merge existing library records.

#### Scenario: Import and repeat an NCM
- **WHEN** a valid NCM and its associated lyrics are imported and then imported again
- **THEN** unpacked audio and attachments remain available, the repeated source is skipped as a duplicate, and original files retain their contents
