# Changelog

This file records notable user-facing changes; it does not list every
implementation commit. Its format follows [Keep a Changelog 1.1.0] and the
project's version policy follows [Semantic Versioning 2.0.0]. Until a release
is cut, changes belong under `Unreleased`.

## [Unreleased]

### Added

- **Language framework:** Build languages from modular `.langmeld` definitions or
  Rust-authored models, resolved to one validated model. Generic lexing and
  parsing, typed AST generation, workspace analysis, source diagnostics,
  formatting, custom Rust host hooks, and host-owned runtime/lowering/backend
  contracts support interpreted or artifact-producing languages.
- **Language grammar:** Schema 2 blocks own their delimiters and child separators.
  New declarative projects use the schema, and parsing, formatting, generated
  editor configuration, and representable Tree-sitter grammars honor the layout.
- **Expression support:** Declare reusable built-in expression grammars and
  expression-valued fields in schema 3. Parsing honors operator precedence,
  nested groups, explicit punctuation terminators, and source spans. The
  standalone expression crate also provides a host-defined atom API.
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

### Changed

- **Definition files:** Use `.langmeld` for Langmeld language definitions.
  Languages built with Langmeld can still choose `.lm` as a program suffix.

### Fixed

- **Project workflow:** Keep generated starter and example builds working with
  canonical paths across symlinked and Windows project locations.
- **Project locks:** Serialize definition paths with portable separators so
  lockfiles stay stable across operating systems. Normalize CRLF to LF in
  definition hashes so Windows checkout conversion does not stale a lock.
- **LSP paths:** Convert Windows drive and UNC paths to standard file URIs.

[Keep a Changelog 1.1.0]: https://keepachangelog.com/en/1.1.0/
[Semantic Versioning 2.0.0]: https://semver.org/spec/v2.0.0.html
