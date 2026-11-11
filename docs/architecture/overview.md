# Architecture overview

Langmeld separates reusable language-development infrastructure from
language-specific policy. The framework can parse and expose declared syntax,
but the language project owns semantics, execution, and any custom syntax or
behavior it needs.

## Crate responsibilities

| Crate | Responsibility |
| --- | --- |
| `langmeld-core` | Source snapshots and identities, byte spans, and structured diagnostics. |
| `langmeld-language` | Language grammar model, `.langmeld` frontend and validation, built-in lexer and parser, construct-handler dispatch, and host parser, analysis, completion, and navigation contracts. |
| `langmeld-project` | Project manifests and modular definition loading through directory, in-memory, and overlay resolvers. |
| `langmeld-expression` | Optional configurable expression grammar, parser, and host-defined type and evaluation hooks. |
| `langmeld-lsp` | LSP protocol adapter for declarative languages and Rust-hosted backends. |
| `langmeld-textmate` | TextMate grammar export for compatible declarative grammars. |
| `langmeld-tree-sitter` | Tree-sitter grammar source export for the supported declarative subset. |
| `langmeld-cli` | User-facing project commands that compose project loading, validation, formatting, editor services, and grammar export. |

The crates are independently usable where their APIs permit. For example,
applications can use an in-memory definition resolver without the CLI's
filesystem workflow, and a host can embed the LSP adapter without running the
Langmeld executable.

## Definition-to-tooling flow

For a declarative project, the main flow is:

1. `langmeld-project` reads `langmeld.toml` and resolves imported
   `.langmeld` sources.
2. `langmeld-language` parses and validates those definitions into a
   `LanguageSpec`.
3. The built-in parser uses that specification to parse source files into
   generic construct and field data, with source-located diagnostics.
4. A host may register construct handlers with its own state. The dispatcher
   provides validated field access, source-aware diagnostics, and explicit
   child dispatch; handlers decide how their language treats nested constructs.
5. Optional adapters use the same validated specification to provide grammar
   scoped LSP behavior or emit TextMate and Tree-sitter grammar artifacts.
6. The language's Rust host adds semantics and behavior that declarations
   cannot determine, such as name resolution, type checking, execution, or
   compilation.

Custom parser projects can implement `LanguageParser` and return a host-owned
tree instead of using the generic construct parser. Hosts can implement
`LanguageAnalyzer`, completion, and navigation contracts, or provide a
`LanguageBackend` directly when they need control over editor lifecycle and
workspace behavior. The framework adapts those callbacks to LSP; it does not
invent language-specific meaning from syntax alone.

## Choosing an extension point

- Use `.langmeld` for ordered token patterns, captures, alternatives, and
  delimited child blocks.
- Provide a custom lexer when the built-in token categories do not fit.
- Use `langmeld-expression` when its configurable expression model fits, or
  parse expression captures in host code.
- Implement `LanguageParser` for a different grammar strategy or a
  language-specific AST.
- Add host analysis and editor callbacks for semantic behavior. Implement
  `LanguageBackend` directly when the adapter contracts need more control.
- Use TextMate and Tree-sitter exporters only when their documented grammar
  subsets represent the language accurately.

See the [language definition guide](../language/definitions.md),
[editor integration guides](../editor/README.md), and
[project workflow](../guides/project-workflow.md) for usage details.
