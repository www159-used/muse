## Why

Importing a deeply nested local folder currently requires typing its entire path. The user requests Tab completion and a bottom-left Vim-style mode label, while keeping the shortcut footer removed. Navigation uses NORMAL, colon commands use COMMAND, and INSERT is reserved for ordinary UI text fields.

## What Changes

- Complete directories and MP3/FLAC/NCM paths in :import using Tab, with common-prefix expansion, visible candidates and forward/reverse cycling.
- Preserve relative, absolute, home-relative, quoted, spaced and Unicode paths without executing a shell.
- Show NORMAL while browsing/controlling playback and COMMAND while editing a colon command, with a visible text cursor.
- Keep Tab's results-focus behavior outside text entry and preserve Enter/Esc submission/cancellation.
- Document and verify completion, editing, mode transitions and actual imports.

## Capabilities

### New Capabilities

- `command-completion`: Import path completion, candidate selection, text editing and mode indication.

### Modified Capabilities

None. Existing terminal specifications remain in unarchived changes.

## Impact

Changes are limited to crates/muse terminal command handling and rendering, regression tests, README and OpenSpec records. No new dependencies, public library APIs, storage migrations or playback changes are required.
