# Changelog

All notable changes to Langmeld are documented here.

The format follows [Keep a Changelog 2.0.0](https://keepachangelog.com/en/2.0.0/),
and this project uses [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- A declarative function-language example showing parameter captures, typed
  AST lowering, forward declarations, and ordered semantic analysis in Rust.

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
