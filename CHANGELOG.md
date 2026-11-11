# Changelog

All notable changes to Langmeld are documented here.

The format follows [Keep a Changelog 2.0.0](https://keepachangelog.com/en/2.0.0/),
and this project uses [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).
Migration notes for the current unreleased API are in the
[upgrading guide](docs/guides/upgrading.md).

## [Unreleased]

### Changed

- **Breaking:** `HostDocument::parsed()` and `analysis()` now return `Option`;
  mismatched snapshot results are discarded, and stale parse trees are never
  passed to analyzers.
- **Breaking:** `AnalysisContext::new` now returns `Result` and rejects parsed
  documents from a different database-managed source snapshot.
- **Breaking:** `ConstructHandler::handle` now takes only `ConstructContext`.
  Use `context.construct()` for the current node and omit the construct
  argument from field access and child dispatch methods.
- Cloned source snapshots now share immutable source text instead of copying it.
- **Breaking:** Loaded definition projects keep their manifest, parsed
  definition, and source snapshots together behind read-only accessors and
  `into_parts()`.
- **Breaking:** LSP backend documents keep source identity, host state, and
  diagnostics associated through read-only accessors and `into_parts()`.
- **Breaking:** Host parser and analysis documents keep their source identity,
  payload, and diagnostics associated through read-only accessors and
  `into_parts()`.
- **Breaking:** Generic parse results expose constructs, syntax trees,
  diagnostics, and resource-limit status through read-only accessors and can
  transfer owned results with `into_parts()`.
- **Breaking:** Expression parse results keep source identity, AST, consumed
  range, and diagnostics associated through read-only accessors and
  `into_parts()`.
- **Breaking:** Source snapshots, identities, locations, definition sources,
  and LSP source mappings and host document bundles expose source-associated
  data through read-only accessors instead of public fields.
- Plain-text diagnostics now include their error, warning, or info severity.
- The declarative starter and examples use severity-aware validity checks and
  report non-error findings without treating them as failures.
- The generated starter README now explains how to add a construct and where
  to implement its Rust behavior.

### Added

- Construct handler registries can dispatch top-level parse results with
  database-managed source-snapshot validation.
- Construct handler registries can dispatch validated top-level children when
  root constructs are structural containers.
- Construct handler registries can dispatch one selected construct directly,
  enabling optional behavior for root and module constructs.
- Project-definition LSP mode supports go-to-definition and references for
  construct and category names across imported `.langmeld` files.
- Shared error-severity detection and explicit validity checks for generic,
  expression, and host parsing and analysis results.
- Source-located and project-level constructors for every diagnostic severity.
- Public helpers to quote and decode Langmeld definition string literals.
- A declarative function-language example showing parameter captures, typed
  AST lowering, forward declarations, and ordered semantic analysis in Rust.
- A complete Brainfuck interpreter demonstrating host-owned parsing,
  source-located diagnostics, and bounded execution.

### Fixed

- Report cleanup failures when project initialization rolls back a partial
  scaffold, while preserving the original write error.
- Reject stale managed parse snapshots in the schema language example's
  per-file and workspace analysis.
- Treat warnings and informational findings as non-fatal in language-definition
  validity checks; only error diagnostics prevent a definition from being valid.
- Format managed diagnostic locations against their exact source snapshot,
  avoiding incorrect line and column values for colliding names or stale
  revisions.
- Diagnose host parser or analyzer results tied to a different managed source
  snapshot, discard mismatched results, and reject editor queries for invalid
  backend documents.
- Apply the shared offset and source-span validation to host LSP completion and
  navigation results.
- Fall back to per-source analysis for every open document when configured host
  workspace analysis returns incomplete or mismatched results.
- Require commas between function parameters in the declarative-functions
  example and reject trailing commas.
- Restore the language definition and handler registry when construct generation
  fails, and report all rollback failures.

## [0.7.0] - 2026-11-11

### Added

- The declarative parser returns a lossless syntax tree alongside parsed
  constructs, preserving token, comment, whitespace, and unclassified-source
  spans for host tooling.
- Handler contexts can iterate repeated captures in source order and read each
  captured source span.
- `langmeld init` can optionally create project-specific `AGENTS.md`
  instructions and a local Langmeld authoring skill.

## [0.6.0] - 2026-11-11

### Added

- Generic construct handlers with host-owned state, validated captured-field
  access, source-located diagnostics, explicit child dispatch, and host-defined
  control-signal propagation.
- A working declarative `langmeld init` starter with string bindings, `print`,
  runnable sample output, and syntax and semantic error tests.
- `langmeld generate handler <construct>` for validated construct names,
  additive handler stubs, and safe registration in generated source sections.
- Ordered `LanguagePass` analysis with host-owned state and diagnostics, while
  retaining the existing single-callback `LanguageAnalyzer` option.
- Incremental LSP document updates using negotiated UTF-8, UTF-16, or UTF-32
  positions, plus host workspace-symbol search and document outlines.
- Host-resolved navigation and references across closed project files, with
  document-close callbacks for host-managed indexes.

### Changed

- `langmeld generate construct` now treats the construct name as a stable
  semantic identity and accepts a separate starter spelling with `--literal`.
- Interactive construct generation asks for the initial syntax spelling while
  keeping the name-based default for noninteractive use.

### Fixed

- Preserve LF line endings in generated project files across operating systems.
- Use `language/main.langmeld` as the generated project's conventional root
  definition and keep initializer templates in separate files.
- Cross-platform file-URI comparison and workspace lockfile package versions.

## [0.5.0] - 2026-11-10

### Added

- Usable declarative and custom-Rust `langmeld init` starters with CLI/LSP
  workflows, clean-template CI coverage, and documented project commands.
- Modular `.langmeld` definitions with imports, embedded and overlay sources,
  construct categories, root declarations, comments, finite choices, and
  delimiter-bounded host expression captures.
- Project-aware definition authoring with context-sensitive completion for
  references, imports, and choice values, plus watched-file refresh.
- A bundled-definition extension example showing crate-provided constructs.

### Fixed

- Starter validation, package-name defaults, generated licenses, and recovery
  from malformed explicit-root input.

## [0.4.0] - 2026-11-10

### Added

- Parse-driven completion, navigation, and host-analysis contracts for
  language-specific semantics and editor data.
- A capability-aware LSP backend with completion, diagnostics, navigation,
  references, rename, hover, formatting, document symbols, and semantic tokens.
- TextMate and Tree-sitter exporters for supported declarative syntax, plus
  manifest-backed project loading and starter initialization.

### Fixed

- Use Rust 1.88-compatible chunk decoding in the Aster semantic-token test.

## [0.3.0] - 2026-11-10

### Added

- A public host-analysis contract for language-specific diagnostics, scoped
  name resolution, and type facts without imposing shared semantic rules.
- Aster and schema-language consumers demonstrating local and cross-source
  host analysis.

## [0.2.0] - 2026-11-10

### Added

- Syntax alternatives, optional and repeated groups, nested blocks, and
  configurable expression parsing with caller-selected termination boundaries.
- A host lexer contract that validates token text and source ranges before the
  generic parser consumes them.
- A host parser contract that keeps a language's own AST alongside source
  identity and structured diagnostics.
- A standalone expression parser with configurable prefix/infix operators,
  precedence, associativity, calls, members, and indexes.
- Explicit local `.lm` imports composed through a host-provided resolver, with
  cycle, duplicate-source, and import-depth handling.
- A separate custom-lexer consumer example for raw strings and comments.
- A custom-parser consumer example combining host-owned declarations and
  evaluation with the reusable expression parser.
- A multi-file declarative consumer example with nested local imports and a
  host-owned definition resolver.
- An Aster consumer example with a host-owned parser, scoped symbol resolution,
  and basic type checking.

## [0.1.0] - 2026-11-10

### Added

- Source snapshots, stable source identities, revisions, byte spans, and
  structured diagnostics in `langmeld-core`.
- A validated grammar model and recovering generic parser for token patterns,
  primitive captures, and explicitly scoped nested blocks in
  `langmeld-language`.
- A first `.lm` definition frontend parsed through the generic engine, plus a
  root-level Toy language consumer example.
