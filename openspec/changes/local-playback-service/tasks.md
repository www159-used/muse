## 1. Local service lifecycle

- [x] 1.1 Verify transport, private runtime storage, ownership locking, and terminal-independent process startup on macOS/Linux/Windows with the pinned toolchain; record the selected dependencies and any platform limits in design.md.
- [x] 1.2 Implement canonical library identity, exclusive service ownership, bounded connect/start, authenticated version handshake, and stale-record recovery; verify concurrent launches, equivalent paths, separate libraries, and incompatible peers.
- [x] 1.3 Add the internal service entry point and explicit shutdown; verify with real processes that client exit does not kill the service and successful shutdown releases ownership and endpoint resources.

## 2. Shared playback session

- [x] 2.1 Move application playback ownership into the service, retaining native backend behavior and adding typed commands, request completion, and snapshots; verify load failures, explicit pause/resume, generation checks, bounded queues, and serialized quit-stop after pending loads.
- [x] 2.2 Add the background client adapter and bounded status polling; verify multiple clients observe the same state, disconnects produce useful errors, unknown mutations are not replayed, and device failure leaves browsing/import available.

## 3. Noninteractive CLI

- [x] 3.1 Add play/pause/stop/status/shutdown parsing and English output with stable database track IDs, completion-based exit codes, and existing --library behavior; verify real non-TTY invocations against the shared service.
- [x] 3.2 Implement no-service and invalid-argument semantics without accidental database creation or service startup; verify help/version/backup/restore regressions.

## 4. Terminal attach and detach

- [x] 4.1 Replace UI-owned playback with the service client and hydrate snapshots on reconnect; verify reattachment preserves track, progress, and paused state without loading again.
- [x] 4.2 Add :detach parsing and completion with explicit exit intent; preserve stop-on-quit/Ctrl-C/observed hangup, safe import cancellation, and terminal restoration on success and failure.
- [x] 4.3 Add real process and pseudo-terminal workflows for play-detach-status-reattach, paused detach, shared ordinary quit, import during detach, service loss, and shutdown. Use temporary libraries and deterministic playback injection; retain native output coverage separately.

## 5. Documentation and verification

- [x] 5.1 Document CLI syntax, database IDs, shared quit semantics, explicit detach, service shutdown, idle lifetime, crash behavior, and per-library identity; update project context to distinguish implemented behavior from deferred features.
- [x] 5.2 Run make ci and strict OpenSpec validation, verify supported-platform process tests through the available CI workflow, and record results and any unverified physical-device/platform checks without claiming them passed.


## Verification

- `make ci` passed on macOS with Rust 1.95.0: formatting, the standard-library playback interface check, Clippy with denied warnings, 114 ordinary Rust test executions, 13 import scenarios with 109 steps, two documentation tests, and the release build.
- Real service processes and the production CLI verify explicit play/pause/resume/stop/shutdown, idempotence, missing-session behavior without file creation, production automatic-start races, singleton ownership, stale-record recovery, canonical symlink identity, library isolation, failed loads, stale seeks, and stop completion after a pending load.
- Real pseudo-terminal workflows verify completion without execution, playing and paused detach, reattachment without a new track generation, progress continuity, stop-on-quit, terminal hangup, forced client loss, import transaction cleanup, and service loss followed by detach. Existing navigation, search, mouse restoration, and maintenance workflows pass.
- An initial terminal regression caught ordinary quit being mistaken for detach when the client was waiting on its queue. The exit intent is now checked again after that wait; the process-level quit and hangup tests pass.
- Protocol checks reject incorrect credentials, incompatible versions, and oversized frames, bound incomplete frames by a total deadline, and report lost responses without replaying mutations.
- The production service modules passed isolated cross-compilation for `x86_64-unknown-linux-musl` and `x86_64-pc-windows-gnu`, with only the audio worker replaced by a stub in the external compile probe. This checks transport, runtime/ACL API usage, locking API usage, and process startup code; it is not a full application build or runtime test on those hosts.
- The existing GitHub Actions matrix includes the new process tests through `cargo test --workspace --locked --all-targets --all-features`; pseudo-terminal tests run on Unix. No Git remote is configured, so Linux/Windows CI jobs were not triggered here. Their actual process/ACL/audio behavior and physical audio output remain unverified in this session.
- `openspec validate --all --strict` passed all fifteen changes. Test playback injection exists only in test executables; production always uses NativePlayback. All executed import/playback workflows used temporary libraries and generated repository fixtures; no personal library or music was changed.
