## Context

The command editor already owns Unicode cursor editing, path completion sessions and candidate cycling. The UI renders a Paths popup and dispatches commands independently. Command names currently have no completion.

## Goals / Non-Goals

**Goals:** Complete registered names, reuse candidate navigation, preserve NCM path behavior and execute only on Enter.

**Non-Goals:** Shell completion, new commands, argument completion for commands without arguments, or completion in the middle of a token.

## Decisions

- **Implementation choice:** Use one private registry for name completion and command dispatch, including the `q`/`quit` aliases. This keeps suggestions aligned with executable commands; separate UI and editor lists could drift.
- **Implementation choice:** Represent each session as command names or import paths. Reuse common-prefix expansion and cycling, and label the popup Commands or Paths. Resolve the working directory only for path scans, so name completion and cached cycling cannot fail because of filesystem access.
- **Implementation choice:** A unique `im` match becomes `import ` and ends the session; the next Tab completes paths. A fully typed bare `import` retains its existing immediate path completion. Multiple-name sessions keep cycling command names without adding spaces; typing a space after selecting `import` starts path completion.
- **Implementation choice:** Limit new name completion to the end of a single command token and preserve leading whitespace. Unknown prefixes and arguments for other commands are unchanged. Editing or moving the cursor invalidates candidates. Enter and Esc retain their existing meanings, and NORMAL Tab retains its focus behavior.

## Risks / Trade-offs

- The alias `q` is also a prefix of `quit` → display both and retain `q` until explicitly cycled; never execute during completion.
- Switching from names to paths can leave stale candidates → unique completion ends its session, and text edits invalidate multiple-match sessions.
- Small terminals cannot show a full popup → retain the existing bounded popup and test minimal dimensions.

## Migration Plan

Build and restart the application. No persistent data migration is required; rolling back restores the previous command editor.

## Open Questions

None.
