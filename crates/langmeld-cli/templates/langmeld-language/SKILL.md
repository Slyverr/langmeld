---
name: langmeld-language
description: Help an agent extend this Langmeld language project while keeping syntax, Rust behavior, and tests in their intended places.
---

# Work on this language project

Use this skill when changing the language's syntax, semantics, parser,
runtime, or tests.

## Find the right files

- Read `README.md` first for this project's commands and layout.
- In a declarative project, start at `language/main.langmeld` and follow its
  explicit imports. Syntax is under `language/`; construct behavior is under
  `src/constructs/`.
- In a custom-parser project, syntax and AST decisions live in the Rust parser
  implementation. Keep the host-owned tree and diagnostics source-aware.
- Runtime and application-specific behavior belong in the Rust host.

## Make a language change

1. Decide whether the change affects syntax, language meaning, or both.
2. Update the declarative grammar or custom parser for syntax changes.
3. Implement language-specific semantics in Rust; do not move runtime policy
   into grammar declarations.
4. Add or update integration tests under `tests/` for valid programs and
   relevant diagnostics.
5. Run `cargo fmt --all -- --check`, `cargo test --locked`, and `cargo run`.

For a behavior-bearing declarative construct, `langmeld generate construct
<name>` creates its syntax declaration, root import, and Rust handler. Edit the
grammar directly for syntax entries that do not need Rust behavior.

For declarative projects with the Langmeld CLI installed, run `langmeld check`
to validate the definition and imports.
