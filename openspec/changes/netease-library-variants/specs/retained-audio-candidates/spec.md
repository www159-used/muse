## ADDED Requirements

### Requirement: Retain distinct content and platform identities

Imports SHALL retain different encoded audio despite matching title/artist tags. Identical stored audio bytes SHALL share a blob. Different explicit NetEase IDs SHALL remain distinguishable even when their blobs are identical; ID-less sources SHALL NOT cause conflicting platform identities to merge. Reimporting a verified source SHALL be idempotent.

#### Scenario: One platform song has multiple encodings
- **WHEN** MP3 and FLAC audio with the same NetEase ID have different stored bytes
- **THEN** both are retained as playable candidates with their own metadata

#### Scenario: Matching tags have different platform IDs
- **WHEN** two sources have equal normalized title/artist but different NetEase IDs
- **THEN** both identities and audio relationships remain available for explicit selection

#### Scenario: Identical audio has conflicting identities
- **WHEN** sources with different explicit NetEase IDs contain exactly the same decoded encoded-audio bytes
- **THEN** storage keeps one audio blob with distinct candidate identities and source observations

#### Scenario: Exact repeat
- **WHEN** the same verified source is imported again
- **THEN** no additional candidate or audio blob is created and permitted missing-attachment supplementation remains possible

### Requirement: Provenance and attachments remain candidate-specific

Candidates SHALL retain their own metadata and attachment associations. Source observations SHALL preserve available source labels, container formats, platform IDs, and actual audio hashes. A title/artist collision alone SHALL NOT transfer attachments between different candidates. New alternatives SHALL be reported as retained additions rather than skipped duplicates.

#### Scenario: Studio and live recordings collide by name
- **WHEN** matching names have different audio, albums, durations, or local lyrics
- **THEN** each recording retains its own data and the importer reports the additional candidate

#### Scenario: Source files are removed after import
- **WHEN** an imported source is no longer available
- **THEN** its stored candidate still plays from library-owned audio and its known source label remains descriptive only

### Requirement: Migration preserves owned data and enables reimport recovery

Upgrade SHALL be explicit and offline, refuse an active playback service, and complete a verified no-overwrite backup before schema writes. It SHALL preserve candidate IDs, audio, and existing attachments, and mark legacy source mappings for revalidation. Read commands SHALL NOT silently upgrade libraries. Backup and restore SHALL handle supported old and new layouts without modifying the source while copying.

#### Scenario: Upgrade cannot create its backup
- **WHEN** backup creation or validation fails
- **THEN** the original schema and content remain unchanged and upgrade reports failure

#### Scenario: Reimport a historically suppressed alternative
- **WHEN** a legacy source hash points at a different retained track and the original source is reimported after upgrade
- **THEN** the importer rereads and decodes the source, preserves its actual audio as a candidate, repairs the source association atomically, and creates no further candidate on a subsequent identical reimport

#### Scenario: Recovery fails or the source is unavailable
- **WHEN** a source needed to recover discarded audio cannot be read or decoded
- **THEN** existing stored audio and IDs remain intact, the source remains eligible for retry, and no recovered candidate is claimed

#### Scenario: Restore an old backup
- **WHEN** the user restores a valid version-1 backup to a new path
- **THEN** the copy retains that supported schema and requires explicit upgrade before new-version library operations
