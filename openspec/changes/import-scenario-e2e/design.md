## Context

The current library integration suite covers import through real files and SQLite, but setup and assertions are mixed inside Rust functions. The requested references use declarative case files and independently reported test names. `muse-library` already exposes the import, progress, cancellation, listing, owned audio and attachment interfaces needed by an external caller.

## Goals / Non-Goals

**Goals:** A separate workspace test package, readable case files, isolated side effects, stable public-interface assertions, and a small runner that makes new cases cheap to add.

**Non-Goals:** Terminal rendering/playback tests, a shell command language, mocks of filesystem/SQLite, exporting application internals, or removing focused transaction-failure tests.

## Decisions

- **Confirmed scope:** `crates/muse-e2e` is a non-publishable test-only workspace member. Runtime packages never depend on it; it needs no library test feature. It tests the complete source-to-owned-library import flow through public Rust interfaces rather than keyboard input.
- **Implementation choice:** TOML files contain `name`, `given` files/directories and ordered `steps` with `when` operations and `then` expectations. TOML supports comments, Unicode names and multiline descriptions without a custom parser. Reject unknown fields so misspelled assertions fail. A small tagged set of operations covers import, reopen, write, move, delete-source and remove-track; do not add arbitrary scripts or SQL.
- **Implementation choice:** Follow crsu's `libtest-mimic` registration pattern: recursively discover sorted files, report each case separately, support Cargo listing/filtering and identify malformed files as failures. Reject an empty suite, duplicate case names, and paths escaping the temporary sandbox.
- **Implementation choice:** Each case owns a temporary source directory and a separate SQLite path. Verify ordered step reports, explicit library contents, exact golden audio, optional complete metadata/attachment expectations, terminal progress and unchanged source files. No global current-directory/environment mutation or sleep-based cancellation: use the public progress callback to cancel after a committed outcome.
- **Implementation choice:** Keep synthetic golden fixtures owned by the E2E package. Additional binary variants record provenance and can be regenerated offline; expected metadata is written explicitly in cases. The runner reads fixture files and never imports or reproduces the production decoder. Fixture encoders are not invoked by CI.
- **Implementation choice:** Migrate overlapping user-level tests after their scenario equivalents pass; retain focused storage fault injection, race checks and UI dispatch tests at their existing seams. Add `make e2e` and `make e2e-list`; existing workspace CI automatically includes the package.

## Risks / Trade-offs

- A growing scenario language can become another application → keep a finite set of domain operations and explicit outcomes; no inheritance, generic scripting or SQL assertions.
- Fixtures can drift or contain personal data → use generated audio, fixed synthetic metadata and documented independent golden expectations.
- Behavioral coverage can be lost during migration → map each removed test to passing scenarios and retain low-level tests that the public interface cannot express.
- New metadata shapes are not automatically compatible → store a minimal reproducer as a fixture and add its case; a case can state a useful fallback as well as full metadata support.

## Migration Plan

Add the package and one tracer case, prove failures are detected, then expand coverage and migrate overlaps. Verify focused and workspace commands. No database or production-interface migration is required.

## Open Questions

None.
