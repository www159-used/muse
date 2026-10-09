## Why

Tab currently completes import paths but leaves users to type every built-in command name. The command prompt should offer the same completion interaction for names and arguments.

## What Changes

- Complete `import`, `stop`, `q` and `quit` by prefix in COMMAND mode.
- Show a Commands popup for ambiguous prefixes, including an empty prompt; support forward and reverse cycling.
- Hand off a uniquely completed `import` to existing NCM path completion.
- Keep completion separate from submission: Enter executes and Esc cancels.

## Capabilities

### New Capabilities

- `builtin-command-completion`: Contextual completion of registered built-in command names and import paths.

### Modified Capabilities

None.

## Impact

The private command editor, command dispatch, candidate popup, application tests and usage documentation change. No public library API, dependency or database change is needed.
