## ADDED Requirements

### Requirement: Import path completion

Tab in an :import path argument SHALL complete local directories and MP3/FLAC/NCM files. Completion SHALL preserve relative, absolute, home-relative, spaced, quoted and Unicode paths without shell execution. A unique directory SHALL end with a path separator. No match SHALL leave text intact; filesystem scan errors SHALL keep input editable and report an English message.

#### Scenario: Complete a directory and descend
- **WHEN** a path prefix uniquely matches a directory and Tab is pressed
- **THEN** its remaining name and separator are inserted and a later Tab can complete its children

#### Scenario: Complete and import a quoted Unicode path
- **WHEN** a quoted or spaced Unicode path is completed and submitted
- **THEN** its original characters are preserved and the existing importer receives the intended path

#### Scenario: No match or failed scan
- **WHEN** no candidate matches or the parent directory cannot be read
- **THEN** command text is retained and the user can continue editing or cancel, with an English error for the failed scan

### Requirement: Multiple completion candidates

Multiple candidates SHALL first extend the common prefix and appear in a bounded popup. Repeated Tab SHALL cycle forward and Shift-Tab SHALL cycle backward, wrapping at both ends. Typing, deletion and cursor movement SHALL invalidate the candidate session. Completion SHALL not toggle results focus.

#### Scenario: Select among matching files
- **WHEN** several audio paths match and the user presses Tab repeatedly or Shift-Tab
- **THEN** candidates are visible, selection moves in the requested direction and the chosen path remains ready for submission

### Requirement: Visible interaction mode

The bottom-left badge SHALL show NORMAL during Vim-style navigation/playback controls and COMMAND during colon command entry. INSERT SHALL be reserved for ordinary UI text fields and SHALL NOT describe colon command entry. Command entry SHALL display its text cursor and allow Unicode-safe editing of completed quoted paths. Enter SHALL submit and Esc SHALL cancel, returning the badge to NORMAL. The removed shortcut footer SHALL remain absent. Tab in NORMAL SHALL retain results focus switching.

#### Scenario: Enter and leave text entry
- **WHEN** colon begins input followed by submission or cancellation
- **THEN** the badge changes from NORMAL to COMMAND and returns to NORMAL, without displaying the old shortcut footer

#### Scenario: Edit inside a completed quoted path
- **WHEN** the user types or deletes after completion adds a closing quote
- **THEN** edits occur at the visible cursor inside the quote and the resulting path remains valid UTF-8
