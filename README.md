# Langmeld

Langmeld is an early-stage, Rust-first framework for describing language
syntax, parsing source, and building language-specific behavior in Rust.

The current libraries provide source snapshots, revision-aware diagnostics, a
validated syntax model, and a generic parser for token patterns, typed and
generic captures, alternative forms, optional and repeated groups, and scoped
blocks. The `.lm` frontend is
itself parsed by that generic parser. The parser does not yet provide
expressions, type checking, execution, code generation, or editor integrations;
language authors own those behaviors in Rust. Rust hosts can provide a custom
lexer while reusing the generic parser, or implement `LanguageParser` and keep
their own parse-tree type. Arbitrary host trees do not receive automatic editor
tooling yet. The separate `langmeld-expression` crate provides a configurable,
source-spanned expression parser; typing and evaluation remain host-owned.
Project definitions can explicitly import local construct files through a
host-provided resolver.

## Requirements

- Rust 1.88 or newer

## Try the example

```sh
cargo check --workspace --locked
cargo run -p langmeld-aster-language-example --locked
cargo run -p langmeld-toy-language-example --locked
cargo run -p langmeld-custom-lexer-example --locked
cargo run -p langmeld-custom-parser-example --locked
```

The [`toy-language`](examples/toy-language) example shows a multi-file `.lm`
definition project and nested constructs. The
[`custom-lexer`](examples/custom-lexer) example shows host-defined raw strings
and comment skipping.
The [`custom-parser`](examples/custom-parser) example combines a Rust-owned
declaration parser and runtime with `langmeld-expression` for operator
precedence and expression trees.
The [`aster-language`](examples/aster-language) example adds a host-owned AST,
scope-aware name resolution, and basic type checks.

See [Language definitions](docs/language-definitions.md) for the current `.lm`
syntax and its extension boundaries.

## License

Licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT license](LICENSE-MIT), at your option.
