# {{LANGUAGE_NAME}}

A small Rust-hosted language project built with Langmeld.
{{TYPED_BINDINGS_GUIDANCE}}
## Run

```sh
cargo run
cargo run -- path/to/program.widget
cargo run -- --help
cargo test
```

The default program demonstrates string bindings and `print`. The language
definition starts at `language/main.langmeld` and imports modular construct
definitions from `language/constructs/`. Rust behavior lives in
`src/constructs/`; shared state and the runner live in `src/runtime.rs` and
`src/lib.rs`. Add public language behavior tests under `tests/`.
Generated diagnostic codes use the Cargo package name as their prefix (for
example, `my-language` produces `MY-LANGUAGE-*`); adjust them to match your
language's diagnostic conventions.

Load the definition once with `language::load()`, then reuse the returned
`Language` to parse or run source files. Its `specification()` method exposes
the validated grammar for editor integrations. Each run gets fresh runtime
state and handler instances. `Language::run` returns output and structured
diagnostics; the CLI formats them for the terminal, while other hosts can
present them in their own way. A failed run also retains output emitted before
the failure. `language::load()` returns typed project errors or definition
diagnostics with their source snapshots, so hosts can format or inspect them.
It loads the project containing the generated Cargo package; use
`language::load_from(project_root)` when the definition project is elsewhere.

## Add a construct

From the project root, generate a syntax declaration and Rust handler:

```sh
langmeld generate construct repeat
```

This adds `language/constructs/repeat.langmeld`, imports it from
`language/main.langmeld`, and creates and registers
`src/constructs/repeat.rs`. Edit the declaration to describe the construct's
syntax, then implement its language-specific behavior in the handler. The
generated handler reports a diagnostic until that behavior is implemented.
Run `langmeld check` to validate the language definition and `cargo test` to
verify the Rust behavior.

## Editor support

With the Langmeld CLI installed, run `langmeld lsp` from this project directory
for syntax diagnostics and grammar-based completions. These features come from
the language definition; host-specific semantic checks and runtime behavior
are not inferred. Run `langmeld lsp --definitions` to get editor support while
editing `.langmeld` files.

## License

Licensed under either the MIT License or Apache License 2.0, at your option.
See `LICENSE-MIT` and `LICENSE-APACHE` for details.
