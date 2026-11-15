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
Generated diagnostic codes use the Cargo package name as their prefix (for
example, `my-language` produces `MY-LANGUAGE-*`); adjust them to match your
language's diagnostic conventions.

## Language server

Start the stdio LSP server with:

```sh
cargo run --bin language-lsp
```

It publishes parser diagnostics from the host parser. Add host analysis,
completion, and navigation by implementing Langmeld's corresponding traits
and attaching them to `HostLanguageBackend` in `src/lsp.rs`. The binary in
`src/bin/language-lsp.rs` only starts that server.

## License

The initializer does not choose a license for this project. Add the SPDX
expression to `Cargo.toml` and include the corresponding license text before
sharing or publishing it.
