# muse

A standalone Rust terminal music player with Neovim-style navigation.

Import local NetEase NCM files with `:import <NCM file or folder>`. Folders are scanned recursively for NCM sources; ordinary MP3/FLAC files are excluded. The SQLite library stores complete audio, metadata and associated local artwork/lyrics, so imported music no longer depends on its original files. NCM is unpacked during import; playback uses the stored MP3/FLAC through Symphonia and Rodio without an external player.

All packages live under `crates/`; the repository root is a virtual Cargo workspace:

```text
crates/
  muse/           Terminal binary and background application workers
  muse-library/   Storage, imports, backup and restore
  muse-playback/  Playback traits and optional native backend
```

Application messages, help and errors are in English. Imported metadata and Unicode paths are preserved.

Import progress composes decomposed Korean filenames into complete Hangul syllables for display while retaining the original filesystem paths for reading.

## Usage

```sh
cargo run -p muse -- --library ./library.muse
```

The default library is `./library.muse`. `--help` and `--version` work without an interactive terminal or opening the library.

- `j/k` or Up/Down selects a track; Enter plays it.
- Space pauses/resumes, or plays the selection if nothing is loaded.
- Left/Right seeks backward/forward 10 seconds when the track list has focus. Seeking while paused keeps playback paused.
- `:stop` stops, retains the current source and resets its position.
- The bottom-left badge shows `NORMAL` for Vim-style navigation/playback controls and `COMMAND` for `:` commands. Esc cancels and Enter submits a command. `INSERT` applies to ordinary UI text fields; the current interface has no such fields.
- `:import ~/Music/NetEase` imports NCM files from a folder or a single NCM file. Extension checks are case-insensitive. Ordinary MP3/FLAC in a folder are ignored; directly requesting one reports an English error. A folder without NCM files imports zero songs. Paths may contain spaces or matching quotes; commands do not run through a shell.
- Active imports show a lower progress panel: scanning reports discovered NCM files, then importing shows a file progress bar, processed/total, added, skipped duplicates, failures and the current filename. Totals count only selected NCM sources. Counts update after each file finishes; a large file can keep progress unchanged while it is read or decoded. The final report replaces the panel when the import finishes or is cancelled. Import stages are independent of the `NORMAL`/`COMMAND` keyboard badge.
- In `COMMAND`, Tab completes built-in names: `:im` becomes `:import `, `:st` becomes `:stop`, and `:qu` becomes `:quit`. Tab at an empty `:` lists `import`, `q`, `quit` and `stop`; `:q` lists both quit aliases. Further Tab/Shift-Tab cycles candidates. After selecting `import` from a list, type a space to complete its path. Completion only edits input; Enter executes and Esc cancels.
- In `:import`, Tab completes directories and NCM files. A unique match fills immediately; multiple matches expand their common prefix and show a candidate list. Further Tab presses cycle forward; Shift-Tab cycles backward. A unique directory match ends with a separator so another Tab can complete its children. Relative, absolute, `~/`, quoted and Unicode paths are supported.
- In `COMMAND`, Left/Right and Home/End move the text cursor; Backspace/Delete edit it. Completed whitespace paths are quoted, with the cursor inside the closing quote. Long inputs scroll horizontally.
- In `NORMAL`, Tab focuses import results; `j/k` scrolls vertically and `h/l` or Left/Right scrolls horizontally.
- `:q` or Ctrl-C stops playback, cancels remaining imports, waits for the current file to commit or roll back, and restores the terminal.

Playback controls and audio reads run in the background. The current track changes only after successful loading. The status line shows actual state, music position and known duration; natural completion retains the current track and stops. Invalid replacement audio preserves a healthy old source. Runtime input, decoder or device failures remain visible until a successful load or stop rebuilds playback. Browsing, importing and backups remain usable without an audio device.

Imports preserve source files and do not access NetEase databases or network services. SHA-256 deduplication compares complete source/audio bytes; different encodings or tags remain separate tracks. `Skipped duplicates` counts existing audio encountered again, including renamed copies and NCM/plain files with identical unpacked audio; these do not create extra tracks. Re-imports can fill missing attachments without overwriting existing ones. Artwork and lyrics are currently stored, not displayed.

Lyrics come from matching local `.lrc` files. Artwork comes from embedded data or `meta/track-ID.jpg` matching a NetEase ID already present in the file. Attachment searches stay within the import scope and do not follow external symlinks.

NCM-only imports do not merge earlier library records. To rebuild from an empty library, exit muse and reset the stored library, then import your source folder again:

```sh
make reset-library
# For a custom library:
make reset-library LIBRARY="/path/to/library.muse"
```

Reset deletes the selected library and its SQLite WAL, shared-memory and journal files, including stored audio, metadata, attachments and deduplication records. Original source files are preserved. `make clean` only removes Cargo build output.

## Backup and restore

```sh
muse --library ./library.muse --backup ./library-backup.muse
muse --library ./restored.muse --restore ./library-backup.muse
```

For development, replace `muse` with `cargo run -p muse --`. Both operations run without a terminal. Copies include a consistent committed snapshot, including WAL data, complete audio, attachments and deduplication records. Destinations and their SQLite journal files must not exist. Copies are validated and published as portable single files. Missing sources fail without creating empty libraries. Automatic backups before future schema migrations are not implemented.

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

The playback crate defaults to standard-library traits. The application enables `native`; it exposes `NativePlayback`, `PlaybackError` and `PlaybackErrorKind` with bounded PCM buffering and owned inputs.

The library crate retains its general-purpose MP3/FLAC/NCM import entry points for Rust callers. The terminal worker uses `Library::import_ncm_with_progress`, which shares the same storage and decoding pipeline while selecting only NCM sources.

```sh
cargo check -p muse-playback --locked --no-default-features
cargo test -p muse-playback --locked --features native
# Requires a default output device and plays a short generated test tone:
cargo test -p muse-playback --features native default_device_smoke -- --ignored --nocapture
cargo test -p muse library_playback_controls_and_stale_requests -- --ignored --nocapture
```

Automatic backend tests consume real decoded PCM and Rodio mixer output without requiring a sound card. An optional local import check is read-only and uses a temporary library:

```sh
MUSE_SAMPLE_ROOT=~/Music/NetEase cargo test -p muse-library --test local_samples -- --ignored
```

The sample folder must directly contain MP3, FLAC and NCM. CI uses generated repository fixtures rather than personal music.

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
