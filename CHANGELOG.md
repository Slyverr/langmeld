# Changelog

This file records notable user-facing changes; it does not list every
implementation commit. Its format follows [Keep a Changelog 1.1.0] and the
project's version policy follows [Semantic Versioning 2.0.0]. Until a release
is cut, changes belong under `Unreleased`.

## [Unreleased]

### Added

- **Aster example:** Demonstrate a modular language project with function-local
  scopes, host type checks, typed-AST lowering, an optimization pass, an
  interpreter, a host-defined expression atom, and a separately implemented
  syntax extension.

### Changed

- **Tree-sitter capability:** Mark generated grammars as partial because
  grammar parsing does not reproduce Langmeld semantic validation.
- **Editor capabilities:** Report language configuration as partial when host
  parser callbacks may add delimiter syntax outside the declared bracket model.
- **Generated Rust AST:** Type references by symbol-kind marker instead of an
  arbitrary construct that happens to declare that kind.
- **Generated documentation:** Include artifact and language-service support
  statuses from the validated capability report in the generated grammar
  reference, and direct the generated Agent Skill to that source.
- **TextMate highlighting:** Match longer overlapping tokens first and use
  identifier boundaries that preserve hyphenated language tokens.
- **Tooling capabilities:** Report host-provided custom lexer callbacks, and
  explain when custom parser callbacks make generated TextMate and LSP semantic
  highlighting incomplete.
- **Highlighting classifications:** Map shared classifications to TextMate
  scopes, Tree-sitter captures, and LSP token types in generated metadata.

## [0.1.0] - 2026-11-04

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
- **Lexical policy:** Schema 4 can declare line and non-nested block comments.
  Built-in parsing, expression parsing, TextMate, editor comment settings, and
  generated Rust models share the configured policy; unsupported Tree-sitter
  block-comment export is reported explicitly.
- **Symbol analysis:** Schema 5 can configure declaration order, duplicate-name
  handling, and shadowing once per symbol kind. Ambiguous references produce
  diagnostics without navigation targets; defaults preserve existing behavior.
- **Definition validation:** Bounded syntax-conflict analysis reports a warning
  when it cannot inspect every candidate. Rust hosts can read these findings
  from the validated language, and `langmeld check` includes them separately
  from source diagnostics.
- **Editor configuration:** Generated bracket pairs follow declared blocks and
  built-in list, map, property, and expression fields; unused conventional
  pairs are omitted.
- **Tooling capabilities:** Generated language projects now receive a versioned
  capability report with support levels and reasons for tooling artifacts and
  generic editor features. Rust hosts can request the same typed report through
  `generate_tooling_capabilities`. Generated highlighting metadata and LSP
  semantic tokens now share the same token type and modifier legends.
- **Expression tooling:** The lossless syntax tree exposes nested expression,
  operator, atom, group, call, member, and index nodes while retaining each
  source token exactly once. Generated browser adapters serialize expression
  nodes and source spans as structured JSON.
- **Project workflow:** An interactive CLI initializes and configures Rust
  language projects, checks and formats programs, inspects definitions,
  manages local declarative extensions, and generates deterministic editor and
  host artifacts. Safe refresh preserves authored callback implementations.
- **Rust-host tooling:** Generated CLI projects can rebuild and check tooling
  from their compiled Rust or combined language model with `generate` and
  `generate --check`. Library-only projects can use the same safe writer APIs.
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

- **Generated guidance:** Describe inferred bracket pairs accurately and state
  that undeclared conventional pairs are omitted.
- **Project workflow:** Keep generated starter and example builds working with
  canonical paths across symlinked and Windows project locations.
- **Project locks:** Serialize definition paths with portable separators so
  lockfiles stay stable across operating systems. Normalize CRLF to LF in
  definition hashes so Windows checkout conversion does not stale a lock.
- **LSP paths:** Convert Windows drive and UNC paths to standard file URIs.

[Keep a Changelog 1.1.0]: https://keepachangelog.com/en/1.1.0/
[Semantic Versioning 2.0.0]: https://semver.org/spec/v2.0.0.html
