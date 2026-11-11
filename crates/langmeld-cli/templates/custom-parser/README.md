# {{LANGUAGE_NAME}}

A Rust-defined language parser built with Langmeld.

## Run

```sh
cargo run
cargo run -- path/to/program.say
cargo run -- --help
cargo test
```

Without a path, the CLI parses a small built-in sample. Pass a source file to
parse your own program. The parser, typed program tree, and source-spanned
statements live in `src/lib.rs`; the CLI entry point is `src/main.rs`, and
public behavior tests belong in `tests/`. This template uses Langmeld's
`LanguageParser` contract and leaves grammar design to your Rust code.

## Language server

Start the stdio LSP server with:

```sh
cargo run --bin language-lsp
```

It publishes parser diagnostics from the host parser. Add host analysis,
completion, and navigation by implementing Langmeld's corresponding traits
and attaching them to `HostLanguageBackend` in `src/bin/language-lsp.rs`.

## License

Licensed under either the MIT License or Apache License 2.0; choose either
license. Replace the copyright placeholder in `LICENSE-MIT` before distributing.
