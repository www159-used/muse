## ADDED Requirements

### Requirement: Collision groups preserve exact candidate identities

Browsing SHALL group candidates according to NetEase identity and real tagged-name collision rules without discarding their individual identities. Groups SHALL expose candidate counts, and details SHALL expose stable candidate IDs and available format, album, duration, platform identity, and source information. Unknown quality/provenance SHALL remain unknown.

#### Scenario: List a collision
- **WHEN** a display group contains three playable alternatives
- **THEN** the list shows its candidate count and details distinguish all three by stable IDs and available metadata

### Requirement: Confirm a candidate before changing playback

Interactive group playback SHALL play a single candidate directly and offer a chooser when multiple candidates exist. The chooser SHALL support movement, explicit confirmation, and cancellation. Opening, browsing, or cancelling it SHALL NOT issue a load or stop operation. Only the confirmed exact candidate ID SHALL be sent to playback.

#### Scenario: Cancel while another song plays
- **WHEN** the user opens a multiple-candidate chooser and presses Esc
- **THEN** the chooser closes, browsing state is restored, and the previous song continues unchanged

#### Scenario: Confirm an alternative
- **WHEN** the user confirms a displayed candidate
- **THEN** the service loads that exact ID and retains existing load-failure behavior if it cannot be loaded

#### Scenario: Selection becomes stale
- **WHEN** a chooser's membership changes or its selected candidate disappears before confirmation
- **THEN** the choices are refreshed or the absence is reported, with no automatic substitution or playback until a fresh valid confirmation

### Requirement: Search and navigation remain meaningful with groups

Numbered movement SHALL refer to displayed group rows. Search SHALL match metadata across all candidates, including alternatives whose metadata differs from the group representative. Exact candidate IDs SHALL NOT be interpreted as row numbers.

#### Scenario: Search matches only an alternate album
- **WHEN** the user's query matches an album on a non-representative candidate
- **THEN** the containing group is a search match and its candidate details expose that match without starting playback

### Requirement: One-shot playback does not prompt for ambiguity

An exact candidate-ID command SHALL retain current direct-play semantics. A noninteractive request for a multiple-candidate group SHALL return nonzero with candidate identifiers and selection guidance, without changing playback or waiting for input.

#### Scenario: Ambiguous CLI play
- **WHEN** a one-shot CLI command requests a group with multiple candidates
- **THEN** it lists the available candidates and exits unsuccessfully until the user supplies an exact candidate ID
