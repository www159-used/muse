## 1. Navigation

- [x] 1.1 Add focused, viewport-aware half/full paging with rendered-screen tests for resizing, bounds and mode isolation.
- [x] 1.2 Add aligned song numbers, gg/G, counted movement and numeric jumps with rendered-screen tests for visibility, prefix reset, overflow and empty/tiny views.
- [x] 1.3 Align keyboard selection/camera behavior with agent board, retain one-row page overlap, support counted paging and verify visible movement, jumps, bounds and resize stability.
- [x] 1.4 Connect mouse capture and event dispatch, align wheel scrolling with agent board, and verify actual movement, targeting, mode isolation, bounds and terminal cleanup.

## 2. Documentation and verification

- [x] 2.1 Document bindings and scope, run workspace CI and strict OpenSpec validation, and record results.

## Verification

- The initial paging tracer failed with Ctrl-d leaving selection at row 1. Paging now advances selection six rows in a twelve-row view and two after resizing to five, revealing it only when necessary under the accepted agent-board refinement.
- The numbering/jump tracer failed because the line-number gutter was absent, then passed with aligned absolute numbers, 23G/:75, gg/G and counted j/k. Database IDs differ from displayed numbers in the fixture.
- A resize regression showed only five songs after enlarging the last viewport to eighteen rows. Clamping both panel offsets during drawing restored full last-page contents.
- The camera refinement tracer failed because 23G left the viewport at zero until rendering. Explicit reveal/clamp now runs after every selection change and after resize/refresh. Tests verify minimal scrolling, jumps within a scrolled viewport, stable repainting and camera bounds at both ends.
- The counted-page tracer failed because 2 Ctrl-d moved only six rows. Counts now multiply half/full page distances once, including PageUp/PageDown and results scrolling, and saturate safely for oversized input. Full pages retain the reference's one-row overlap; short libraries use existing rows.
- The wheel tracer failed because mouse events were ignored. The production dispatcher now processes wheel input with the camera and selection moving by the same actual bounded distance. Rendered tests verify stable screen rows, partial boundary moves, pinned endpoints, repeated inverse scrolling, keyboard interleaving, pointer targeting, prefixes and mode isolation.
- The real pseudo-terminal test passed: mouse capture is enabled, SGR wheel reports move the running binary's viewport three rows down and back, and quit disables mouse reporting before leaving the alternate screen.
- Twelve navigation/rendering tests and the real pseudo-terminal wheel test passed. The progress test also verifies that wheel input cannot scroll hidden previous results, and quitting ignores further wheel events.
- `make ci` passed formatting, the standard-library playback interface check, Clippy with denied warnings, 75 ordinary Rust tests, eight import scenarios with 65 steps, two documentation tests and the release build. Three existing physical-device/personal-source tests remain ignored.
- `openspec validate --all --strict` passed all twelve changes. Tests used temporary libraries, generated navigation metadata and synthetic tagged fixture audio; no personal music or library was used.
