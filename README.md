# muse

A standalone Rust terminal music player with Neovim-style navigation.

Import local NetEase downloads with `:import <audio file or folder>`. Folders are scanned recursively for NCM, MP3 and FLAC sources. Each library has a stored name and source type; NetEase is the first supported type. Imports retain different audio candidates and group matching tagged names for browsing. The SQLite library stores complete audio, metadata and associated local artwork/lyrics, so imported music no longer depends on its original files. NCM is unpacked during import; playback uses the stored MP3/FLAC through Symphonia and Rodio without an external player.

All packages live under `crates/`; the repository root is a virtual Cargo workspace:

```text
crates/
  muse/           CLI, terminal UI and local playback service
  muse-library/   Typed storage, built-in NetEase extension, upgrade and backup
  muse-playback/  Playback traits and optional native backend
  muse-e2e/       Declarative import workflows, runner and synthetic fixtures
```

Application messages, help and errors are in English. Imported metadata and Unicode paths are preserved.

Import progress composes decomposed Korean filenames into complete Hangul syllables for display while retaining the original filesystem paths for reading.

## Usage

```sh
cargo run -p muse -- --library ./library.muse
```

The default library is `./library.muse`, with type `netease` and a filename-derived name that survives renaming. Older version-1 and version-2 libraries require the explicit upgrade described below. `--help` and `--version` work without an interactive terminal or opening the library.

