# Changelog

All notable changes to Langmeld are documented here.

The format follows [Keep a Changelog 2.0.0](https://keepachangelog.com/en/2.0.0/),
and this project uses [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).
API migration notes are in the
[upgrading guide](docs/guides/upgrading.md).

## [Unreleased]

### Added

- Expose `langmeld_project::resolve_import_key` so editor integrations can
  validate import paths with the project's resolver rules.
- Expose a `NAME` constant on generated typed construct views for concise,
  compile-time references from Rust handlers.
- Expose `construct_handler_names` so handler generators can share Langmeld's
  Rust module and handler-struct naming rules.

### Changed

- Return named parts structs from host parse, analysis, and LSP backend document
  `into_parts()` methods instead of positional tuples.
- Return a named `LoadedDefinitionProjectParts` from `into_parts()`, including
  watched paths alongside the other owned project data.
- Return named `DefinitionSourceParts` from `DefinitionSource::into_parts()`
  instead of positional tuple values.
- Match generated handler struct casing to generated typed construct views.
- Generate starter diagnostic codes with a prefix derived from the new Cargo
  package name instead of the generic `EXAMPLE` prefix.
- Use Clap for optional source-file arguments in the runnable language examples,
  giving them consistent help and argument validation.

### Fixed

- Keep the empty consumed span on a UTF-8 boundary when an expression range is
  invalid.
- Diagnose invalid host workspace-analysis result batches before falling back
  to per-source analysis.
- Diagnose invalid custom LSP workspace results before falling back to
  per-document analysis.
- Diagnose custom single-document backend results tied to a different source
  snapshot and suppress their stale editor data.
- Report and suppress backend diagnostics with invalid or foreign source
  locations instead of silently dropping them.
- Filter import-path completions using the same portability and project-root
  checks as definition loading, and resolve candidates from the importer's
  canonical project key.
- Keep formatted diagnostic columns consistent with editor positions for CRLF
  line endings.
- Escape all reserved Rust keywords when deriving handler module names from
  construct names, including `become`, `box`, and `union`.
- Reject existing handler paths, including dangling symlinks, with a consistent
  already-exists diagnostic.
- Correct the typed-bindings dependency example and document local crate
  resolution before the current release line is published.
- Fully qualify generated typed-binding support types so language constructs
  can use names such as `Option`, `Result`, or `Diagnostic` without collisions.
- Track requested and canonical definition paths plus scoped symlink watches so
  Cargo regenerates typed bindings after in-project import changes without
  recursively watching the project root.

## [0.12.1] - 2026-11-12

### Fixed

- Refuse to follow destination symlinks when replacing CLI-managed files.

## [0.12.0] - 2026-11-12

### Added

- Add `CompletionInsertTextFormat` to completion candidates, with plain text as
  the `CompletionCandidate::new` default. Struct-literal initializers must set
  the new field. The LSP adapter sends snippets only to supporting clients.
- Demonstrate a host-authored, context-limited function declaration snippet in
  the Aster language server example.

### Fixed

- Keep invalid custom-lexer token-span diagnostics attached to the source,
  using the nearest valid UTF-8 offset.

## [0.11.0] - 2026-11-12

### Added

- Warn when a definition import or manifest root uses the legacy `.lm`
  extension; both remain loadable while users migrate to `.langmeld`.
- Keep project LSP file watching active for legacy `.lm` definitions during
  migration.

### Changed

- Explain portable filename requirements when a manifest or import path is
  rejected.

### Fixed

- Reject Unix filenames containing backslashes when mapping filesystem paths
  to portable project keys, preventing collisions with nested definition paths.
- Reject colon-containing definition paths, which are reserved on Windows and
  can select alternate data streams.
- Reject Windows-invalid definition path characters, trailing dots or spaces,
  and reserved device names so project paths remain portable across hosts.

## [0.10.0] - 2026-11-12

### Added

- `langmeld-build` generates typed Rust views of declarative constructs,
  captures, named optional groups, and repeated groups into Cargo's `OUT_DIR`.
- `ConstructContext::typed` lets handlers access a generated view of the
  current construct; `ParseResult::contains_construct` checks node membership,
  and the declarative-functions example uses generated views while lowering
  its AST.
- `.langmeld` and Rust grammars can assign names to syntax alternatives and
  optional/repeated groups. `ConstructSpec` retains alternative names, and
  parsed capture groups retain group names for tooling.
- `ParsedConstruct::capture_groups` preserves occurrence boundaries and nested
  structure for optional and repeated syntax groups while retaining the flat
  captured-field list.
- `ConstructContext::field_identifier_capture` returns an identifier's borrowed
  spelling and source span together for host-owned name resolution.
- `ConstructContext::field_text_capture` returns a capture's exact source
  spelling and span together for host-side decoding and diagnostics.
- `LanguageSpec::builder` and `LanguageSpec::into_builder` support Rust-authored
  grammars and extending a loaded declarative grammar with Rust constructs.
- The declarative starter can load a definition project from an explicit root
  directory as well as from its Cargo project directory.
- `langmeld init --typed-bindings` can scaffold a declarative project with
  generated typed capture views, build integration, and typed starter handlers.
- `langmeld init --local-crates` can generate an ignored Cargo patch config for
  using crates from a matching Langmeld source checkout without changing the
  generated registry-based manifest.
- `langmeld-project` can load a validated `LanguageSpec` directly while keeping
  invalid-definition diagnostics and their source snapshots in a typed error.

### Changed

- Declarative completion now suggests expected literal tokens after completed
  keywords, identifier fields, and finite choice values, while keeping invalid
  or incomplete input scoped to its valid grammar context.
- Typed handler scaffolds derive dispatch identity from their generated
  construct view, keeping registration and capture access tied to one declared
  construct.
- TextMate comment patterns now follow the built-in lexer's longest-opening
  rule, so overlapping line and block comment markers tokenize consistently.
- TextMate string highlighting now scopes a trailing backslash as an escape,
  matching the built-in lexer's support for escaped line breaks.
- TextMate number highlighting now follows identifier and integer token
  boundaries, including digits adjacent to identifier text.
- TextMate punctuation highlighting now follows the built-in lexer's grouped
  operator boundaries.
- **Breaking:** `ProjectManifest` now includes the `rust_bindings` field. Code
  constructing the struct directly must provide `RustBindings`; parsed
  manifests without the setting default to dynamic access.
- `langmeld check` now displays non-error definition diagnostics, and
  `langmeld-build` forwards definition warnings to Cargo.
- **Breaking:** Replace `HostLanguageBackend::new(parser).with_analyzer(analyzer)`
  with `HostLanguageBackend::new_with_analyzer(parser, analyzer)`. Choose the
  analyzer before attaching analysis-dependent providers so none can be
  silently discarded.
- **Breaking:** `PatternPart::Optional` now stores an optional group name and
  its parts, and `PatternPart::Repeat` stores an optional name. Existing
  constructors remain available; direct enum construction must add `name: None`
  or use the `optional`, `zero_or_more`, and `one_or_more` constructors.
- **Breaking:** `ParsedConstruct` includes `capture_groups`; update any direct
  struct initializers. Parser-produced constructs populate the field.
- **Breaking:** `LanguagePass` is now generic over the parser's complete
  source-bound result. Update custom-parser implementations from
  `LanguagePass<Tree, State>` to `LanguagePass<ParsedDocument<Tree>, State>`;
  declarative parsers can use `LanguagePass<ParseResult, State>` directly.
- **Breaking:** `DefinitionResolver` now has an associated `Error` type that
  preserves resolver failures for Rust callers; implementers must declare
  `type Error` and return `Result<DefinitionSource, Self::Error>`.
- **Breaking:** `ConstructHandlerRegistry::dispatch_construct` and
  `dispatch_block` now require the originating `ParseResult` and reject a
  selected node from another parse result.
- The declarative starter and toy language return structured diagnostics from
  program runs, leaving diagnostic presentation to the host and preserving
  output emitted before an execution failure.
- Generated language projects return typed project-loading errors and retain
  source-backed definition diagnostics for host-controlled presentation.

### Fixed

- Tree-sitter grammar export now accepts escaped line breaks in quoted strings,
  matching the built-in Langmeld lexer.
- Tree-sitter punctuation tokens now follow the built-in lexer's grouping rules.
- The project-definition LSP now drops its shared symbol and completion index
  when the configured root definition cannot be loaded, avoiding stale results.
- Construct generation now preserves CRLF line endings in managed Rust
  registration sections.
- Typed-binding errors now list every supported unnamed-group naming form and
  point to the dynamic API when generated views are not a fit.
- The generated declarative library no longer imports an unused diagnostic
  formatter.
- Typed binding generation no longer reserves `new` for internal view
  construction and reports top-level `from_construct` accessor conflicts.
- The declarative example and typed starter report binding-generation failures
  without panicking from their Cargo build scripts.

## [0.9.0] - 2026-11-11

### Changed

- **Breaking:** Standalone `SourceFile`s now receive unique snapshot identities,
  and source-associated parse, analysis, diagnostic-location, and backend
  results require those identities so mismatched snapshots are rejected
  consistently. Source-bound result types no longer implement `Default`.
- **Breaking:** `SourceIdentity::database_id()` is renamed to `namespace_id()`.
  Use `SourceFile::is_database_managed()` to distinguish database-managed
  sources from standalone snapshots.
- Generated starter binaries use Clap for standard help, version, and argument
  handling.
- Clarify how host projects provide interpretation or compilation after
  parsing.

### Fixed

- Preserve filesystem source identities across repeated reads of unchanged
  definition files so project navigation can resolve valid locations.

## [0.8.0] - 2026-11-11

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
- Replace existing language files atomically when formatting or generating
  constructs and handlers.

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

### Changed

- **Breaking:** Language definition files use the `.langmeld` extension instead
  of `.lm`. Rename definition files and update imports and the manifest's
  `definition` path; the manifest remains at `format = 1` and its root path
  must use `.langmeld`.

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
