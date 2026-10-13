## Why

Playback currently belongs to the terminal application and stops when that process exits. Users need an explicit way to leave the interface while music continues, then reconnect to the same playback session.

## What Changes

- Add `:detach` to leave the TUI without stopping playback, including command completion. Preserve `:q` and `:quit` as exit-and-stop operations.
- Move playback ownership into a local background service shared by TUI and CLI clients. Opening `muse` reconnects to an existing session or starts the service when needed.
- Propose noninteractive `play`, `pause`, `stop`, `status`, and `shutdown` commands for the same library session.
- Keep library browsing, importing, and existing backup/restore paths using the existing library API. Detaching does not introduce background import jobs.
- Define service identity, startup, disconnection, and failure handling without adding remote access or OS service installation.

## Capabilities

### New Capabilities

- `local-playback-session`: Local service ownership, shared playback controls, reconnection, and lifecycle.
- `terminal-detach`: Explicit detach behavior and preserved stop-on-quit behavior.
- `playback-cli`: Noninteractive control of the shared playback session.

### Modified Capabilities

None in `openspec/specs`, which currently contains no main specifications. Related requirements remain in the completed `native-playback`, `playback-interface`, and `builtin-command-completion` changes; this change adds explicit detach and service behavior while preserving their playback and ordinary quit contracts.

## Impact

Changes will touch `crates/muse/src/main.rs`, `command.rs`, `ui.rs`, and the application playback worker, with service/client modules and process integration tests. The native playback library remains responsible for decoding and output; SQLite storage and its format remain unchanged. Local transport and serialization dependencies must support the existing macOS, Linux, and Windows builds.

The user confirmed a new exit-without-stopping operation while retaining ordinary quit. The service layout and CLI details below are proposed implementation choices, not separately confirmed user requirements. This proposal supersedes the earlier no-background-process scope only for local playback.
