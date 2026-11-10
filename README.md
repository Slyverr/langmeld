# Langmeld

Langmeld is an early-stage, Rust-first framework for describing language
syntax, parsing source, and building language-specific behavior in Rust.

The current libraries provide source snapshots, revision-aware diagnostics,
UTF-8/UTF-16/UTF-32 editor position conversion, a validated syntax model, and
a generic parser for token patterns, typed, generic, and finite-choice captures,
alternative forms, optional and repeated groups, and scoped blocks. The `.langmeld` frontend
is itself parsed by that generic parser. The framework does not prescribe
language-specific semantics, execution, or code generation; authors implement
those behaviors in Rust. Rust hosts can provide a custom
lexer while reusing the generic parser, or implement `LanguageParser` and keep
their own parse-tree type. A host can optionally implement `LanguageAnalyzer`
to return its own semantic result and diagnostics without adopting
framework-defined symbol or type rules, and `LanguageCompleter` to provide
completion candidates and `LanguageNavigator` to provide definition and
reference queries from an existing parse and optional analysis result. The
separate `langmeld-expression` crate provides a configurable, source-spanned
expression parser; typing and evaluation remain host-owned. Project
definitions can explicitly import local construct files through a host-provided
resolver. Declarative grammars also provide scoped syntax-token completion
candidates. The `langmeld-lsp` crate provides full-text LSP synchronization,
diagnostics, and completion for declarative languages, plus host-backed
definition, references, rename, formatting, hover, document symbols, and
semantic tokens through the same LSP adapter. The `langmeld lsp` CLI command
serves manifest-backed projects, and `langmeld lsp --definitions` assists with
editing `.langmeld` syntax with project validation, diagnostics, scoped
completion, and formatting.
`langmeld format` formats all definitions reachable through the project
manifest; pass a file path to format one file. The optional
`langmeld-textmate` crate exports lexical highlighting rules, and
`langmeld-tree-sitter` exports parser grammars for compatible declarative
syntax. The `langmeld-project` crate provides a directory-backed resolver for
modular `.langmeld` projects, with project-root confinement, manifest-backed
loading, and source snapshots for diagnostic display. The core, expression,
language, TextMate, and Tree-sitter crates compile for `wasm32-unknown-unknown`;
Langmeld does not yet provide browser bindings or a browser editor adapter.

## Requirements

- Rust 1.88 or newer

Nix users can enter the development shell with `nix develop` and build the CLI
with `nix build`. With direnv installed, run `direnv allow` once to activate
the checked-in `.envrc`.

## Try the example

```sh
cargo check --workspace --locked
cargo run -p langmeld-aster-language-example --locked
cargo run -p langmeld-toy-language-example --locked
cargo run -p langmeld-custom-lexer-example --locked
cargo run -p langmeld-custom-parser-example --locked
cargo run -p langmeld-schema-language-example --locked
```

The [`toy-language`](examples/toy-language) example shows a multi-file
`.langmeld` definition project and nested constructs. The
[`custom-lexer`](examples/custom-lexer) example composes a `.langmeld`
grammar with Rust-defined raw-string and comment tokenization.
The [`custom-parser`](examples/custom-parser) example combines a Rust-owned
declaration parser and runtime with `langmeld-expression` for operator
precedence and expression trees.
The [`aster-language`](examples/aster-language) example adds a host-owned AST,
scope-aware name resolution, basic type checks, and navigation that follows
resolved references through shadowed lexical scopes.
The [`schema-language`](examples/schema-language) example uses a global type
namespace with forward and recursive references, then completes field types
from the existing semantic result through `LanguageCompleter` and navigates
those references through `LanguageNavigator`. It also provides a standalone
LSP binary assembled with `HostLanguageBackend`, including type resolution,
completion, definition, and references across open files.

Create a starter language project with
`cargo run -p langmeld-cli -- init my-language`. The initializer prompts for
project and language names, then lets you choose a declarative `.langmeld`
grammar or a custom Rust parser. Validate declarative manifests and definitions with
`cargo run -p langmeld-cli -- check --manifest-path my-language/langmeld.toml`.
Generate lexical highlighting JSON with:

```sh
cargo run -p langmeld-cli -- generate textmate \
  --manifest-path my-language/langmeld.toml \
  --scope-name source.my_language
```

Generate a Tree-sitter `grammar.js` file with:

```sh
cargo run -p langmeld-cli -- generate tree-sitter \
  --manifest-path my-language/langmeld.toml \
  --grammar-name my_language
```
See [Project workflow](docs/project-workflow.md) for generated files and
non-interactive options.

See [Language definitions](docs/language-definitions.md) for the current
`.langmeld` syntax and its extension boundaries.

## License

Licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT license](LICENSE-MIT), at your option.
