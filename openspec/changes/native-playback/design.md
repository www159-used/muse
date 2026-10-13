## Context

The user has authorized the native backend after the earlier trait-only phase. Playback retains its six methods and state contract. The library already returns complete owned MP3/FLAC readers independent of its connection; NCM is unpacked during import. All packages now live under crates and application messages use English.

## Goals / Non-Goals

**Goals:** Real decoding and default-device output, pause/stop/replay, time seeking, persistent failures, background terminal controls and explicit cleanup.

**Non-Goals:** Automatic advancement, queues, network inputs, volume, device selection and schema changes.

## Decisions

### Implemented: Symphonia 0.5.5 decoding and Rodio 0.22.2 output

A dedicated Symphonia MP3/FLAC path preserves read and decoder errors. Rodio enables only its playback feature and handles device output, format conversion and mixing. Its generic decoder is not used because runtime decoding errors must remain observable. The playback crate's native feature is optional; muse enables it. The interface builds with the standard library alone.

### Implemented: One backend worker with bounded PCM

Synchronous NativePlayback methods submit commands to the thread that owns the decoder and output device, then wait for completion. Status reads a short shared snapshot. Commands have priority over decoding. The PCM queue holds about 250 ms of music, plus one private decoded packet. Paused and Ready sources may prefetch up to this limit.

The decoder publishes complete frames to a fixed-capacity rtrb 0.4.0 single-producer/single-consumer ring buffer. The output Source reads complete frames without taking the status mutex, reading files, allocating or waiting. Playback state, submitted frame count and EOF use atomics; the status mutex retains timeline metadata and persistent failures. Actual starvation produces silence without advancing music position. Status reads and decoder publication cannot substitute silence for already queued PCM. EOF is observed before inspecting the queue so the final publication cannot be mistaken for a drained stream. Successful loading, rebuilding or seeking creates a new output session and invalidates the old Source. Obsolete PCM and EOF cannot update a new session. Position counts submitted music frames. Finished requires confirmed EOF and a drained queue. Device buffering and conversion may retain a short tail of previously submitted audio.

Playback state occupies the low three bits of one atomic word and submitted frames occupy the remaining bits. Reserving a frame increments its count only while that same word still says Playing. Pause changes state while preserving the count, so it cannot return before a previously authorized frame advances the reported position. A chunk rejected after a concurrent pause remains queued. Completion also changes only a Playing state, preserving a concurrent pause and its output source.

### Implemented: Prepare before replacing, with persistent errors

Loading probes a candidate, checks MP3/FLAC and decodes its first valid block before replacing the old source and identity. An invalid candidate leaves a healthy old source and playback intent intact. A private mutex around the reader satisfies the decoder's Sync requirement; public AudioSource still requires only Send.

Input, decode, device and worker failures stop further music submission and remain visible through status. A successful load or stop rebuild can recover input, decode or device faults; a terminated worker requires constructing a new backend. Stop retains the source and returns it to zero. Errors preserve their stage and original cause.

Complete FLAC decoding checks the decoder's final integrity result and declared sample count. Seeking resets decoding and disables whole-stream checksum verification because earlier samples are intentionally skipped. EOF handling remains limited by the format reader's ability to distinguish truncation; it does not promise detection of every damaged file.

### Implemented: Seeking returns the actual position

Absolute and execution-time relative targets use saturating arithmetic and clamp to a reliable known duration. Seeking invalidates old output, resets decoding, discards samples to the target and returns the actual landing point. Seeking to zero rebuilds from the owned input; seeking to a known end completes immediately. Playing continues, Paused remains paused, nonzero Ready targets become Paused, and Finished targets inside the track become Paused. Prevalidation rejection preserves the old session; failure after seeking starts becomes persistent.

### Implemented: Background application control

