## Why

The library stores complete imported audio, but the existing playback trait alone cannot play it. The user has requested a built-in backend and terminal playback controls. They also require every package, including the binary, to live under crates, with English application messages and errors.

## What Changes

- Add the optional native feature, NativePlayback and standard error types; enable it in the application.
- Decode owned MP3/FLAC sources with bounded PCM buffering, prepare replacement audio before switching, and implement pause, stop, replay and time seeking.
- Report natural completion separately from persistent failures; invalidate obsolete audio sessions and reject stale application controls.
- Read library audio and execute controls in the background. Provide Enter, Space, Left/Right, :stop and status/progress; retain the current track at natural completion.
- Install Linux audio build dependencies and check both the standard-library interface and real decoding in CI.
- Move the application to crates/muse, make the root a virtual workspace, update release/version paths, and use English help, interface labels, reports and errors.

## Capabilities

### New Capabilities

- `native-playback`: Built-in MP3/FLAC decoding, device output, source ownership, seeking, failures and cleanup.
- `terminal-playback`: Background controls, confirmed track identity, progress and shutdown.
- `workspace-conventions`: All packages under crates, a virtual workspace root, application release paths and English messages.

### Modified Capabilities

None. Existing specifications remain in unarchived changes. This change preserves the declared Playback interface.

## Impact

The change affects muse-playback, application workers, Cargo dependencies and lockfile, CI, release scripts and documentation. Symphonia decodes audio; Rodio manages output. Linux builds need ALSA development libraries. SQLite schema and import formats stay compatible. Playback needs no external player or network service. Automatic advancement, queues, volume and device selection remain future work. Builds alone do not establish physical audio output on each platform.
