## Context

`ui::App` owns `playback::Worker`; dropping that worker joins its thread and destroys `NativePlayback`. A thread cannot preserve playback after the TUI process exits. The worker already serializes playback requests, exposes snapshots, and guards operations using track generations. Import is a separate UI-owned worker with cancellation between songs. CLI maintenance currently bypasses the terminal.

Confirmed product decision: keep ordinary quit-and-stop and add an explicit detach operation. `:detach` is the proposed spelling carried forward from the discussion. The choices below were implemented as engineering defaults under the authorized change; they are not separately confirmed product preferences. They replace the earlier no-daemon scope only where needed for playback.

## Goals / Non-Goals

Goals: retain playback across explicit detachment; reconnect without reloading audio; give CLI and TUI one authoritative playback session; retain terminal cleanup, import transaction safety, and existing playback error behavior.

Non-goals: remote streaming/control, login startup, OS service registration, queues, playlists, persisted playback across service restart, background import jobs, and restoring cursor/search state across TUI launches.

## Decisions

### 1. A local service owns playback

Run the existing binary in an internal service mode with no inherited terminal input/output or controlling-terminal dependency. Keep service, client, and protocol modules under `crates/muse` initially. Reuse `muse-playback` without adding transport concepts to its standard-library interface.

CLI and TUI submit typed commands to the service; the service owns the application worker and publishes snapshots. Browsing/import/backup remain direct `muse-library` operations. An in-process shared module alone cannot survive TUI exit; making the TUI itself the server has the same lifetime problem. Moving all library operations into RPC would expand this change unnecessarily.

### 2. One service per user and canonical library path

Resolve the selected library to an absolute canonical path before establishing service identity. Equivalent relative and symlink paths share a session; distinct library paths do not share controls. Keep `./library.muse` and `--library` semantics. Hard-link aliases are outside the initial identity guarantee and must be documented.

Use a user-private runtime directory, a held OS lock for ownership, and an endpoint record containing protocol version, library identity, service instance ID, and connection details. Concurrent startup contenders must converge on the owner. Reclaim stale records only after acquiring ownership; never terminate a PID merely because a record mentions it. TUI startup can create a new library as today; control commands must not create a missing library.

Selected portable transport: loopback TCP on an OS-assigned port, authenticated with an unpredictable per-instance token stored only in the private runtime record. Bind only loopback, authenticate before accepting commands, and cap frames and connection timeouts. The token must not appear in arguments or logs. Unix sockets plus Windows named pipes are an alternative if a suitable existing dependency makes them simpler; the selected implementation uses std TCP and File::try_lock, serde/serde_json, getrandom, sha2, tempfile, rustix setsid on Unix, and windows-sys directory security plus detached process flags on Windows. The runtime record token also identifies the service instance.

Portability verification uses real process tests on this macOS host and isolated compilation of the production service modules against Linux musl and Windows GNU targets, substituting only the audio worker in the compile probe. The existing three-platform CI workflow runs the process integration tests on each host; Unix pseudo-terminal tests remain cfg(unix). Cross-compilation does not verify Windows ACL behavior or Linux/Windows process lifetimes at runtime.

References: [Rust file locks](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock), [Unix process setup](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html), [Windows process flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags), [Windows owner-rights SID](https://learn.microsoft.com/en-us/windows/win32/secauthz/sid-strings).

### 3. Shared commands and authoritative snapshots

Use a versioned, length-bounded request/response protocol with request IDs. Commands include load-and-play by database track ID, explicit play/resume, explicit pause, toggle for the TUI, seek, stop, status, and shutdown. Preserve generation checks for controls targeting a displayed track. Ordinary quit requests a session-level stop, not a stale-track operation, so an already accepted load cannot start music after successful quit.

Snapshots contain current track ID, pending load, state, position, duration, generation, and error. TUI polls snapshots through a background client with bounded waits rather than blocking rendering on IPC. Reattach hydrates from the service without loading the track again. Concurrent commands execute in service order; loading remains limited to one accepted operation at a time. CLI success means the operation completed, not just entered a queue. Do not replay mutations automatically after a response is lost; report an uncertain outcome and allow status inspection.

### 4. Explicit lifecycle operations

| Operation | Playback | TUI | Service |
|---|---|---|---|
| `:detach` | Preserve playing/paused/stopped state | Exit | Remain available |
| `:q`, `:quit`, handled Ctrl-C | Stop shared session | Exit after acknowledgement | Remain available |
| `:stop` / `muse stop` | Stop | Remain open if present | Remain available |
| `muse shutdown` | Stop | Connected clients see disconnection | Exit and release ownership |

Retain stop-on-observed-terminal-hangup for an attached TUI. An uncatchable crash cannot send stop; the service treats unannounced connection loss as detachment and continues playback. This limitation is explicit rather than promising reliable stop on process death. Multiple clients share the session, so ordinary quit in any TUI stops it for everyone. Detach in one client does not affect another.

Stop and shutdown have bounded client waits and honest errors if acknowledgement is unavailable. TUI terminal restoration runs even on errors. Detach requires no playback mutation or stop acknowledgement. Both quit modes cancel an active import and wait for its current song transaction to finish; they do not leave an import running in the service. The service remains available when idle until explicit shutdown or OS session termination; automatic idle expiry is deferred.

### 5. Minimal noninteractive CLI

Proposed syntax: `muse [--library <file>] play [<track-id>]`, `pause`, `stop`, `status`, and `shutdown`. A track ID is the stable database ID, not the numbered TUI row. `play <track-id>` loads that track; bare `play` resumes/starts the loaded track or fails if none exists. `pause` and `stop` are idempotent. `status` prints the current ID, state, position, duration, and any playback error in English.

TUI and `play <track-id>` can start a service automatically. Bare `play` and `pause` require an existing session. With no service, `status` reports that fact successfully, and `stop`/`shutdown` succeed as no-ops. None of these observational/no-op paths spawn a process or initialize a database. Invalid arguments fail before side effects. Existing help/version/backup/restore continue to work without a TTY or service. JSON output and library-list/import CLI commands are deferred.

## Risks / Trade-offs

- [Background process survives its launcher differently on each OS] → Test real child processes and terminal closure on supported platforms; do not inherit terminal handles.
- [Concurrent starts or stale endpoint records create duplicate playback] → Hold ownership for the service lifetime and test race/restart behavior.
- [A shared session makes quit affect other clients] → Document it and keep detach explicit in help/completion.
- [Process crashes leave playback running] → Document the unannounced-disconnect rule and provide noninteractive stop/shutdown.
- [Transport failure makes command completion uncertain] → Bound waits, avoid mutation replay, and expose authoritative status on reconnect.
- [Audio device unavailable in CI] → Use an injected deterministic playback implementation in process tests; retain native decode/output tests and separate physical-device checks.

## Migration Plan

First establish the portable service lifecycle and shared playback protocol with process tests. Add CLI controls, then adapt the TUI and implement detach. Keep storage format, import behavior, and backup/restore unchanged. Update README and project context when behavior is implemented; do not describe the proposed service as existing functionality yet.

Before replacing a binary with an incompatible protocol version, explicitly shut down its service. Refuse incompatible handshakes with an actionable English error rather than killing or silently replacing the running instance. Rollback requires shutting down the service before using the older binary; no database migration is required.

## Open Questions

- Linux and Windows runtime verification must come from their CI hosts; this checkout has no Git remote configured, so those remote jobs cannot be triggered here.
- Proposed defaults above (per-library service, shared stop-on-quit, idle lifetime, and minimal CLI syntax) are not separately user-confirmed; revise if product preferences change. No additional product decision blocks preparing implementation.
