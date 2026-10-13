## ADDED Requirements

### Requirement: Persistent interactive command mode

`muse command` SHALL provide an interactive prompt for the selected library with help, in-session history, command/path completion, quoted Unicode paths, and repeated command execution. It SHALL use shared application operations and SHALL NOT execute input through a shell. Default `muse` SHALL continue opening the TUI.

#### Scenario: List and play in one session
- **WHEN** a user lists songs, requests an ambiguous group, and selects a displayed candidate in command mode
- **THEN** the selected candidate plays through the shared service and the prompt remains available for subsequent commands

#### Scenario: Exit command mode
- **WHEN** the user invokes detach
- **THEN** the prompt exits with playback preserved and active import work safely cancelled between song transactions

#### Scenario: Ordinary command-mode quit
- **WHEN** the user invokes quit/q, handled Ctrl-C, or EOF
- **THEN** the client requests shared playback stop, reports unconfirmed failures honestly, safely finishes any current import transaction, and exits

### Requirement: Query and import the current typed library

CLI and command mode SHALL expose list, search filtering, candidate/group details, import, and library information through the selected library type. One-shot list/show SHALL support structured JSON output with stable candidate IDs. Read commands SHALL neither create missing libraries nor start playback services. Import SHALL apply the same rules as TUI import.

#### Scenario: Script lists alternatives
- **WHEN** a script requests candidate listing or group details as JSON
- **THEN** stdout contains structured data with exact candidate IDs, while diagnostics use stderr and no interactive prompt is opened

#### Scenario: Import from command mode
- **WHEN** the user imports a quoted folder containing NCM and MP3/FLAC
- **THEN** the NetEase importer preserves candidates, reports additions/duplicates/failures, and leaves the prompt usable after completion

#### Scenario: Missing library query
- **WHEN** list, show, or library info targets a nonexistent library
- **THEN** it fails clearly without creating a database or launching a playback service