The terminal's worker opens library audio and calls synchronous backend methods. The audio device opens lazily on the first load, allowing browsing, import and backup without it. Only one application load is pending at a time. Loading success updates current track identity before attempting play, so a later play failure cannot misidentify the loaded source. Controls carry the captured track generation and reject stale requests, including a reload of the same track.

Enter plays the selection, Space pauses/resumes, Left/Right seeks ten seconds, and :stop retains the source at zero. Natural completion retains the current track without advancement. Quit releases playback before waiting for the current import transaction to commit or roll back.

### Implemented: Workspace layout and English messages

The root is a virtual workspace with crates/muse as its default member. Application source and integration tests live there; the library and playback crates remain siblings. Shared target paths remain at the root. Release validation and packaging read crates/muse/Cargo.toml. cargo-release selects muse only, and its per-crate hook runs make -C ../.. ci. Internal crates do not release independently.

Help, labels, import reports and application/library errors use English. Original metadata, paths and Unicode regression fixtures retain their content.

## Verification

- make ci passes: format, interface-only build, Clippy with warnings denied, 44 ordinary tests, two doctests and the release build.
- Backend tests decode real MP3/FLAC and consume PCM through Rodio's mixer without a physical device. They cover bounded buffering, pause, seek boundaries, replay, obsolete sessions, failed replacements, persistent input/decode/device/worker faults and cleanup. An owned Send reader containing Cell confirms that Sync is not part of the public requirement.
- A regression consumes a queued six-channel frame while the status mutex is held, checks every sample and the submitted position, then confirms EOF only after the frame drains. The previous try_lock output replaced that frame with silence.
- A concurrent output/control regression performs 10,000 play/pause cycles and verifies that submitted position remains fixed after every pause. Separate state and frame-count atomics reproduced a late frame increment; the combined transport word prevents it.
- A local six-channel, 16-bit, 44.1 kHz FLAC decoded byte-for-byte identically to macOS Core Audio over the full track. A three-second paced mixer capture matched all 264,600 reference stereo samples after the ring-buffer change; the previous callback inserted additional silence during lock contention. This verifies PCM output before the physical device.
- Both optional physical default-device tests passed on this macOS machine: backend playback/position/pause/seek/completion/stop, and library playback after source removal with stale generation rejection.
- A real terminal session imported a generated FLAC, removed its source, played from the library, paused, sought both boundaries, stopped/replayed, controlled playback during a 500-file import, and quit successfully. The alternate screen was restored and the resulting database passed integrity_check.
- Root cargo run --locked -- --help selects muse and prints English help. CLI regressions cover English errors and Unicode paths.
- Packaging in a temporary workspace verified the application version, binary, licenses, README, SHA-256 checksum and mismatched-tag rejection. cargo-release's actual CI hook and app-only v0.1.1 preview passed in a temporary Git repository with spaces in its path. The preview changed neither versions nor Git state.
- Native rustdoc builds with warnings denied. OpenSpec changes pass strict validation.

Linux and Windows workflows are configured with native builds, and Linux installs ALSA development libraries. Their workflows and physical devices have not been run from this session; local macOS checks do not establish those results.

## Risks / Trade-offs

- Arbitrary custom Read implementations can block forever. Inputs must be stable owned local readers whose operations eventually return; Drop does not claim bounded cancellation of arbitrary readers.
- Decoder prefetch and device buffering make submitted music position differ slightly from the instant of audible output.
- Preparing a replacement can exhaust the old queue. Starvation emits silence while retaining playback intent.
- Linux playback requires ALSA at runtime and an available device.
- Library audio still loads an entire track into memory. Large-track memory optimization remains separate work.

## Migration Plan

Enable native dependencies and the application feature, update CI, and connect the backend to the terminal. Move the application and its tests under crates and update release paths. SQLite needs no migration. Disabling native and removing application playback integration returns to import/browsing behavior.

## Open Questions

Additional physical-device checks on Linux/Windows, volume, sustained playback speed changes, queues, device selection and large-track memory optimization remain future work.
