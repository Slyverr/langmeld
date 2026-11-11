# {{LANGUAGE_NAME}}

A small Rust-hosted language project built with Langmeld.

## Run

```sh
cargo run
cargo run -- path/to/program.widget
cargo test
```

The default program demonstrates string bindings and `print`. The language
definition starts at `language/main.langmeld` and imports modular construct
definitions from `language/constructs/`. Rust behavior lives in
`src/constructs/`; shared state and the runner live in `src/runtime.rs` and
`src/lib.rs`. Add public language behavior tests under `tests/`.

The generated `language::load()` helper finds the project definition from this
Cargo package. The CLI and Rust host share the same parser, handler registry,
and diagnostics.

## Editor support

With the Langmeld CLI installed, run `langmeld lsp` from this project directory
for syntax diagnostics and grammar-based completions. These features come from
the language definition; host-specific semantic checks and runtime behavior
are not inferred. Run `langmeld lsp --definitions` to get editor support while
editing `.langmeld` files.

## License

Licensed under either the MIT License or Apache License 2.0, at your option.
See `LICENSE-MIT` and `LICENSE-APACHE` for details.