- The TUI opens on the **全部本地** playlist, which contains every local track. Playback, advance, and repeat modes apply only to the current playlist. `r` cycles sequential (stop at the end), list loop, and single-track loop. The status line shows the mode.
- `l` toggles a right-hand lyrics panel. It reads `.lrc` text already stored in the library, highlights the line for the current position, and shows an empty state when the track has no lyrics. Artwork is not rendered.
- `:playlist list` shows playlists. `:playlist new <name>` creates one. `:playlist add <playlist>` adds the selected track. `:playlist use <playlist>` shows that playlist, and `:playlist play <playlist>` starts it. `:playlist library` returns to grouped library browsing.
- Browsing groups show aligned line numbers starting at 1 in library order and candidate counts. Numbers remain absolute when scrolled.
- `j/k` or Up/Down selects a group. Enter plays a single candidate or opens a chooser for alternatives. In the chooser, `j/k` or arrows move, Enter confirms the exact candidate, and Esc cancels while the current song continues. Numeric counts work with movement: `3j` moves down three rows and `2k` moves up two.
- Ctrl-d/u moves down/up half the focused panel's visible rows. Ctrl-f/b or PageDown/PageUp moves a page with one row of overlap, following agent board's keyboard rules. Counts multiply the distance: `2` followed by Ctrl-d moves two half pages. Short libraries count only existing rows, and distances adapt to resizing and stop at both ends.
- Keyboard movement and jumps keep the viewport in place while the target is visible; crossing an edge scrolls only enough to reveal it. Import results scroll independently when that panel has focus.
- In `NORMAL`, the mouse wheel scrolls the panel under the pointer by three rows. In the library, selection follows the viewport's actual movement and stays on the same screen row; both stop at viewport boundaries. Results scroll independently without changing keyboard focus. Command editing, import progress and panel borders ignore wheel input. Mouse capture is disabled on exit.
- `gg` / `G` jumps to the beginning/end of the focused panel. `23G` selects song line 23; `:23` followed by Enter does the same and focuses the library. Numbered jumps stay visible and clamp to the available songs.
- `/text` searches forward and `?text` backward through titles, artists and albums across every candidate, including alternatives. Matching uses literal, case-insensitive Unicode text, preserves the complete list and its line numbers, and previews from the original selection as you type. Matching rows are highlighted; the library title shows the query and current match/total (0 when selection is not a match).
- Enter confirms search without playing; Esc restores the original song, viewport and panel focus. `n` follows the confirmed direction and `N` reverses it, wrapping through matches; counts such as `3n` are supported. Empty `/` or `?` followed by Enter repeats the previous text in that direction. Search results refresh after imports, including commits from another CLI or TUI. A background reader checks about every 500 ms and preserves command/search input, selection and focus. Changed chooser membership requires fresh confirmation; a failed refresh keeps the last valid view and retries. Search input ignores Tab completion and wheel navigation.
- Esc clears unfinished numeric or `g` prefixes in `NORMAL`. Entering a command or switching focus also clears them. Navigation keys do not act on the list during command editing.
- Space pauses/resumes, or plays the selection if nothing is loaded.
- Left/Right seeks backward/forward 10 seconds when the track list has focus. Seeking while paused keeps playback paused.
- `:stop` stops, retains the current source and resets its position.
- The bottom-left badge shows `NORMAL` for Vim-style navigation/playback controls and `COMMAND` for `:`, `/` and `?` prompts. Esc cancels and Enter submits input. `INSERT` applies to ordinary UI text fields; the current interface has no such fields.
- `:import ~/Music/NetEase` imports NCM, MP3 and FLAC from a folder or a single file. Extension checks are case-insensitive. Fresh folder imports attempt NCM first to retain its metadata and artwork when alternate downloads coexist. Unsupported files are ignored in folders; directly requesting one reports an English error. Paths may contain spaces or matching quotes; commands do not run through a shell.
- Active imports show a lower progress panel: scanning reports discovered audio files, then importing shows a file progress bar, processed/total, added candidates, exact duplicates, retained collisions, failures and the current filename. Totals count selected NCM, MP3 and FLAC sources, including exact repeats; different encoded audio counts as a retained addition. Counts update after each file finishes; a large file can keep progress unchanged while it is read or decoded. The final report replaces the panel when the import finishes or is cancelled. Import stages are independent of the `NORMAL`/`COMMAND` keyboard badge.
- In `:` command input, Tab completes built-in names: `:im` becomes `:import `, `:st` becomes `:stop`, and `:qu` becomes `:quit`. Tab at an empty `:` lists `import`, `q`, `quit`, `stop` and `detach`; `:q` lists both quit aliases. Further Tab/Shift-Tab cycles candidates. After selecting `import` from a list, type a space to complete its path. Completion only edits input; Enter executes and Esc cancels.
- In `:import`, Tab completes directories and NCM, MP3 and FLAC files. A unique match fills immediately; multiple matches expand their common prefix and show a candidate list. Further Tab presses cycle forward; Shift-Tab cycles backward. A unique directory match ends with a separator so another Tab can complete its children. Relative, absolute, `~/`, quoted and Unicode paths are supported.
- In `COMMAND`, Left/Right and Home/End move the text cursor; Backspace/Delete edit it. Completed whitespace paths are quoted, with the cursor inside the closing quote. Long inputs scroll horizontally.
- In `NORMAL`, Tab focuses import results; `j/k` scrolls vertically and `h/l` or Left/Right scrolls horizontally.
- `:q`, `:quit` or Ctrl-C stops the shared playback session, cancels remaining imports, waits for the current file to commit or roll back, and restores the terminal.
- `:detach` exits the interface while keeping playback (or pause) unchanged. `:det` followed by Tab completes it. Detach also cancels remaining imports safely; imports do not become background jobs. Reopen `muse` with the same library to reconnect without restarting the song.

Playback belongs to a local service, started automatically when opening the TUI. CLI commands control that same session without a terminal:

```sh
muse --library ./library.muse status
muse --library ./library.muse pause
muse --library ./library.muse play       # Resume/start the current loaded track
muse --library ./library.muse play 42    # Play database track ID 42 inside the current playlist
muse --library ./library.muse playlist list
muse --library ./library.muse playlist create "Night Drive"
muse --library ./library.muse playlist add "Night Drive" 42
muse --library ./library.muse playlist play "Night Drive"
muse --library ./library.muse next
muse --library ./library.muse mode       # Cycle sequential, list, and single
muse --library ./library.muse stop
muse --library ./library.muse shutdown   # Stop playback and exit the service
```

