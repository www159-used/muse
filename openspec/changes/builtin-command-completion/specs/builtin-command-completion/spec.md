## ADDED Requirements

### Requirement: Complete registered command names

In COMMAND mode, Tab SHALL complete case-sensitive prefixes of `import`, `stop`, `q` and `quit` at the end of a command token, preserving leading whitespace. Name completion SHALL require no filesystem access. Unknown names, arguments for commands without arguments, and completion inside a token SHALL leave input unchanged.

#### Scenario: Unique matches
- **WHEN** the user presses Tab after `:im`, `:st` or `:qu`
- **THEN** the input becomes `:import `, `:stop` or `:quit`, respectively, without executing anything

#### Scenario: Unsupported completion context
- **WHEN** the user presses Tab after an unknown prefix, after `stop anything`, or inside a command token
- **THEN** the existing text and cursor remain unchanged

### Requirement: Navigate ambiguous command candidates

An empty command prompt or ambiguous prefix SHALL show a Commands popup with matching names in lexical order after Tab expands their common prefix. Further Tab SHALL cycle forward and Shift-Tab SHALL cycle backward with wraparound. Editing or cursor movement SHALL dismiss the session.

#### Scenario: Discover and cycle names
- **WHEN** the user presses Tab at `:`
- **THEN** the popup contains `import`, `q`, `quit` and `stop`, and further Tab or Shift-Tab selects matching names without executing them

#### Scenario: Quit alias remains safe
- **WHEN** the user presses Tab at `:q`
- **THEN** `q` and `quit` appear as candidates and the application remains open until Enter submits the command

### Requirement: Preserve command submission and path completion

The application SHALL execute completed commands only on Enter and SHALL cancel input on Esc. A uniquely completed `import ` SHALL use the existing directory/NCM path completion on the next Tab. A fully typed bare `import` SHALL retain immediate path completion. COMMAND and NORMAL badges, NORMAL Tab focus, quoted Unicode paths and small-terminal rendering SHALL retain their behavior.

#### Scenario: Complete an import name and its argument
- **WHEN** the user completes `:im`, types a quoted Unicode NCM path prefix, presses Tab and then Enter
- **THEN** the path is completed and imported through the existing NCM-only worker, with the mode returning to NORMAL

#### Scenario: Cancel a completed quit command
- **WHEN** the user completes `:qu` with Tab and presses Esc
- **THEN** the application remains open and returns to NORMAL
