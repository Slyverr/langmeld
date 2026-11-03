# Changelog

This file records notable user-facing changes; it does not list every
implementation commit. Its format follows [Keep a Changelog 1.1.0] and the
project's version policy follows [Semantic Versioning 2.0.0]. Until a release
is cut, changes belong under `Unreleased`.

## [Unreleased]

### Added

- **Language framework:** Build languages from modular `.lm` definitions or
  Rust-authored models, resolved to one validated model. Generic lexing and
  parsing, typed AST generation, workspace analysis, source diagnostics,
  formatting, custom Rust host hooks, and host-owned runtime/lowering/backend
  contracts support interpreted or artifact-producing languages.
- **Project workflow:** An interactive CLI initializes and configures Rust
  language projects, checks and formats programs, inspects definitions,
  manages local declarative extensions, and generates deterministic editor and
  host artifacts. Safe refresh preserves authored callback implementations.
- **Editor and browser tooling:** The language server provides diagnostics,
  completion, navigation, rename, hover, semantic tokens, formatting, and
  project-file indexing where the model or host provides the required facts.
  TextMate/editor metadata, generated documentation and agent guidance,
  optional Tree-sitter export for representable grammars, and an optional
  WebAssembly parsing/editor-analysis adapter are included.
- **Consumer examples:** Independent projects cover an interpreted DSL,
  Rust-authored and combined language models, and focused C, Rust, TypeScript,
  and Python parser or source-indexing integrations.

[Keep a Changelog 1.1.0]: https://keepachangelog.com/en/1.1.0/
[Semantic Versioning 2.0.0]: https://semver.org/spec/v2.0.0.html
