## Context

The completed import-scenario-e2e implementation contains 39 independent TOML cases. Its real filesystem/SQLite world and public-interface assertions already provide the required coverage, but nested tables expose the runner's representation instead of the import story. This change supersedes that case-format choice.

## Goals / Non-Goals

**Goals:** Ordered English stories, literal readable setup and outcomes, unchanged regression coverage, automatic case registration and useful line diagnostics.

**Non-Goals:** Production changes, terminal automation, a general scripting language, scenario inheritance or fixture generation during tests.

## Decisions

- **Implementation choice:** Parse standard `.feature` syntax with the `gherkin` crate and keep `libtest-mimic` registration. Restrict each file to one ordinary Scenario with no Background, Rule, Outline or tags; unsupported constructs fail instead of being ignored. A handwritten Gherkin grammar would add maintenance work; a full asynchronous Cucumber runtime is unnecessary for these synchronous workflows.
- **Implementation choice:** Use a finite vocabulary of Given fixture/text files and empty directories, When import/move/reopen/write/delete/remove actions, and Then report/library assertions. Tables hold only parallel records such as files or tracks; lyrics use literal docstrings. Counts, metadata and attachment expectations are written explicitly in sentences or small field/value tables.
- **Implementation choice:** Translate story steps into the existing checked operations. Every import must declare its added/duplicate/failure counts, known or unknown total, and complete library contents before another action. Reopen/remove also require library contents. Optional discovery/detail checks remain explicit; unexpected scan/attachment issues or cancellation fail by default.
- **Implementation choice:** Preserve the existing world, golden audio checks, source-preservation and progress assertions. Track metadata and attachments select a declared track by title and audio fixture, so different encodings with the same title remain distinguishable. Compilation validates tables, duplicates, argument shape and ordering, and execution errors identify the action line.

## Risks / Trade-offs

- Readable steps can grow into a programming language → keep a documented finite vocabulary with no expressions, variables or arbitrary commands.
- A syntax migration can erase coverage → migrate all 39 cases, retain their source trees/actions/expected bytes and run all cases plus negative runner checks.
- Parsed syntax can be mistaken for supported execution → reject unsupported Gherkin constructs, misplaced assertions and incomplete outcomes before running side effects.

## Migration Plan

Prove one Gherkin case fails before parser support, implement that path, extend the vocabulary through existing workflows, then replace all TOML files and remove their parsing dependencies. Update contributor examples and verify CI, filtering and runtime dependency separation. No user library migration is involved.

## Open Questions

None. Gherkin is the implementation choice for the user's confirmed readability requirement.
