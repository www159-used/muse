## ADDED Requirements

### Requirement: Viewport-aware paging

In NORMAL mode, Ctrl-d/u SHALL move down/up by half the focused panel's visible row span, rounded down, and Ctrl-f/b or PageDown/PageUp SHALL move by that span minus one row, retaining one row of overlap. The song span SHALL exclude nonexistent rows in short libraries. Distances SHALL be at least one row. A numeric prefix SHALL multiply the page distance, with zero treated as one count and oversized counts handled without overflow. Song paging SHALL move the selection first; its viewport SHALL follow only when needed to reveal the selection. Results paging SHALL change vertical scroll only. Paging SHALL clamp at both ends, adapt after resizing and remain safe in empty or tiny views.

#### Scenario: Page and resize the song list
- **WHEN** the song list has twelve visible rows and Ctrl-d is pressed
- **THEN** selection advances six rows and the viewport stays in place if that row is already visible
- **AND** after resizing to five visible rows, Ctrl-d advances two rows

#### Scenario: Retain overlap and consume a page count
- **WHEN** a twelve-row song list starts at the first row and Ctrl-f is pressed
- **THEN** the twelfth song is selected without moving the viewport
- **AND** typing 2 followed by Ctrl-f advances twenty-two songs and clears the count

#### Scenario: Page import results
- **WHEN** import results have focus and Ctrl-f is pressed
- **THEN** the results scroll down by their visible height minus one row without changing the selected song
- **AND** further paging stops at the last viewport

### Requirement: Selection and camera alignment

Keyboard song movement and jumps SHALL preserve the current viewport when the target is already visible, otherwise move the viewport only far enough to reveal that target. Resizing and library refresh SHALL retain a valid selection and clamp the viewport to valid full-page bounds. Repainting an unchanged view SHALL not move the selection or camera.

#### Scenario: Move within a scrolled viewport
- **WHEN** songs 12 through 23 are visible and song 23 is selected
- **THEN** k selects song 22 without changing the viewport
- **AND** jumping to visible song 15 keeps the viewport in place
- **AND** jumping to song 5 places song 5 at the top of the viewport

### Requirement: Mouse wheel scrolling

The terminal SHALL enable mouse capture while muse runs and disable it when restoring the terminal. In NORMAL mode, a vertical wheel event over a panel's inner body SHALL scroll three rows in that direction, bounded by the first and last full viewport. Song scrolling SHALL move selection by the camera's actual movement so its screen row remains stable; when the camera cannot move, selection SHALL also stop. Scrolling back and forth where both moves fit SHALL restore selection and viewport. Results scrolling SHALL affect only report text under the pointer, retaining keyboard focus and song selection. COMMAND mode, import progress, borders, other screen regions and non-wheel mouse events SHALL ignore the wheel/navigation action. Accepted wheel input SHALL clear unfinished navigation prefixes. Empty, short and zero-height views SHALL remain safe.

#### Scenario: Scroll the list beneath a stable selection
- **WHEN** songs 12 through 23 are visible with song 23 selected and the wheel scrolls down over the song body
- **THEN** songs 15 through 26 are visible and song 26 is selected on the same screen row
- **AND** scrolling back up restores the original selection and viewport

#### Scenario: Stop selection at the viewport boundary
- **WHEN** a wheel movement has only one row left before the last full viewport
- **THEN** the camera and selected song advance exactly one row
- **AND** further downward wheel input changes neither position

#### Scenario: Scroll the panel under the pointer
- **WHEN** the song list has keyboard focus and the wheel scrolls over completed import results
- **THEN** results scroll three rows while song selection, song viewport and keyboard focus remain unchanged

### Requirement: Absolute song line numbers

Each displayed song SHALL have an aligned line number starting at 1 based on current list order. Numbers SHALL remain absolute when scrolled, leave room for the selection and playing markers, and remain safe in narrow terminals.

#### Scenario: Scroll past the first page
- **WHEN** the library is scrolled to show its twenty-third song
- **THEN** that song displays line number 23, rather than a viewport-relative number or database ID

### Requirement: Direct line and endpoint jumps

In NORMAL mode, gg/G without a numeric prefix SHALL jump to the focused panel's beginning/end. A decimal prefix followed by G and a numeric colon command such as :23 SHALL select that numbered song and focus the song list. Numbered targets SHALL clamp to the first or last song, stay visible and handle oversized decimal input without overflow. Empty libraries SHALL keep no selection. Numeric prefixes SHALL also support counted j/k movement.

#### Scenario: Jump to a displayed line
- **WHEN** a user types 23G or submits :23
- **THEN** the twenty-third song is selected, visible and displayed as line 23

#### Scenario: Clamp an oversized jump
- **WHEN** a numeric target exceeds the library's song count or integer range
- **THEN** the last song is selected without an overflow or panic

### Requirement: Navigation input respects modes

Navigation sequences SHALL apply only in NORMAL mode. COMMAND mode SHALL retain ordinary editing, completion, submission and cancellation. Esc, unrelated keys and focus/mode changes SHALL clear incomplete numeric or g prefixes. Released keys and unrelated modified characters SHALL not execute ordinary navigation. Paging and jumps SHALL not start playback or change imports. The removed shortcut footer SHALL remain absent.

#### Scenario: Edit a command after a partial jump
- **WHEN** a user starts a numeric or g prefix, enters COMMAND mode, then types or cancels a command
- **THEN** text remains editable and navigation does not consume command characters or reuse the previous prefix
