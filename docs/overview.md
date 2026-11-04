# Langmeld overview

Langmeld supplies reusable language infrastructure. A language project defines
its syntax and data model, then uses a Rust host for behavior specific to that
language.

## Language definitions

Authors can declare constructs, fields, syntax patterns, categories, symbols,
and scopes in modular `.lm` files, or define the language model in Rust. A
project can combine Rust-authored definitions with declarative modules and
extensions. Langmeld validates the definition, parses source into a generic
syntax tree, and can generate typed Rust views of that tree.

The built-in parser handles compatible declarative syntax. A Rust host can
provide a custom parser when a language's grammar needs different parsing
behavior. The generic analyzer provides symbol and reference resolution and
workspace type facts; hosts can register language-specific analysis callbacks.

## Rust host and execution

`langmeld init` scaffolds a Rust project with optional command-line targets.
Generated host hooks can implement runtime behavior and register parsers,
analysis, or artifact backends. `langmeld generate` rebuilds derived language
tooling; `--install-host` refreshes generated host modules while preserving
marked callback implementations.

Build and run commands need host implementations for the language's actual
semantics. Langmeld validates backend descriptors, settings, artifacts, and
source maps, but it does not invent a language-specific lowering pipeline or
runtime.

## Project boundaries

- `.lm` definitions describe language structure and declarative composition.
- Rust host code supplies behavior that cannot be inferred from that structure.
- Generated files under `.langmeld/generated/` are derived from project
  sources and can be rebuilt.
- The optional browser adapter runs parsing and generic analysis in
  WebAssembly; it is not a backend that compiles a user's language to
  WebAssembly.

See [project authoring](project-authoring.md) for the workflow and
[editor tooling](editor-tooling.md) for LSP, highlighting, and browser details.
