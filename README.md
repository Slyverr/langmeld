# Langmeld

Langmeld is a Rust-first framework for building programming languages and
domain-specific languages. Describe language structure in modular `.langmeld`
definitions or Rust, then use Langmeld's parser and editor tooling with a Rust
host for language-specific behavior.

Langmeld is pre-1.0 and under active development. Its public APIs and generated
project files may change between minor releases.

## What it provides

- A generic lexer, parser, syntax tree, typed AST generation, and reusable
  semantic analysis.
- A CLI to initialize language projects, validate definitions and sources, and
  regenerate derived tooling.
- Rust host hooks for custom parsing, semantic analysis, interpretation, and
  compilation.
- Editor support through an LSP server, plus optional Tree-sitter and browser
  WebAssembly outputs.

See [current capabilities and boundaries](docs/overview.md) for details.

## Try it

To try the CLI from this checkout, use Rust 1.88 or newer:

```sh
cargo run -p langmeld-cli -- init
```

The interactive initializer creates a language project and its Rust host.
Choose its options in the terminal, then follow the generated project's
README. For a working example, see
[`examples/declarative-interpreter`](examples/declarative-interpreter).

## Documentation

- [Project setup and authoring](docs/project-authoring.md)
- [Editor and generated tooling](docs/editor-tooling.md)
- [Compatibility and versioning](docs/compatibility.md)
- [Examples and what they demonstrate](docs/examples.md)
- [Contributing and development checks](CONTRIBUTING.md)
- [Changelog](CHANGELOG.md)

## License

Licensed under either of these terms, at your option:

- [MIT](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)
