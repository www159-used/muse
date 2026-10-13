## ADDED Requirements

### Requirement: Mixed download sources

The application SHALL import a single MP3, FLAC or NCM source and recursively discover those extensions case-insensitively in folders. Completion, English guidance and progress totals SHALL reflect all three formats. Unsupported files SHALL remain excluded from folder audio outcomes; directly requested unsupported files SHALL report an English source failure. Folder scanning SHALL not follow external symlinks.

#### Scenario: Ordinary downloads alongside NCM
- **WHEN** a folder contains distinct valid MP3, FLAC and NCM downloads
- **THEN** all three are considered for import and progress counts each selected source

### Requirement: Tagged song identity

Download imports SHALL additionally match a nonempty tagged title and artist against persisted library metadata after trimming surrounding whitespace and Unicode lowercasing. Filenames SHALL not participate; artist differences SHALL keep distinct songs unless complete-byte deduplication already identifies the same content. Incoming missing title or artist SHALL disable metadata matching and retain content matching. General-purpose and explicit NCM-only APIs SHALL not add metadata-based lookup.

#### Scenario: Different files of one song
- **WHEN** two downloaded files have different names or encoded audio but the same tagged title and artist
- **THEN** the download importer retains one song and reports the later source as a duplicate

#### Scenario: Same title with different artists
- **WHEN** two byte-distinct files share a title but have different artists
- **THEN** both are added as separate songs

#### Scenario: Missing tags
- **WHEN** two byte-distinct files have the same filename fallback but lack a tagged title or artist
- **THEN** filename equality does not merge them

### Requirement: Atomic retained song and duplicate records

Metadata identity lookup SHALL occur inside the same immediate transaction as retained-song selection, new source records and missing attachment writes. Name duplicates SHALL preserve the existing complete audio and metadata, supplement only missing attachments and increment duplicate outcomes only after commit. Matching SHALL remain effective after restarting or restoring the library. Existing duplicate tracks SHALL not be deleted or merged. Removing a retained track SHALL remove its successful source associations and permit re-import.

#### Scenario: Import an NCM after an ordinary tagged file
- **WHEN** a tagged MP3 is imported, the library is reopened and an NCM with the same title and artist is imported
- **THEN** the original MP3 remains stored, the NCM is counted as a duplicate and a missing cover can be added

### Requirement: Deterministic fresh import and failure recovery

Fresh folder download imports SHALL attempt NCM before ordinary formats, retaining deterministic discovery order within those groups. Existing entries SHALL take precedence. Invalid sources SHALL not reserve an identity, and source failures, cancellation and storage failures SHALL retain the established per-file transaction and progress contracts.

#### Scenario: Preferred source is invalid
- **WHEN** a folder contains an invalid NCM and a valid ordinary source
- **THEN** the invalid file fails and the valid file can still be added without an incomplete duplicate record