`play <track-id>` uses a database ID, **not the numbered row in the TUI**, and plays that track inside the current playlist. The default playlist is 全部本地. `playlist play` switches playlist and starts at its first track. `next` moves to the next playlist track; sequential mode stops at the end, list mode wraps, and single mode repeats the current track when it finishes. `status` shows the current track ID, state, position, duration, playlist, repeat mode, and playback errors. Explicit track playback can start a service for an existing library; bare `play` requires a loaded track. Pause and stop are idempotent. With no service, status reports its absence and stop/shutdown do nothing successfully; these commands do not create a library. Commands report success after completion. If a connection is lost before the response, inspect status before repeating the operation.

One service is shared by this user and the library's canonical path. Relative and symlink aliases reconnect to it; hard-link aliases are not unified. Multiple TUIs share playback, so `:q` in one stops it for all; `:detach` leaves it alone. Selection and search prompts remain local to each TUI. The service stays available after stop or track completion until `shutdown` or OS termination. It does not register for login startup or restore playback after service restart.

An observed terminal hangup stops playback. A forced kill or crash cannot send stop, so the service continues and can be controlled from another terminal. If the service exits, clients report disconnection; they do not automatically restart the song. Shut down the old service before replacing muse with an incompatible version.

The service listens only on loopback and requires a random credential in a user-private runtime directory: `$HOME/.muse-sessions` on Unix or `%LOCALAPPDATA%\.muse-sessions` on Windows. `MUSE_RUNTIME_DIR` can select an isolated runtime directory; its parent must already exist. Existing directories must have owner-only permissions (0700 on Unix; a protected owner-only DACL on Windows). All clients for a session must use the same runtime directory. No credentials are passed in command arguments.

Playback controls and audio reads run in the background. The current track changes only after successful loading. The status line shows actual state, music position, known duration, the current playlist, and the repeat mode. When a track finishes, sequential mode plays the next playlist track and stops at the end; list loop wraps; single loop restarts the track. Invalid replacement audio preserves a healthy old source. Runtime input, decoder or device failures remain visible until a successful load or stop rebuilds playback. Browsing, importing and backups remain usable without an audio device.

The built-in `muse-library::netease` extension owns NCM decoding, NetEase tags/IDs, local download selection, attachment association and browsing groups. Storage owns audio blobs, candidates, source observations and transactions. Ordinary MP3/FLAC files remain usable without a NetEase ID. Unknown library types are rejected. Additional source extensions, APIs for lyrics/artwork, and multi-library aggregation are deferred.

Imports preserve source files and work offline. Exact encoded audio bytes share storage after NCM unpacking; tags are part of those bytes. The same NetEase ID with different audio retains alternatives. Different explicit IDs remain separate even when the audio is identical. An ID-less exact copy can reuse an unambiguous candidate or gain an observed ID, but cannot merge conflicting identities. Verified source reimports are idempotent.

Candidates with the same NetEase ID form a platform song. Platform songs whose representative real tagged title and artist match after trimming and Unicode lowercasing share a browsing group. Missing tags never use filename equality for grouping. Groups are presentation only: live recordings and remasters retain separate candidate IDs, audio and attachments. No version is ranked or preferred automatically. A `g:<lowest-candidate-id>` token identifies the current group; deleting its anchor invalidates that token.

Reimports can fill missing primary attachments on the same candidate. Conflicting attachment observations remain recorded, and a name collision never copies artwork or lyrics to another recording. `show` exposes source/container information and attachment attribution, including unknown legacy provenance. Artwork is stored but not rendered. Lyrics from a stored `.lrc` attachment are shown in the TUI lyrics panel.

Lyrics come from matching local `.lrc` files. Artwork comes from embedded data or `meta/track-ID.jpg` matching a NetEase ID already present in the file. Attachment searches stay within the import scope and do not follow external symlinks.

## Library commands and command mode

