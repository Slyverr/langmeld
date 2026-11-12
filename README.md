# Langmeld

Langmeld is a Rust-first framework for building programming languages and
domain-specific languages. Describe token-based syntax in modular `.langmeld`
files, or use a Rust parser when the syntax needs more control. Language
semantics, execution, and code generation stay in the host project.

The framework provides source spans and diagnostics, parsing and analysis
interfaces, a project CLI, and editor integrations. TextMate and Tree-sitter
grammars can be generated for compatible declarative definitions.
Declarative projects can opt into compile-time checked Rust views for named
captures and groups with `langmeld init --typed-bindings`; see [typed Rust
bindings](docs/language/typed-bindings.md).

## Try it

Langmeld requires Rust 1.88 or newer. From this repository, create and check a
starter language project:

```sh
cargo run -p langmeld-cli -- init my-language
cargo run -p langmeld-cli -- check \
  --manifest-path my-language/langmeld.toml
```

The declarative starter runs a small language with `let` bindings and `print`.
Add a construct with `langmeld generate construct <name>`; it creates a syntax
module, imports it from the language entry point, and registers a Rust handler.
The generated handler starts with a diagnostic until its behavior is
implemented.

Browse the [documentation](docs/README.md) for project setup, language
definitions, editor integration, and the framework architecture.

## Examples

See the [example guide](examples/README.md) to choose an example by the Langmeld
workflow or extension point you want to explore.

## License

Licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT license](LICENSE-MIT), at your option.
