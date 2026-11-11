# Langmeld

Langmeld is a Rust-first framework for building programming languages and
domain-specific languages. Describe token-based syntax in modular `.langmeld`
files, or use a Rust parser when the syntax needs more control. Language
semantics, execution, and code generation stay in the host project.

The framework provides source spans and diagnostics, parsing and analysis
interfaces, a project CLI, and editor integrations. TextMate and Tree-sitter
grammars can be generated for compatible declarative definitions.

## Try it

Langmeld requires Rust 1.88 or newer. From this repository, create and check a
starter language project:

```sh
cargo run -p langmeld-cli -- init my-language
cargo run -p langmeld-cli -- check \
  --manifest-path my-language/langmeld.toml
```

Browse the [documentation](docs/README.md) for project setup, language
definitions, editor integration, and the framework architecture.

## Examples

- [Toy language](examples/toy-language): modular declarative syntax.
- [Aster language](examples/aster-language): a Rust parser and host-owned
  analysis.
- [Schema language](examples/schema-language): cross-file types and an LSP
  backend.

## License

Licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT license](LICENSE-MIT), at your option.