```sh
muse list --search "artist"             # Browsing groups, searches every candidate
muse list --candidates --json           # Stable candidate IDs for scripts
muse show g:42 --json                   # Group candidates and provenance
muse show 42                            # Exact candidate details
muse import ~/Music/NetEase             # Import local NCM, MP3 and FLAC
muse library info                      # Stored name, type and schema version
muse command                           # Persistent interactive prompt
```

`--library <file>` works before or after these commands. `list`, `show` and `library info` require an existing library and never start playback. JSON query output goes to stdout; import reports and errors go to stderr. One-shot import takes a literal filesystem path; let the shell expand `~` (for example, `muse import ~/Music/NetEase`) or use an absolute path. TUI and command-mode import also support `~/` themselves.

Command mode supports repeated commands, Up/Down history, editable Unicode input, Tab/Shift-Tab command and import-path completion, and quoted paths. It does not invoke a shell. Import runs in the background, so the prompt remains usable. Only one import runs per client. `q`, `quit`, Ctrl-C or EOF (Ctrl-D on an empty line) stops shared playback and safely finishes the current import transaction. `detach` cancels remaining imports and exits while playback continues.

`play g:42` in command mode lists numbered alternatives; enter a number to confirm or Esc to cancel. A one-shot ambiguous group play exits unsuccessfully with candidate IDs, without changing playback. `play 42` always loads candidate ID 42 directly. TUI row numbers refer to groups and are independent of candidate IDs.

## Upgrading an older library

Close all TUI and command-mode clients, and shut down the old playback service **with the old binary before replacing it**. New clients reject incompatible services rather than reusing them.

```sh
muse --library ./library.muse shutdown
muse --library ./library.muse library upgrade
muse --library ./library.muse import /path/to/original/downloads
```

Upgrade requires exclusive maintenance ownership and refuses active clients/services. It first writes and validates a unique, no-overwrite `<library>.pre-v2.<suffix>.muse` backup, including committed WAL data. The schema migration is atomic; a failed backup leaves the schema untouched. Backups stay on disk until you remove them explicitly. New clients honor maintenance ownership; old binaries must be closed manually. The adjacent `.maintenance.lock` file may remain after exit; ownership is an OS lock, not the file's presence.

Existing candidate IDs, owned audio and attachments survive. Old source mappings are marked unverified because some may refer to audio previously skipped by name. Reimport reads and decodes the original files, recovers missing candidates and repairs source mappings transactionally; repeating it adds nothing further. Audio that was never stored cannot be recovered without its original source. Existing legacy attachment attribution remains unknown rather than being reassigned to a recovered version. Legacy real-tag grouping is established as sources are verified.

To roll back, close clients/services and restore the pre-upgrade backup to a **new** path. That copy remains version 1 and can be opened by the old binary; a new client requires explicit upgrade. A pre-upgrade backup does not include imports made afterward. To intentionally rebuild an empty library instead, close clients, shut down its service and use `make reset-library LIBRARY="/path/to/library.muse"`. Reset removes stored audio and SQLite journals, leaving originals unchanged; `make clean` only removes Cargo build output.

## Backup and restore

```sh
muse --library ./library.muse --backup ./library-backup.muse
muse --library ./restored.muse --restore ./library-backup.muse
```

For development, replace `muse` with `cargo run -p muse --`. Both operations run without a terminal. Copies include a consistent committed snapshot, including WAL data, complete audio, attachments and deduplication records. Destinations and their SQLite journal files must not exist. Copies are validated and published as portable single files. Missing sources fail without creating empty libraries. Backup/restore validate version-1, version-2, and version-3 layouts without upgrading the source. Explicit upgrade creates its own verified pre-upgrade backup: `.pre-v2` when leaving version 1, and `.pre-v3` when leaving version 2. A version-1 upgrade continues through the current playlist schema in the same transaction.

## Development

Install rustup and GNU Make. `rust-toolchain.toml` pins Rust 1.95.0, rustfmt and Clippy. Linux builds also require ALSA headers and pkg-config:

