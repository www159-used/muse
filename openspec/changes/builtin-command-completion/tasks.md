## 1. Command completion

- [x] 1.1 Share a private built-in command registry between dispatch and completion, implement contextual name/path sessions and regression tests.
- [x] 1.2 Render Commands candidates and verify cycling, cancellation, submission, mode badges and name-to-NCM-path import through the UI.

## 2. Documentation and validation

- [x] 2.1 Update usage/help/context and pass workspace checks, strict OpenSpec validation and a real-terminal smoke test using a temporary library.

## Verification

- `cargo test -p muse --locked`: 24 application/maintenance tests passed; the physical output-device test remains explicitly ignored.
- `make ci`: formatting, default-free playback interface check, Clippy with denied warnings, 65 workspace tests, two documentation tests and the release build passed. Three existing physical-device/personal-source tests remain ignored.
- `openspec validate --all --strict`: all nine changes passed.
- Real PTY smoke using the release binary and a temporary library: Commands popup, empty-prefix discovery, Tab/Shift-Tab cycling and wraparound, completed stop submission, `q` alias selection/cancellation and uniquely completed `quit` requiring Enter all passed. NORMAL/COMMAND badges and terminal restoration passed.
- The same PTY smoke completed `im` to `import `, completed a quoted Korean NCM path while ignoring its MP3 sibling, and imported one track. SQLite integrity, stored MP3 bytes, original NCM source hash and unchanged fixture sources were verified.
- The initial smoke harness expected a trailing space after trimming an empty NORMAL input row; correcting that assertion allowed the complete real-terminal smoke to pass without application changes.