```sh
sudo apt-get install libasound2-dev pkg-config  # Debian/Ubuntu
make ci       # Format, interface check, Clippy, tests, docs and release build
make fmt
make run
make release  # target/release/muse
```

Linux runtime playback requires ALSA and an available output device. macOS and Windows use system audio interfaces. CI covers Linux, macOS and Windows builds; physical output checks are separate.

Development builds optimize SHA-256 while retaining application debug information. Ordinary MP3/FLAC imports reuse their source hash as the audio hash; NCM hashes the unpacked audio separately.

The playback crate defaults to standard-library traits. The application enables `native`; it exposes `NativePlayback`, `PlaybackError` and `PlaybackErrorKind` with bounded PCM buffering and owned inputs. A fixed-capacity ring buffer transfers complete audio frames without locking the output callback. Reading playback status or publishing decoded audio does not replace queued music with silence.

The terminal worker uses `Library::import_downloads_with_progress` for mixed formats and tagged-title/artist matching. The library crate also retains general-purpose content-only MP3/FLAC/NCM import entry points and the explicit `Library::import_ncm_with_progress` policy for Rust callers. All policies share decoding, transaction, cancellation and attachment handling.

```sh
cargo check -p muse-playback --locked --no-default-features
cargo test -p muse-playback --locked --features native
# Requires a default output device and plays a short generated test tone:
cargo test -p muse-playback --features native default_device_smoke -- --ignored --nocapture
cargo test -p muse library_playback_controls_and_stale_requests -- --ignored --nocapture
```

Service tests run real processes against temporary libraries with a clock-based playback implementation linked only into test executables. Terminal tests exercise detach, reattachment, shared quit, hangup, service loss and import cleanup through a pseudo-terminal on Unix. Production binaries always use the native audio backend.

Automatic backend tests consume real decoded PCM and Rodio mixer output without requiring a sound card. An optional local import check is read-only and uses a temporary library:

```sh
MUSE_SAMPLE_ROOT=~/Music/NetEase cargo test -p muse-library --test local_samples -- --ignored
```

The sample folder must directly contain MP3, FLAC and NCM. CI uses generated repository fixtures rather than personal music.

Import and playlist workflows have a separate test-only workspace package. English Gherkin stories cover MP3/FLAC imports, multiple tracks, recursive folders, Unicode paths, mixed downloads, local lyrics/artwork, repeated imports, and phase-1 playlist playback. Given / When / Then step definitions use real isolated source files and SQLite through the public library interface. Playlist stories drive the `muse` CLI against the shared playback service. Cucumber discovers `.feature` files and reports each step. Workspace tests and CI include all scenarios.

```sh
make e2e
make e2e-list
make e2e E2E_FILTER=twice
cargo test -p muse-e2e --locked --test import -- --name twice
```

See [the E2E guide](crates/muse-e2e/README.md) for the scenarios and their Rust step definitions.

## Releases

The application version is in `crates/muse/Cargo.toml`. cargo-release 1.1.2 manages application versions and `v<version>` tags. Packages are not published to crates.io; internal libraries do not create their own tags.

```sh
make release-tools
make release-dry-run LEVEL=patch
make release-tag LEVEL=patch
```

Release commands require a clean committed `main` branch, Git identity and an origin remote. A dry run previews changes. The tag command runs checks, bumps the application version and creates a local commit/tag; it does not push automatically.

```sh
git push --atomic origin main v0.1.1
make package TAG=v0.1.0
```

Version tags trigger GitHub Actions. Successful builds produce Linux x86_64, macOS ARM64 and Windows x86_64 archives containing the binary, README, LICENSE, NOTICE and reference licenses, plus SHA-256 checksums. Packaging requires Python 3.11+ and writes to `dist/`. The release tag must match the application manifest version.

## License

AGPL-3.0-only. [LICENSE](LICENSE) contains the unmodified [official GNU license](https://ftp.gnu.org/gnu/Licenses/agpl-3.0.txt).
